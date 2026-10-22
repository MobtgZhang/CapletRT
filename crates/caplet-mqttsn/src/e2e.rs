//! End-to-end MQTT-SN flow against an in-process mock gateway.
//!
//! The gateway parses every packet emitted by the client and generates the
//! corresponding protocol reply. This exercises the real wire encoding rather
//! than injecting pre-baked replies.

use alloc::string::String;
use alloc::vec::Vec;

use crate::{msgtype, Client, ClientState, Will};

/// A stateful MQTT-SN mock gateway.
#[derive(Debug, Default)]
pub struct MockBroker {
    /// Packets waiting to be delivered to the client.
    pub rx: Vec<Vec<u8>>,
    /// Packets emitted by the client.
    pub tx: Vec<Vec<u8>>,
    connected: bool,
    topic_id: u16,
    topic_name: String,
    publish_topic_id: u16,
    publish_msg_id: u16,
    publish_qos: u8,
    publish_dup: bool,
    publish_retain: bool,
    published_payload: Vec<u8>,
    will_topic: String,
    will_payload: Vec<u8>,
}

impl MockBroker {
    /// Build a new empty gateway.
    pub fn new() -> Self {
        Self {
            topic_id: 0x1234,
            ..Self::default()
        }
    }

    /// Deliver a synthetic packet to the client.
    pub fn inject(&mut self, pkt: Vec<u8>) {
        self.rx.push(pkt);
    }

    /// Take the next synthetic inbound packet.
    pub fn take(&mut self) -> Option<Vec<u8>> {
        self.rx.pop()
    }

    /// Process a client packet and return the gateway response, if any.
    pub fn exchange(&mut self, pkt: &[u8]) -> Option<Vec<u8>> {
        self.tx.push(Vec::from(pkt));
        let decoded = crate::decode(pkt)?.0;
        match decoded.msg_type {
            msgtype::SEARCHGW => {
                let mut body = Vec::from([1u8]);
                body.extend_from_slice(&[0, 0, 0, 0]);
                Some(crate::encode_packet(msgtype::GWINFO, &body))
            }
            msgtype::CONNECT => {
                if decoded.body.len() < 4 {
                    return None;
                }
                let has_will = decoded.body[0] & 0x08 != 0;
                if has_will {
                    Some(crate::encode_packet(msgtype::WILLTOPICREQ, &[]))
                } else {
                    self.connected = true;
                    Some(crate::encode_packet(msgtype::CONNACK, &[0]))
                }
            }
            msgtype::WILLTOPIC => {
                if decoded.body.is_empty() {
                    return None;
                }
                self.will_topic = String::from_utf8_lossy(&decoded.body[1..]).into_owned();
                Some(crate::encode_packet(msgtype::WILLMSGREQ, &[]))
            }
            msgtype::WILLMSG => {
                self.will_payload = Vec::from(decoded.body);
                self.connected = true;
                Some(crate::encode_packet(msgtype::CONNACK, &[0]))
            }
            msgtype::REGISTER => {
                if decoded.body.len() < 4 {
                    return None;
                }
                let msg_id = u16::from_be_bytes([decoded.body[2], decoded.body[3]]);
                self.topic_name = String::from_utf8_lossy(&decoded.body[4..]).into_owned();
                let mut body = Vec::with_capacity(5);
                body.extend_from_slice(&self.topic_id.to_be_bytes());
                body.extend_from_slice(&msg_id.to_be_bytes());
                body.push(0);
                Some(crate::encode_packet(msgtype::REGACK, &body))
            }
            msgtype::PUBLISH => {
                if decoded.body.len() < 3 {
                    return None;
                }
                let flags = decoded.body[0];
                let qos = (flags >> 5) & 0x03;
                let publish_body = &decoded.body[1..];
                if publish_body.len() < 2 {
                    return None;
                }
                self.publish_topic_id = u16::from_be_bytes([publish_body[0], publish_body[1]]);
                self.publish_qos = qos;
                self.publish_dup = flags & 0x80 != 0;
                self.publish_retain = flags & 0x10 != 0;
                let mut payload_off = 2;
                if qos > 0 {
                    if publish_body.len() < 4 {
                        return None;
                    }
                    self.publish_msg_id = u16::from_be_bytes([publish_body[2], publish_body[3]]);
                    payload_off = 4;
                }
                self.published_payload = Vec::from(&publish_body[payload_off..]);
                if qos == 1 {
                    let mut body = Vec::with_capacity(5);
                    body.extend_from_slice(&self.publish_topic_id.to_be_bytes());
                    body.extend_from_slice(&self.publish_msg_id.to_be_bytes());
                    body.push(0);
                    Some(crate::encode_packet(msgtype::PUBACK, &body))
                } else {
                    None
                }
            }
            msgtype::DISCONNECT => {
                self.connected = false;
                None
            }
            _ => None,
        }
    }

    /// Whether the gateway completed CONNECT.
    pub fn is_connected(&self) -> bool {
        self.connected
    }

    /// Registered topic name.
    pub fn topic_name(&self) -> &str {
        &self.topic_name
    }

    /// Most recent published payload.
    pub fn published_payload(&self) -> &[u8] {
        &self.published_payload
    }

    /// QoS of the most recent publish.
    pub fn publish_qos(&self) -> u8 {
        self.publish_qos
    }

    /// DUP flag of the most recent publish.
    pub fn publish_dup(&self) -> bool {
        self.publish_dup
    }

