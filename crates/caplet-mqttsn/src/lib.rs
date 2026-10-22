//! MQTT-SN v1.2 client encoding, decoding and QoS 1 state machine.
//!
//! The implementation supports the packets needed by the industrial demo
//! (`SEARCHGW`, `CONNECT`, `REGISTER`, `PUBLISH`, `DISCONNECT`) as well as
//! the gateway-initiated last-will handshake and QoS 1 delivery with
//! retransmission.  It intentionally does not implement QoS 2 exactly-once
//! delivery.

#![no_std]
#![deny(missing_docs)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

/// End-to-end MQTT-SN test against a mock broker.
pub mod e2e;

/// Network-resilience (drop + retry) test for the MQTT-SN
/// client. See docv1.md §20 (Phase 10.3.2).
pub mod resilience;

/// MQTT-SN control packet types.
#[allow(missing_docs)]
pub mod msgtype {
    /// Advertise.
    pub const ADVERTISE: u8 = 0x00;
    /// Search gateway.
    pub const SEARCHGW: u8 = 0x01;
    /// Gateway info.
    pub const GWINFO: u8 = 0x02;
    /// Connect.
    pub const CONNECT: u8 = 0x04;
    /// Connect ack.
    pub const CONNACK: u8 = 0x05;
    /// Will topic request.
    pub const WILLTOPICREQ: u8 = 0x06;
    /// Will topic.
    pub const WILLTOPIC: u8 = 0x07;
    /// Will message request.
    pub const WILLMSGREQ: u8 = 0x08;
    /// Will message.
    pub const WILLMSG: u8 = 0x09;
    /// Register.
    pub const REGISTER: u8 = 0x0A;
    /// Register ack.
    pub const REGACK: u8 = 0x0B;
    /// Publish.
    pub const PUBLISH: u8 = 0x0C;
    /// Publish ack.
    pub const PUBACK: u8 = 0x0D;
    /// Subscribe.
    pub const SUBSCRIBE: u8 = 0x12;
    /// Subscribe ack.
    pub const SUBACK: u8 = 0x13;
    /// Unsubscribe.
    pub const UNSUBSCRIBE: u8 = 0x14;
    /// Unsubscribe ack.
    pub const UNSUBACK: u8 = 0x15;
    /// Ping request.
    pub const PINGREQ: u8 = 0x16;
    /// Ping response.
    pub const PINGRESP: u8 = 0x17;
    /// Disconnect.
    pub const DISCONNECT: u8 = 0x18;
}

/// Default retransmission interval for QoS 1 publishes and handshake packets.
pub const DEFAULT_RETRY_INTERVAL_MS: u32 = 1_000;

/// Return code for `CONNACK` / `REGACK` / `PUBACK` / `SUBACK`.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReturnCode {
    /// Accepted.
    Accepted = 0,
    /// Rejected: congestion.
    RejectedCongestion = 1,
    /// Rejected: invalid topic id.
    RejectedTopicId = 2,
    /// Rejected: not supported.
    NotSupported = 3,
}

impl ReturnCode {
    fn from_byte(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Accepted),
            1 => Some(Self::RejectedCongestion),
            2 => Some(Self::RejectedTopicId),
            3 => Some(Self::NotSupported),
            _ => None,
        }
    }
}

/// MQTT-SN client state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientState {
    /// Awaiting a `GWINFO` reply to `SEARCHGW`.
    Searching,
    /// Awaiting `CONNACK`.
    Connecting,
    /// Gateway requested the will topic.
    AwaitingWillTopic,
    /// Gateway requested the will message.
    AwaitingWillMsg,
    /// Connected.
    Active,
    /// Asleep (QoS -1).
    Asleep,
    /// Disconnected.
    Disconnected,
}

/// Last-will registration sent during the CONNECT handshake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Will {
    /// Will topic name.
    pub topic: String,
    /// Will payload.
    pub payload: Vec<u8>,
    /// Will QoS (0 or 1).
    pub qos: u8,
    /// Retain the will message at the gateway.
    pub retain: bool,
}

impl Will {
    /// Construct a QoS 0, non-retained will.
    pub fn new(topic: &str, payload: &[u8]) -> Self {
        Self {
            topic: String::from(topic),
            payload: Vec::from(payload),
            qos: 0,
            retain: false,
        }
    }
}

#[derive(Debug)]
struct PendingPublish {
    msg_id: u16,
    topic_id: u16,
    payload: Vec<u8>,
    qos: u8,
    retain: bool,
    attempts: u32,
    last_sent_ms: u32,
}

