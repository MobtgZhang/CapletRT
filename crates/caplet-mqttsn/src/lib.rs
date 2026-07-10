//! MQTT-SN v1.2 — minimal client encoding/decoding.
//!
//! MQTT-SN (MQTT for Sensor Networks) is the UDP-based sibling
//! of MQTT, designed for low-bandwidth, low-power networks.
//! This module implements just enough to:
//!
//! * Search for a gateway (`SEARCHGW`)
//! * Connect to it (`CONNECT` / `CONNACK`)
//! * Register a topic name (`REGISTER` / `REGACK`)
//! * Publish a message (`PUBLISH`)
//!
//! It does not implement will messages, retained messages or
//! QoS 2; the industrial demo uses QoS 0 (at-most-once).

#![no_std]
#![deny(missing_docs)]

//! End-to-end MQTT-SN test against a mock broker.
pub mod e2e;

/// Network-resilience (drop + retry) test for the MQTT-SN
/// client. See docv1.md §20 (Phase 10.3.2).
pub mod resilience;

extern crate alloc;

/// MQTT-SN control packet types (subset).
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

/// Return code for `CONNACK` / `REGACK` / `SUBACK`.
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

/// MQTT-SN client state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientState {
    /// Awaiting `SEARCHGW` reply.
    Searching,
    /// Awaiting `CONNECT` ack.
    Connecting,
    /// Connected.
    Active,
    /// Asleep (QoS -1).
    Asleep,
    /// Disconnected.
    Disconnected,
}

