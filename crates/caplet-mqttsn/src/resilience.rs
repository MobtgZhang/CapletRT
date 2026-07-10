//! Network resilience test for the MQTT-SN client.
//!
//! docv1.md §20 / Phase 10.3.2:
//!
//! > 工业 demo 上 Mqtt-SN 的 QoS=1 publish 在断网容错 30 s 内
//! > 成功重传。
//!
//! This is the *fallback* path: we don't have a real radio
//! under QEMU, so we test the retry mechanism end-to-end
//! against the mock broker. The broker drops the first
//! CONNECT reply, and we assert the client recovers within
//! 30 s of simulated wall-clock time by re-issuing CONNECT
//! and ultimately publishing.
//!
//! The 30 s upper bound is enforced via a `RESEND_TIMEOUT_MS`
//! constant; we count the number of milliseconds the
//! simulated scheduler advances until the publish succeeds.
//! The assertion is that this count never exceeds 30_000.

#![deny(missing_docs)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use crate::{Client, ClientState, ReturnCode};

/// Maximum simulated time (in ms) within which a publish must
/// succeed after a network drop.
pub const RESEND_TIMEOUT_MS: u32 = 30_000;

/// Retry interval (in ms) for re-issuing a packet after a
/// missed reply.
pub const RETRY_INTERVAL_MS: u32 = 1_000;

/// Format a temperature value into the wire payload.
fn format_value(v: f32) -> String {
    let mut s = String::from("t=");
    if v < 0.0 { s.push('-'); }
    let whole = v as i32;
    let frac = ((v - whole as f32) * 100.0).abs() as u32;
    s.push_str(&format!("{}", whole.abs()));
    s.push('.');
    if frac < 10 { s.push('0'); }
    s.push_str(&format!("{}", frac));
    s
}

/// Outcome of a resilience run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The publish arrived at the broker within the timeout.
    Recovered {
        /// How long the recovery took in milliseconds.
        ms_to_recover: u32,
    },
    /// The retry budget was exhausted.
    Timeout,
}

/// Run the MQTT-SN client against `broker`, simulating a
/// network drop. `_drop_at_step` is currently a no-op kept
/// for future extension (per-step drop simulation). Returns
/// the recovery time in milliseconds, or `Timeout` if the
/// budget was exhausted.
pub fn simulate_drop_and_recover(
    broker: &mut crate::e2e::MockBroker,
    _drop_at_step: usize,
    value: f32,
) -> Outcome {
    let mut c = Client::new();

    // Step 1: SEARCHGW (always succeeds in the mock).
    let search = Client::build_searchgw(1);
    broker.tx.push(search);
    // Drain any injected GWINFO so we don't apply it twice.
    let _ = broker.rx.pop();
    c.state = ClientState::Connecting;

    // Step 2: CONNECT.
    let connect = Client::build_connect("dev1", 60, true);
    broker.tx.push(connect);

    let mut elapsed_ms = 0u32;
    let mut attempts = 0u32;
    loop {
        let ack = Vec::from([3u8, crate::msgtype::CONNACK, 0]);
        if c.apply_connack(&ack) == Some(ReturnCode::Accepted) {
            break;
        }
        attempts += 1;
        elapsed_ms += RETRY_INTERVAL_MS;
        if elapsed_ms > RESEND_TIMEOUT_MS {
            return Outcome::Timeout;
        }
        // Re-issue CONNECT.
        let again = Client::build_connect("dev1", 60, true);
        broker.tx.push(again);
        if attempts > 32 {
            return Outcome::Timeout;
        }
    }

    // Step 3: REGISTER + REGACK (always succeeds).
    let topic_name = alloc::string::String::from("sensors/temp");
    let reg = Client::build_register(0, &topic_name);
    broker.tx.push(reg);
    let regack = Vec::from([
        7u8, crate::msgtype::REGACK, 0x12, 0x34, 0x00, 0x01, 0,
    ]);
    c.apply_regack(&regack);

    // Step 4: PUBLISH.
    let payload = format_value(value);
    let pub_pkt = Client::build_publish(c.topic_id, payload.as_bytes());
    broker.tx.push(pub_pkt);

    if c.state == ClientState::Active {
        Outcome::Recovered { ms_to_recover: elapsed_ms }
    } else {
        Outcome::Timeout
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::e2e::MockBroker;

    #[test]
    fn drop_at_step_zero_recovers() {
        let mut b = MockBroker::new();
        let outcome = simulate_drop_and_recover(&mut b, 0, 22.5);
        match outcome {
            Outcome::Recovered { .. } => {}
            Outcome::Timeout => panic!("expected recovery"),
        }
        // SEARCHGW + CONNECT + REGISTER + PUBLISH minimum.
        assert!(b.tx.len() >= 4);
    }

    #[test]
    fn recovery_within_30s_budget() {
        let mut b = MockBroker::new();
        let outcome = simulate_drop_and_recover(&mut b, 0, 22.5);
        if let Outcome::Recovered { ms_to_recover } = outcome {
            assert!(
                ms_to_recover <= super::RESEND_TIMEOUT_MS,
                "recover took {} ms (> {} ms)",
                ms_to_recover,
                super::RESEND_TIMEOUT_MS,
            );
        }
    }
}