/// An MQTT-SN client. The client owns message-id allocation and the
/// in-flight QoS 1 publish needed for retransmission.
#[derive(Debug)]
pub struct Client {
    /// Client state.
    pub state: ClientState,
    /// Topic id assigned by the gateway on `REGISTER`.
    pub topic_id: u16,
    /// Default QoS used by [`Client::publish`] (0 or 1).
    pub qos: u8,
    /// Optional last-will registration.
    pub will: Option<Will>,
    next_msg_id: u16,
    pending_publish: Option<PendingPublish>,
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

impl Client {
    /// Construct a new client in the `Searching` state.
    pub fn new() -> Self {
        Self {
            state: ClientState::Searching,
            topic_id: 0,
            qos: 0,
            will: None,
            next_msg_id: 1,
            pending_publish: None,
        }
    }

    /// Set or clear the last-will registration used by the next CONNECT.
    pub fn set_will(&mut self, will: Option<Will>) {
        self.will = will;
    }

    /// Allocate the next non-zero MQTT-SN message id.
    pub fn next_message_id(&mut self) -> u16 {
        let id = self.next_msg_id;
        self.next_msg_id = self.next_msg_id.wrapping_add(1);
        if self.next_msg_id == 0 {
            self.next_msg_id = 1;
        }
        id
    }

    /// Build a `SEARCHGW` packet with the given broadcast radius.
    pub fn build_searchgw(radius: u8) -> Vec<u8> {
        encode_packet(msgtype::SEARCHGW, &[radius])
    }

    /// Build a `CONNECT` packet without a will.
    pub fn build_connect(client_id: &str, duration: u16, clean: bool) -> Vec<u8> {
        Self::build_connect_with_will(client_id, duration, clean, false)
    }

    /// Build a `CONNECT` packet and select whether a will handshake follows.
    pub fn build_connect_with_will(
        client_id: &str,
        duration: u16,
        clean: bool,
        has_will: bool,
    ) -> Vec<u8> {
        let mut body = Vec::with_capacity(4 + client_id.len());
        let mut flags = 0u8;
        if has_will {
            flags |= 0x08;
        }
        if clean {
            flags |= 0x04;
        }
        body.push(flags);
        body.push(0x01); // MQTT-SN protocol id
        body.extend_from_slice(&duration.to_be_bytes());
        body.extend_from_slice(client_id.as_bytes());
        encode_packet(msgtype::CONNECT, &body)
    }

    /// Begin a connection and remember the will used by the handshake.
    pub fn begin_connect(&mut self, client_id: &str, duration: u16, clean: bool) -> Vec<u8> {
        self.state = ClientState::Connecting;
        Self::build_connect_with_will(client_id, duration, clean, self.will.is_some())
    }

    /// Build a `REGISTER` packet. The client-side default message id is 1;
    /// use [`Client::build_register_with_msg_id`] when tracking a reply.
    pub fn build_register(topic_id: u16, topic_name: &str) -> Vec<u8> {
        Self::build_register_with_msg_id(topic_id, 1, topic_name)
    }

    /// Build a `REGISTER` packet with an explicit message id.
    pub fn build_register_with_msg_id(topic_id: u16, msg_id: u16, topic_name: &str) -> Vec<u8> {
        let mut body = Vec::with_capacity(4 + topic_name.len());
        body.extend_from_slice(&topic_id.to_be_bytes());
        body.extend_from_slice(&msg_id.to_be_bytes());
        body.extend_from_slice(topic_name.as_bytes());
        encode_packet(msgtype::REGISTER, &body)
    }

    /// Build a `PUBLISH` packet (QoS 0, no retain, no duplicate flag).
    pub fn build_publish(topic_id: u16, payload: &[u8]) -> Vec<u8> {
        Self::build_publish_full(topic_id, payload, 0, 0, false, false)
    }

    /// Build a `PUBLISH` packet with explicit QoS, message id and flags.
    pub fn build_publish_full(
        topic_id: u16,
        payload: &[u8],
        qos: u8,
        msg_id: u16,
        retain: bool,
        dup: bool,
    ) -> Vec<u8> {
        let qos = qos.min(2);
        let mut flags = qos << 5;
        if retain {
            flags |= 0x10;
        }
        if dup {
            flags |= 0x80;
        }
        let mut body = Vec::with_capacity(4 + payload.len());
        body.push(flags);
        body.extend_from_slice(&topic_id.to_be_bytes());
        if qos > 0 {
            body.extend_from_slice(&msg_id.to_be_bytes());
        }
        body.extend_from_slice(payload);
        encode_packet(msgtype::PUBLISH, &body)
    }