/// An MQTT-SN client. The `state` field tracks the protocol
/// state machine; `topic_id` is assigned by the gateway on
/// `REGISTER`.
#[derive(Debug)]
pub struct Client {
    /// Client state.
    pub state: ClientState,
    /// Topic id assigned by the gateway.
    pub topic_id: u16,
    /// QoS level (0, 1, or 2). The industrial demo uses 0.
    pub qos: u8,
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
        }
    }

    /// Build a `SEARCHGW` packet with the given radius
    /// (max hops the gateway may advertise).
    pub fn build_searchgw(radius: u8) -> alloc::vec::Vec<u8> {
        alloc::vec![3, msgtype::SEARCHGW, radius]
    }

    /// Build a `CONNECT` packet. `client_id` is a UTF-8 string.
    pub fn build_connect(client_id: &str, duration: u16, clean: bool) -> alloc::vec::Vec<u8> {
        let cid = client_id.as_bytes();
        let mut out = alloc::vec::Vec::with_capacity(6 + cid.len());
        out.push(0x02); // length low byte (placeholder)
        out.push(0); // length high byte
        out.push(msgtype::CONNECT);
        out.push(if clean { 0x04 } else { 0x00 });
        out.push(((duration >> 8) & 0xff) as u8);
        out.push((duration & 0xff) as u8);
        out.extend_from_slice(cid);
        // Fix up length.
        let len = out.len();
        out[0] = (len & 0xff) as u8;
        out[1] = ((len >> 8) & 0xff) as u8;
        out
    }

    /// Build a `REGISTER` packet. The first two bytes of
    /// `topic_name` are also the requested topic id.
    pub fn build_register(topic_id: u16, topic_name: &str) -> alloc::vec::Vec<u8> {
        let tn = topic_name.as_bytes();
        let mut out = alloc::vec::Vec::with_capacity(6 + tn.len());
        out.push(0x02);
        out.push(0);
        out.push(msgtype::REGISTER);
        out.push(((topic_id >> 8) & 0xff) as u8);
        out.push((topic_id & 0xff) as u8);
        out.extend_from_slice(tn);
        let len = out.len();
        out[0] = (len & 0xff) as u8;
        out[1] = ((len >> 8) & 0xff) as u8;
        out
    }

    /// Build a `PUBLISH` packet (QoS 0).
    pub fn build_publish(topic_id: u16, payload: &[u8]) -> alloc::vec::Vec<u8> {
        let mut out = alloc::vec::Vec::with_capacity(7 + payload.len());
        out.push(0x02);
        out.push(0);
        out.push(msgtype::PUBLISH);
        out.push(0x00); // flags: dup=0, QoS=0, retain=0
        out.push(((topic_id >> 8) & 0xff) as u8);
        out.push((topic_id & 0xff) as u8);
        out.extend_from_slice(payload);
        let len = out.len();
        out[0] = (len & 0xff) as u8;
        out[1] = ((len >> 8) & 0xff) as u8;
        out
    }

    /// Build a `DISCONNECT` packet.
    pub fn build_disconnect() -> alloc::vec::Vec<u8> {
        alloc::vec![2, msgtype::DISCONNECT]
    }

    /// Apply an incoming `CONNACK` packet (length, type, code).
    /// Returns the [`ReturnCode`].
    pub fn apply_connack(&mut self, pkt: &[u8]) -> Option<ReturnCode> {
        if pkt.len() < 3 || pkt[1] != msgtype::CONNACK {
            return None;
        }
        let rc = match pkt[2] {
            0 => ReturnCode::Accepted,
            1 => ReturnCode::RejectedCongestion,
            2 => ReturnCode::RejectedTopicId,
            3 => ReturnCode::NotSupported,
            _ => return None,
        };
        if rc == ReturnCode::Accepted {
            self.state = ClientState::Active;
        }
        Some(rc)
    }

    /// Apply an incoming `REGACK` packet (length, type, id_hi,
    /// id_lo, msg_id_hi, msg_id_lo, code).
    pub fn apply_regack(&mut self, pkt: &[u8]) -> Option<ReturnCode> {
        if pkt.len() < 7 || pkt[1] != msgtype::REGACK {
            return None;
        }
        let id = ((pkt[2] as u16) << 8) | (pkt[3] as u16);
        let rc = match pkt[6] {
            0 => ReturnCode::Accepted,
            1 => ReturnCode::RejectedCongestion,
            2 => ReturnCode::RejectedTopicId,
            3 => ReturnCode::NotSupported,
            _ => return None,
        };
        if rc == ReturnCode::Accepted {
            self.topic_id = id;
        }
        Some(rc)
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

/// Parse the length-prefixed wire format. Returns the decoded
/// message body and the total packet length consumed.
pub fn decode<'a>(buf: &'a [u8]) -> Option<(Decoded<'a>, usize)> {
    if buf.len() < 2 {
        return None;
    }
    let (len, type_off) = if buf[0] == 0x01 {
        // 3-byte extended length.
        if buf.len() < 3 {
            return None;
        }
        ((((buf[1] as usize) << 8) | (buf[2] as usize)) + 3, 3)
    } else {
        (buf[0] as usize, 1)
    };
    if buf.len() < len || len < 2 {
        return None;
    }
    Some((
        Decoded {
            msg_type: buf[type_off],
            body: &buf[type_off + 1..len],
        },
        len,
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
    fn connect_packet() {
        let p = Client::build_connect("id1", 60, true);
        assert_eq!(p[2], msgtype::CONNECT);
        assert_eq!(p[3], 0x04); // clean
        assert_eq!(p[4..6], (60u16).to_be_bytes());
        assert_eq!(&p[6..], b"id1");
    }

    #[test]
    fn publish_packet() {
        let p = Client::build_publish(0x1234, b"hi");
        assert_eq!(p[2], msgtype::PUBLISH);
        assert_eq!(p[4..6], 0x1234u16.to_be_bytes());
        assert_eq!(&p[6..], b"hi");
    }

    #[test]
    fn connack_accepted_moves_to_active() {
        let mut c = Client::new();
        let ack = alloc::vec![0x03, msgtype::CONNACK, 0x00];
        assert_eq!(c.apply_connack(&ack), Some(ReturnCode::Accepted));
        assert_eq!(c.state, ClientState::Active);
    }

    #[test]
    fn decode_simple() {
        let p = Client::build_searchgw(2);
        let (d, used) = decode(&p).unwrap();
        assert_eq!(used, p.len());
        assert_eq!(d.msg_type, msgtype::SEARCHGW);
    }
}