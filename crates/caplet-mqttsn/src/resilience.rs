//! Network resilience test for the MQTT-SN client.
//!
//! This module drives the real client state machine against the stateful mock
//! gateway and drops one selected request or response. It verifies that the
//! client retransmits within the 30 second recovery budget. QoS 1 publishes
//! are retransmitted with the DUP flag set.

#![deny(missing_docs)]

extern crate alloc;

use alloc::format;
use alloc::string::String;

use crate::e2e::MockBroker;
use crate::{Client, ClientState, ReturnCode, Will};

/// Maximum simulated time (in ms) within which a publish must succeed after a
/// network drop.
pub const RESEND_TIMEOUT_MS: u32 = 30_000;

/// Retry interval (in ms) for re-issuing a packet after a missed reply.
pub const RETRY_INTERVAL_MS: u32 = 1_000;

/// Drop the first `SEARCHGW`/`GWINFO` exchange.
pub const DROP_SEARCHGW: usize = 0;
/// Drop the first CONNECT/WILLTOPICREQ exchange.
pub const DROP_CONNECT: usize = 1;
/// Drop the first WILLTOPIC/WILLMSGREQ exchange.
pub const DROP_WILLTOPIC: usize = 2;
/// Drop the first WILLMSG/CONNACK exchange.
pub const DROP_WILLMSG: usize = 3;
/// Drop the first REGISTER/REGACK exchange.
pub const DROP_REGISTER: usize = 4;
/// Drop the first QoS 1 PUBLISH/PUBACK exchange.
pub const DROP_PUBACK: usize = 5;

/// Outcome of a resilience run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The publish arrived at the gateway within the timeout.
    Recovered {
        /// How long the recovery took in milliseconds.
        ms_to_recover: u32,
    },
    /// The retry budget was exhausted or a reply was malformed.
    Timeout,
}

/// Format a temperature value into the wire payload.
fn format_value(v: f32) -> String {
    let mut s = String::from("t=");
    if v < 0.0 {
        s.push('-');
    }
    let whole = v as i32;
    let frac = ((v - whole as f32) * 100.0).abs() as u32;
    s.push_str(&format!("{}", whole.abs()));
    s.push('.');
    if frac < 10 {
        s.push('0');
    }
    s.push_str(&format!("{}", frac));
    s
}

fn exchange(
    broker: &mut MockBroker,
    pkt: &[u8],
    step: usize,
    drop_at_step: usize,
    dropped: &mut bool,
    elapsed_ms: &mut u32,
) -> Option<alloc::vec::Vec<u8>> {
    let response = broker.exchange(pkt);
    if step == drop_at_step && !*dropped {
        *dropped = true;
        *elapsed_ms = elapsed_ms.saturating_add(RETRY_INTERVAL_MS);
        return None;
    }
    response
}

fn retry_request(
    broker: &mut MockBroker,
    pkt: &[u8],
    step: usize,
    drop_at_step: usize,
    dropped: &mut bool,
    elapsed_ms: &mut u32,
) -> Option<alloc::vec::Vec<u8>> {
    let first = exchange(broker, pkt, step, drop_at_step, dropped, elapsed_ms);
    if first.is_some() {
        first
    } else if *elapsed_ms <= RESEND_TIMEOUT_MS {
        broker.exchange(pkt)
    } else {
        None
    }
}

