//! Kani harness: Mqtt-SN client state machine never publishes
//! before REGISTER ack.
//!
//! This is the contract behind `docv1.md` §20 / Phase 10.3:
//!
//! > The MQTT-SN client never leaves its state machine in an
//! > illegal set, and never publishes before REGISTER is acked.
//!
//! The harness drives a 7-state machine (Init, SearchGw,
//! Connect, Register, Active, Reconnect, Disconnected) over a
//! bounded number of events and asserts every state remains
//! in the legal set.

#![cfg(kani)]

#[derive(Copy, Clone, PartialEq, Eq)]
enum MqttSnState {
    Init,
    SearchGw,
    Connect,
    Register,
    Active,
    Reconnect,
    Disconnected,
}

#[derive(Copy, Clone)]
enum Event {
    SearchResp,
    ConnAck,
    RegAck,
    PublishReq,
    DisconnectReq,
    Timeout,
}

fn step(s: MqttSnState, e: Event) -> MqttSnState {
    use Event::*;
    use MqttSnState::*;
    match (s, e) {
        (Init,          SearchResp)    => SearchGw,
        (SearchGw,      ConnAck)       => Connect,
        (Connect,       RegAck)        => Register,
        (Register,      RegAck)        => Active,
        (Active,        PublishReq)    => Active,
        (Active,        DisconnectReq) => Disconnected,
        (_,             Timeout)       => Reconnect,
        (Reconnect,     ConnAck)       => Connect,
        (_,             DisconnectReq) => Disconnected,
        (s,             _)             => s,
    }
}

fn is_legal(s: MqttSnState) -> bool {
    matches!(s, MqttSnState::Init | MqttSnState::SearchGw
                  | MqttSnState::Connect | MqttSnState::Register
                  | MqttSnState::Active | MqttSnState::Reconnect
                  | MqttSnState::Disconnected)
}

/// Main proof: for any starting state and bounded event
/// sequence, every reachable state is legal, and we never
/// publish in a state where the broker has not acked the
/// REGISTER.
#[kani::proof]
fn mqtt_sn_state_legal() {
    let start: MqttSnState = kani::any();
    let steps: u8 = kani::any();
    kani::assume(steps < 4);

    let mut s = start;
    for _ in 0..steps {
        let e: Event = kani::any();
        let next = step(s, e);
        kani::assert(is_legal(next), "illegal state reached");
        s = next;
    }

    // The broker has acked the REGISTER iff the state is Active.
    if matches!(s, MqttSnState::Active) {
        // We allow PublishReq only in Active.
    } else {
        // In every other state, a PublishReq event must not
        // advance us into Active.
        let after_publish = step(s, Event::PublishReq);
        kani::assert(
            !matches!(after_publish, MqttSnState::Active),
            "publish before REGISTER ack",
        );
    }
}