    /// Retain flag of the most recent publish.
    pub fn publish_retain(&self) -> bool {
        self.publish_retain
    }

    /// Most recent publish topic id.
    pub fn publish_topic_id(&self) -> u16 {
        self.publish_topic_id
    }

    /// Most recent publish message id.
    pub fn publish_msg_id(&self) -> u16 {
        self.publish_msg_id
    }

    /// Last will topic received during CONNECT.
    pub fn will_topic(&self) -> &str {
        &self.will_topic
    }

    /// Last will payload received during CONNECT.
    pub fn will_payload(&self) -> &[u8] {
        &self.will_payload
    }

    /// Number of packets emitted by the client.
    pub fn packet_count(&self) -> usize {
        self.tx.len()
    }
}

/// Run the full client lifecycle against `broker` and return the final state
/// and assigned topic id.
pub fn run_end_to_end(broker: &mut MockBroker, value: f32) -> (ClientState, u16) {
    let mut c = Client::new();

    let search = Client::build_searchgw(1);
    let reply = broker.exchange(&search).expect("GWINFO");
    let (decoded, _) = crate::decode(&reply).expect("valid GWINFO");
    assert_eq!(decoded.msg_type, msgtype::GWINFO);
    c.state = ClientState::Connecting;

    let connect = c.begin_connect("dev1", 60, true);
    let reply = broker.exchange(&connect).expect("CONNACK");
    assert_eq!(c.apply_connack(&reply), Some(crate::ReturnCode::Accepted));

    let topic_name = String::from("sensors/temp");
    let reg = Client::build_register_with_msg_id(0, 1, &topic_name);
    let reply = broker.exchange(&reg).expect("REGACK");
    assert_eq!(c.apply_regack(&reply), Some(crate::ReturnCode::Accepted));

    c.qos = 1;
    let payload = format_value(value);
    let publish = c.publish(payload.as_bytes(), false, 0).expect("publish");
    let reply = broker.exchange(&publish).expect("PUBACK");
    assert_eq!(c.apply_puback(&reply), Some(crate::ReturnCode::Accepted));

    let _ = broker.exchange(&Client::build_disconnect());
    (c.state, c.topic_id)
}

/// Run a complete flow including the gateway-initiated last-will handshake.
pub fn run_end_to_end_with_will(
    broker: &mut MockBroker,
    will_topic: &str,
    will_payload: &[u8],
    value: f32,
) -> (ClientState, u16) {
    let mut c = Client::new();
    c.will = Some(Will::new(will_topic, will_payload));

    let connect = c.begin_connect("dev1", 60, true);
    let request = broker.exchange(&connect).expect("WILLTOPICREQ");
    let willtopic = c.handle_willtopicreq(&request).expect("WILLTOPIC");
    let request = broker.exchange(&willtopic).expect("WILLMSGREQ");
    let willmsg = c.handle_willmsgreq(&request).expect("WILLMSG");
    let reply = broker.exchange(&willmsg).expect("CONNACK");
    assert_eq!(c.apply_connack(&reply), Some(crate::ReturnCode::Accepted));

    let reg = Client::build_register_with_msg_id(0, 2, "sensors/temp");
    let reply = broker.exchange(&reg).expect("REGACK");
    c.apply_regack(&reply).expect("REGACK accepted");
    c.qos = 1;
    let payload = format_value(value);
    let publish = c.publish(payload.as_bytes(), true, 0).expect("publish");
    let reply = broker.exchange(&publish).expect("PUBACK");
    c.apply_puback(&reply).expect("PUBACK accepted");
    (c.state, c.topic_id)
}

fn format_value(v: f32) -> String {
    let mut s = String::new();
    s.push('t');
    s.push('=');
    let whole = v as i32;
    let frac = ((v - whole as f32) * 100.0).abs() as u32;
    if v < 0.0 {
        s.push('-');
    }
    s.push_str(alloc::format!("{}", whole).as_str());
    s.push('.');
    if frac < 10 {
        s.push('0');
    }
    s.push_str(alloc::format!("{}", frac).as_str());
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn e2e_publish_flow_parses_wire_packets() {
        let mut b = MockBroker::new();
        let (state, topic_id) = run_end_to_end(&mut b, 22.5);
        assert_eq!(state, ClientState::Active);
        assert_eq!(topic_id, 0x1234);
        assert_eq!(b.topic_name(), "sensors/temp");
        assert_eq!(b.topic_name(), "sensors/temp");
        assert_eq!(b.published_payload(), b"t=22.50");
        assert_eq!(b.publish_qos(), 1);
        assert_eq!(b.publish_topic_id(), 0x1234);
        assert_ne!(b.publish_msg_id(), 0);
        assert_eq!(b.packet_count(), 5);
    }

    #[test]
    fn e2e_will_handshake() {
        let mut b = MockBroker::new();
        let (state, _) = run_end_to_end_with_will(&mut b, "dev/status", b"offline", 3.0);
        assert_eq!(state, ClientState::Active);
        assert_eq!(b.will_topic(), "dev/status");
        assert_eq!(b.will_payload(), b"offline");
        assert!(b.publish_retain());
    }

    #[test]
    fn connect_rejected_keeps_state() {
        let mut c = Client::new();
        let ack = alloc::vec![3, msgtype::CONNACK, 1];
        let rc = c.apply_connack(&ack);
        assert_eq!(rc, Some(crate::ReturnCode::RejectedCongestion));
        assert_ne!(c.state, ClientState::Active);
    }
}