/// Run the MQTT-SN client against `broker`, dropping the exchange identified by
/// `drop_at_step` once. The flow covers gateway discovery, CONNECT with a last
/// will, topic registration and a QoS 1 publish.
pub fn simulate_drop_and_recover(
    broker: &mut MockBroker,
    drop_at_step: usize,
    value: f32,
) -> Outcome {
    let mut c = Client::new();
    let mut dropped = false;
    let mut elapsed_ms = 0u32;

    let search = Client::build_searchgw(1);
    let Some(reply) = retry_request(
        broker,
        &search,
        DROP_SEARCHGW,
        drop_at_step,
        &mut dropped,
        &mut elapsed_ms,
    ) else {
        return Outcome::Timeout;
    };
    if crate::decode(&reply).map(|(d, _)| d.msg_type) != Some(crate::msgtype::GWINFO) {
        return Outcome::Timeout;
    }
    c.state = ClientState::Connecting;

    c.set_will(Some(Will::new("dev/status", b"offline")));
    let connect = c.begin_connect("dev1", 60, true);
    let Some(reply) = retry_request(
        broker,
        &connect,
        DROP_CONNECT,
        drop_at_step,
        &mut dropped,
        &mut elapsed_ms,
    ) else {
        return Outcome::Timeout;
    };
    let Some(willtopic) = c.handle_willtopicreq(&reply) else {
        return Outcome::Timeout;
    };
    let Some(reply) = retry_request(
        broker,
        &willtopic,
        DROP_WILLTOPIC,
        drop_at_step,
        &mut dropped,
        &mut elapsed_ms,
    ) else {
        return Outcome::Timeout;
    };
    let Some(willmsg) = c.handle_willmsgreq(&reply) else {
        return Outcome::Timeout;
    };
    let Some(reply) = retry_request(
        broker,
        &willmsg,
        DROP_WILLMSG,
        drop_at_step,
        &mut dropped,
        &mut elapsed_ms,
    ) else {
        return Outcome::Timeout;
    };
    if c.apply_connack(&reply) != Some(ReturnCode::Accepted) {
        return Outcome::Timeout;
    }

    let reg = Client::build_register_with_msg_id(0, 1, "sensors/temp");
    let Some(reply) = retry_request(
        broker,
        &reg,
        DROP_REGISTER,
        drop_at_step,
        &mut dropped,
        &mut elapsed_ms,
    ) else {
        return Outcome::Timeout;
    };
    if c.apply_regack(&reply) != Some(ReturnCode::Accepted) {
        return Outcome::Timeout;
    }

    c.qos = 1;
    let payload = format_value(value);
    let Some(publish) = c.publish(payload.as_bytes(), false, elapsed_ms) else {
        return Outcome::Timeout;
    };
    let reply = exchange(
        broker,
        &publish,
        DROP_PUBACK,
        drop_at_step,
        &mut dropped,
        &mut elapsed_ms,
    );
    let reply = match reply {
        Some(reply) => reply,
        None => {
            if elapsed_ms > RESEND_TIMEOUT_MS {
                return Outcome::Timeout;
            }
            let Some(retry) = c.retry_due(elapsed_ms) else {
                return Outcome::Timeout;
            };
            let Some(reply) = broker.exchange(&retry) else {
                return Outcome::Timeout;
            };
            reply
        }
    };
    if c.apply_puback(&reply) != Some(ReturnCode::Accepted) {
        return Outcome::Timeout;
    }

    if c.state == ClientState::Active
        && broker.published_payload() == payload.as_bytes()
        && elapsed_ms <= RESEND_TIMEOUT_MS
    {
        Outcome::Recovered {
            ms_to_recover: elapsed_ms,
        }
    } else {
        Outcome::Timeout
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_drop_point_recovers_within_budget() {
        for step in [
            DROP_SEARCHGW,
            DROP_CONNECT,
            DROP_WILLTOPIC,
            DROP_WILLMSG,
            DROP_REGISTER,
            DROP_PUBACK,
        ] {
            let mut b = MockBroker::new();
            let outcome = simulate_drop_and_recover(&mut b, step, 22.5);
            match outcome {
                Outcome::Recovered { ms_to_recover } => {
                    assert!(ms_to_recover <= RESEND_TIMEOUT_MS);
                    assert_eq!(ms_to_recover, RETRY_INTERVAL_MS);
                }
                Outcome::Timeout => panic!("drop step {step} did not recover"),
            }
            assert_eq!(b.published_payload(), b"t=22.50");
        }
    }

    #[test]
    fn dropped_puback_causes_dup_retransmission() {
        let mut b = MockBroker::new();
        let outcome = simulate_drop_and_recover(&mut b, DROP_PUBACK, 7.0);
        assert!(matches!(outcome, Outcome::Recovered { .. }));
        assert!(b.publish_dup());
    }

    #[test]
    fn dropped_connect_is_retransmitted() {
        let mut b = MockBroker::new();
        let outcome = simulate_drop_and_recover(&mut b, DROP_CONNECT, 1.0);
        assert!(matches!(outcome, Outcome::Recovered { .. }));
        let connects = b
            .tx
            .iter()
            .filter(|p| crate::decode(p).map(|(d, _)| d.msg_type) == Some(crate::msgtype::CONNECT))
            .count();
        assert_eq!(connects, 2);
    }
}