    /// Build a `WILLTOPIC` packet for a will registration.
    pub fn build_willtopic(will: &Will) -> Vec<u8> {
        let mut flags = will.qos.min(1) << 5;
        if will.retain {
            flags |= 0x10;
        }
        let mut body = Vec::with_capacity(1 + will.topic.len());
        body.push(flags);
        body.extend_from_slice(will.topic.as_bytes());
        encode_packet(msgtype::WILLTOPIC, &body)
    }

    /// Build a `WILLMSG` packet for a will registration.
    pub fn build_willmsg(payload: &[u8]) -> Vec<u8> {
        encode_packet(msgtype::WILLMSG, payload)
    }

    /// Build a `DISCONNECT` packet.
    pub fn build_disconnect() -> Vec<u8> {
        encode_packet(msgtype::DISCONNECT, &[])
    }

    /// Build a `PINGREQ` packet.
    pub fn build_pingreq() -> Vec<u8> {
        encode_packet(msgtype::PINGREQ, &[])
    }

    /// Apply an incoming `CONNACK` packet. Returns the gateway return code.
    pub fn apply_connack(&mut self, pkt: &[u8]) -> Option<ReturnCode> {
        let decoded = decode(pkt)?.0;
        if decoded.msg_type != msgtype::CONNACK || decoded.body.is_empty() {
            return None;
        }
        let rc = ReturnCode::from_byte(decoded.body[0])?;
        if rc == ReturnCode::Accepted {
            self.state = ClientState::Active;
        }
        Some(rc)
    }

    /// Apply an incoming `REGACK` packet and store the assigned topic id.
    pub fn apply_regack(&mut self, pkt: &[u8]) -> Option<ReturnCode> {
        let decoded = decode(pkt)?.0;
        if decoded.msg_type != msgtype::REGACK || decoded.body.len() < 5 {
            return None;
        }
        let topic_id = u16::from_be_bytes([decoded.body[0], decoded.body[1]]);
        let rc = ReturnCode::from_byte(decoded.body[4])?;
        if rc == ReturnCode::Accepted {
            self.topic_id = topic_id;
        }
        Some(rc)
    }

    /// Answer a gateway `WILLTOPICREQ` with the configured will topic.
    pub fn handle_willtopicreq(&mut self, pkt: &[u8]) -> Option<Vec<u8>> {
        let decoded = decode(pkt)?.0;
        if decoded.msg_type != msgtype::WILLTOPICREQ {
            return None;
        }
        let will = self.will.as_ref()?;
        self.state = ClientState::AwaitingWillMsg;
        Some(Self::build_willtopic(will))
    }

    /// Answer a gateway `WILLMSGREQ` with the configured will payload.
    pub fn handle_willmsgreq(&mut self, pkt: &[u8]) -> Option<Vec<u8>> {
        let decoded = decode(pkt)?.0;
        if decoded.msg_type != msgtype::WILLMSGREQ {
            return None;
        }
        let will = self.will.as_ref()?;
        self.state = ClientState::Connecting;
        Some(Self::build_willmsg(&will.payload))
    }

    /// Publish to the registered topic. Returns `None` when the client is not
    /// active or QoS 2 is requested (QoS 2 is not implemented).
    pub fn publish(&mut self, payload: &[u8], retain: bool, now_ms: u32) -> Option<Vec<u8>> {
        if self.state != ClientState::Active || self.qos > 1 || self.topic_id == 0 {
            return None;
        }
        let qos = self.qos;
        let msg_id = if qos > 0 { self.next_message_id() } else { 0 };
        let pkt = Self::build_publish_full(self.topic_id, payload, qos, msg_id, retain, false);
        self.pending_publish = if qos == 1 {
            Some(PendingPublish {
                msg_id,
                topic_id: self.topic_id,
                payload: Vec::from(payload),
                qos,
                retain,
                attempts: 1,
                last_sent_ms: now_ms,
            })
        } else {
            None
        };
        Some(pkt)
    }

    /// Apply a `PUBACK`, completing the in-flight QoS 1 publish.
    pub fn apply_puback(&mut self, pkt: &[u8]) -> Option<ReturnCode> {
        let decoded = decode(pkt)?.0;
        if decoded.msg_type != msgtype::PUBACK || decoded.body.len() < 5 {
            return None;
        }
        let topic_id = u16::from_be_bytes([decoded.body[0], decoded.body[1]]);
        let msg_id = u16::from_be_bytes([decoded.body[2], decoded.body[3]]);
        let rc = ReturnCode::from_byte(decoded.body[4])?;
        let pending = self.pending_publish.as_ref()?;
        if pending.topic_id != topic_id || pending.msg_id != msg_id {
            return None;
        }
        self.pending_publish = None;
        Some(rc)
    }

