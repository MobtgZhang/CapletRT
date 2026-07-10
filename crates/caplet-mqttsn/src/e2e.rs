//! End-to-end MQTT-SN test: build packets and parse the
//! server's reply using a fake broker.
//!
//! This test exercises the full `Client` state machine: it
//! SEARCHGW → CONNECT → REGISTER → PUBLISH. A mock broker
//! (a `Vec<u8>`) returns the responses the test synthesises;
//! the assertions check that the client transitions to
//! `Active` and acquires a valid topic id.

#![no_std]
#![deny(missing_docs)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use crate::{Client, ClientState, ReturnCode};

/// A minimal mock broker that accepts a packet queue.
#[derive(Debug, Default)]
pub struct MockBroker {
    /// Incoming packet queue (filled by the test).
    pub rx: Vec<Vec<u8>>,
    /// Outgoing packet queue (filled by the client).
    pub tx: Vec<Vec<u8>>,
}

impl MockBroker {
    /// Build a new empty broker.
    pub fn new() -> Self {
        Self::default()
    }

    /// Hand the broker a synthesised reply.
    pub fn inject(&mut self, pkt: Vec<u8>) {
        self.rx.push(pkt);
    }

    /// Take the next outbound packet.
    pub fn take(&mut self) -> Option<Vec<u8>> {
        if self.tx.is_empty() {
            None
        } else {
            Some(self.tx.remove(0))
        }
    }
}

/// Run the full client lifecycle against `broker`. Returns
/// the final client state and the number of published
/// messages.
pub fn run_end_to_end(broker: &mut MockBroker, value: f32) -> (ClientState, u16) {
    let mut c = Client::new();

    // 1. SEARCHGW
    let search = Client::build_searchgw(1);
    broker.tx.push(search);
    // Broker replies with GWINFO.
    if let Some(pkt) = broker.rx.pop() {
        let (_dec, _used) = crate::decode(&pkt).unwrap();
        c.state = ClientState::Connecting;
    }

    // 2. CONNECT
    let connect = Client::build_connect("dev1", 60, true);
    broker.tx.push(connect);
    let ack = alloc::vec![3u8, crate::msgtype::CONNACK, 0];
    c.apply_connack(&ack);

    // 3. REGISTER
    let topic_name = String::from("sensors/temp");
    let reg = Client::build_register(0, &topic_name);
    broker.tx.push(reg);
    // Broker assigns topic id 0x1234.
    let regack = alloc::vec![
        7,
        crate::msgtype::REGACK,
        0x12,
        0x34,
        0x00,
        0x01,
        0, // accepted
    ];
    c.apply_regack(&regack);

    // 4. PUBLISH
    let payload = format_value(value);
    let pub_pkt = Client::build_publish(c.topic_id, payload.as_bytes());
    broker.tx.push(pub_pkt);

    (c.state, c.topic_id)
}

fn format_value(v: f32) -> String {
    // Minimal ASCII formatting without std::fmt.
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
    fn e2e_publish_flow() {
        let mut b = MockBroker::new();
        b.inject(alloc::vec![4, crate::msgtype::GWINFO, 0x01, 0x02]);
        let (state, topic_id) = run_end_to_end(&mut b, 22.5);
        assert_eq!(state, ClientState::Active);
        assert_eq!(topic_id, 0x1234);
        // 4 outbound packets: SEARCHGW, CONNECT, REGISTER, PUBLISH.
        assert_eq!(b.tx.len(), 4);
    }

    #[test]
    fn connect_rejected_keeps_state() {
        let mut c = Client::new();
        let ack = alloc::vec![3, crate::msgtype::CONNACK, 1];
        let rc = c.apply_connack(&ack);
        assert_eq!(rc, Some(ReturnCode::RejectedCongestion));
        assert_ne!(c.state, ClientState::Active);
    }
}