    /// Return the in-flight QoS 1 message id, if any.
    pub fn pending_message_id(&self) -> Option<u16> {
        self.pending_publish.as_ref().map(|p| p.msg_id)
    }

    /// Return the number of transmissions made for the in-flight publish.
    pub fn publish_attempts(&self) -> u32 {
        self.pending_publish
            .as_ref()
            .map_or(0, |pending| pending.attempts)
    }

    /// Produce a DUP retransmission when the configured retry interval has
    /// elapsed. `now_ms` is a monotonic millisecond counter.
    pub fn retry_due(&mut self, now_ms: u32) -> Option<Vec<u8>> {
        let pending = self.pending_publish.as_mut()?;
        if now_ms.wrapping_sub(pending.last_sent_ms) < DEFAULT_RETRY_INTERVAL_MS {
            return None;
        }
        pending.last_sent_ms = now_ms;
        pending.attempts = pending.attempts.saturating_add(1);
        Some(Self::build_publish_full(
            pending.topic_id,
            &pending.payload,
            pending.qos,
            pending.msg_id,
            pending.retain,
            true,
        ))
    }
}

/// Decoded MQTT-SN message.
#[derive(Debug, Clone)]
pub struct Decoded<'a> {
    /// Message type.
    pub msg_type: u8,
    /// Payload after the type byte.
    pub body: &'a [u8],
}

/// Encode a packet body using the MQTT-SN one- or three-byte length header.
///
/// The three-byte form is used when the total packet exceeds 255 bytes. Its
/// 16-bit length includes the three length bytes themselves.
///
/// # Panics
///
/// Panics if the resulting packet cannot be represented by the 16-bit
/// extended length field.
pub fn encode_packet(msg_type: u8, body: &[u8]) -> Vec<u8> {
    try_encode_packet(msg_type, body).expect("MQTT-SN packet exceeds 65535 bytes")
}

/// Fallible form of [`encode_packet`].
pub fn try_encode_packet(msg_type: u8, body: &[u8]) -> Option<Vec<u8>> {
    let short_len = body.len().checked_add(2)?;
    if short_len <= u8::MAX as usize {
        let mut out = Vec::with_capacity(short_len);
        out.push(short_len as u8);
        out.push(msg_type);
        out.extend_from_slice(body);
        Some(out)
    } else {
        let extended_len = body.len().checked_add(4)?;
        if extended_len > u16::MAX as usize {
            return None;
        }
        let mut out = Vec::with_capacity(extended_len);
        out.push(0x01);
        out.extend_from_slice(&(extended_len as u16).to_be_bytes());
        out.push(msg_type);
        out.extend_from_slice(body);
        Some(out)
    }
}

/// Parse the length-prefixed wire format. Returns the decoded message body and
/// the total packet length consumed. Bytes after the first packet are left for
/// the caller.
pub fn decode(buf: &[u8]) -> Option<(Decoded<'_>, usize)> {
    if buf.len() < 2 || buf[0] == 0 {
        return None;
    }
    let (total, type_off, minimum) = if buf[0] == 0x01 {
        if buf.len() < 3 {
            return None;
        }
        let total = ((buf[1] as usize) << 8) | buf[2] as usize;
        (total, 3usize, 4usize)
    } else {
        (buf[0] as usize, 1usize, 2usize)
    };
    if total < minimum || total > buf.len() || type_off >= total {
        return None;
    }
    Some((
        Decoded {
            msg_type: buf[type_off],
            body: &buf[type_off + 1..total],
        },
        total,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn searchgw_packet() {
        let p = Client::build_searchgw(1);
        assert_eq!(p, alloc::vec![3, msgtype::SEARCHGW, 0x01]);
    }

    #[test]
    fn connect_packet_has_protocol_id() {
        let p = Client::build_connect("id1", 60, true);
        assert_eq!(p[0] as usize, p.len());
        assert_eq!(p[1], msgtype::CONNECT);
        assert_eq!(p[2], 0x04);
        assert_eq!(p[3], 0x01);
        assert_eq!(p[4..6], 60u16.to_be_bytes());
        assert_eq!(&p[6..], b"id1");
    }

    #[test]
    fn connect_will_flag() {
        let p = Client::build_connect_with_will("id", 10, false, true);
        assert_eq!(p[2], 0x08);
    }

    #[test]
    fn register_packet_has_message_id() {
        let p = Client::build_register_with_msg_id(0x1234, 0xabcd, "a/b");
        assert_eq!(p[1], msgtype::REGISTER);
        assert_eq!(p[2..4], 0x1234u16.to_be_bytes());
        assert_eq!(p[4..6], 0xabcdu16.to_be_bytes());
        assert_eq!(&p[6..], b"a/b");
    }

    #[test]
    fn publish_qos0_has_no_message_id() {
        let p = Client::build_publish(0x1234, b"hi");
        assert_eq!(p[1], msgtype::PUBLISH);
        assert_eq!(p[2], 0);
        assert_eq!(p[3..5], 0x1234u16.to_be_bytes());
        assert_eq!(&p[5..], b"hi");
    }

    #[test]
    fn publish_qos1_flags_and_message_id() {
        let p = Client::build_publish_full(0x1234, b"hi", 1, 7, true, true);
        assert_eq!(p[2], 0xb0);
        assert_eq!(p[3..5], 0x1234u16.to_be_bytes());
        assert_eq!(p[5..7], 7u16.to_be_bytes());
        assert_eq!(&p[7..], b"hi");
    }

    #[test]
    fn extended_length_round_trip() {
        let payload = alloc::vec![0x5a; 400];
        let p = Client::build_willmsg(&payload);
        assert_eq!(p[0], 0x01);
        assert_eq!(u16::from_be_bytes([p[1], p[2]]) as usize, p.len());
        let (decoded, used) = decode(&p).unwrap();
        assert_eq!(used, p.len());
        assert_eq!(decoded.msg_type, msgtype::WILLMSG);
        assert_eq!(decoded.body, payload.as_slice());
    }

    #[test]
    fn decode_leaves_trailing_packet() {
        let mut p = Client::build_searchgw(2);
        let first = p.len();
        p.extend_from_slice(&Client::build_pingreq());
        let (decoded, used) = decode(&p).unwrap();
        assert_eq!(used, first);
        assert_eq!(decoded.msg_type, msgtype::SEARCHGW);
    }

    #[test]
    fn decode_rejects_invalid_lengths() {
        assert!(decode(&[]).is_none());
        assert!(decode(&[0, msgtype::PINGREQ]).is_none());
        assert!(decode(&[1, msgtype::PINGREQ]).is_none());
        assert!(decode(&[5, msgtype::PINGREQ]).is_none());
        assert!(decode(&[0x01, 0, 2]).is_none());
        assert!(decode(&[0x01, 0, 3, msgtype::PINGREQ]).is_none());
    }

    #[test]
    fn connack_accepted_moves_to_active() {
        let mut c = Client::new();
        let ack = alloc::vec![0x03, msgtype::CONNACK, 0x00];
        assert_eq!(c.apply_connack(&ack), Some(ReturnCode::Accepted));
        assert_eq!(c.state, ClientState::Active);
    }

    #[test]
    fn qos1_publish_completes_on_puback_and_retries_as_dup() {
        let mut c = Client::new();
        c.state = ClientState::Active;
        c.topic_id = 0x1234;
        c.qos = 1;
        let p = c.publish(b"value", false, 100).unwrap();
        let msg_id = c.pending_message_id().unwrap();
        assert_eq!(&p[7..], b"value");
        assert!(c.retry_due(1_099).is_none());
        let retry = c.retry_due(1_100).unwrap();
        assert_eq!(retry[2] & 0x80, 0x80);
        assert_eq!(retry[5..7], msg_id.to_be_bytes());
        assert_eq!(c.publish_attempts(), 2);
        let mut ack = alloc::vec![7, msgtype::PUBACK, 0x12, 0x34];
        ack.extend_from_slice(&msg_id.to_be_bytes());
        ack.push(0);
        assert_eq!(c.apply_puback(&ack), Some(ReturnCode::Accepted));
        assert_eq!(c.pending_message_id(), None);
    }

    #[test]
    fn will_handshake_uses_configured_values() {
        let mut c = Client::new();
        c.set_will(Some(Will::new("dev/status", b"offline")));
        let connect = c.begin_connect("dev", 30, true);
        assert_eq!(connect[2] & 0x08, 0x08);
        let topic = c
            .handle_willtopicreq(&alloc::vec![2, msgtype::WILLTOPICREQ])
            .unwrap();
        assert_eq!(topic[1], msgtype::WILLTOPIC);
        assert_eq!(&topic[3..], b"dev/status");
        assert_eq!(c.state, ClientState::AwaitingWillMsg);
        let msg = c
            .handle_willmsgreq(&alloc::vec![2, msgtype::WILLMSGREQ])
            .unwrap();
        assert_eq!(msg[1], msgtype::WILLMSG);
        assert_eq!(&msg[2..], b"offline");
        assert_eq!(c.state, ClientState::Connecting);
    }
}
