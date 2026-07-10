//! Minimal TCP — only enough to model a SYN/SYN-ACK/ACK
//! handshake. Real state machines are out of scope for the
//! current phase; this module exists so the industrial
//! MQTT-SN path can speak TCP later.
//!
//! ## Implementation Plan (M2.x)
//!
//! A full TCP implementation following RFC 793 should include:
//!
//! ### State Machine
//! ```ignore
//! Closed -> SYN_SENT (active open)
//! Closed -> LISTEN (passive open)
//! LISTEN -> SYN_RCVD (SYN received)
//! SYN_RCVD -> ESTABLISHED (ACK received)
//! ESTABLISHED -> FIN_WAIT_1 (close)
//! FIN_WAIT_1 -> FIN_WAIT_2 (FIN ACK)
//! FIN_WAIT_2 -> TIME_WAIT (FIN received)
//! TIME_WAIT -> CLOSED (2MSL timeout)
//! ```
//!
//! ### Congestion Control
//! - Slow start (cwnd starts at 1 MSS)
//! - Congestion avoidance
//! - Fast retransmit
//! - Simplified Reno/NewReno
//!
//! ### Key Data Structures
//! ```ignore
//! struct TcpConnection {
//!     state: TcpState,
//!     local: SocketAddr,
//!     remote: SocketAddr,
//!     snd_nxt: u32,  // next sequence to send
//!     snd_una: u32,  // oldest unacknowledged
//!     rcv_nxt: u32,  // next expected
//!     cwnd: u32,     // congestion window
//!     ssthresh: u32, // slow start threshold
//! }
//! ```
//!
//! ### Integration with SocketTable
//! - TCP sockets stored in SocketTable keyed by (local_port, remote_addr, remote_port)
//! - Passive sockets (LISTEN) keyed by local_port only
//! - Incoming segments matched to connections via hash lookup

use crate::addr::Port;

/// TCP flag bits.
pub mod flags {
    /// FIN.
    pub const FIN: u8 = 0x01;
    /// SYN.
    pub const SYN: u8 = 0x02;
    /// RST.
    pub const RST: u8 = 0x04;
    /// PSH.
    pub const PSH: u8 = 0x08;
    /// ACK.
    pub const ACK: u8 = 0x10;
}

/// TCP state. A simplified version of RFC 793.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TcpState {
    /// Closed.
    Closed,
    /// Listen.
    Listen,
    /// Syn sent.
    SynSent,
    /// Established.
    Established,
    /// Close wait.
    CloseWait,
    /// Last ack.
    LastAck,
    /// Time wait.
    TimeWait,
}

/// Decoded TCP segment.
#[derive(Debug, Clone)]
pub struct TcpSegment<'a> {
    /// Source port.
    pub src: Port,
    /// Destination port.
    pub dst: Port,
    /// Sequence number.
    pub seq: u32,
    /// Acknowledgement number.
    pub ack: u32,
    /// Flag bits.
    pub flags: u8,
    /// Payload after the 20-byte header.
    pub payload: &'a [u8],
}

/// Parse a TCP segment.
pub fn parse_tcp(buf: &[u8]) -> Option<TcpSegment<'_>> {
    if buf.len() < 20 {
        return None;
    }
    let data_offset = (buf[12] >> 4) as usize * 4;
    if buf.len() < data_offset {
        return None;
    }
    Some(TcpSegment {
        src: Port(u16::from_be_bytes([buf[0], buf[1]])),
        dst: Port(u16::from_be_bytes([buf[2], buf[3]])),
        seq: u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]),
        ack: u32::from_be_bytes([buf[8], buf[9], buf[10], buf[11]]),
        flags: buf[13],
        payload: &buf[data_offset..],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tcp_basic() {
        let mut hdr = [0u8; 20];
        hdr[0..2].copy_from_slice(&12345u16.to_be_bytes());
        hdr[2..4].copy_from_slice(&80u16.to_be_bytes());
        hdr[4..8].copy_from_slice(&100u32.to_be_bytes());
        hdr[8..12].copy_from_slice(&200u32.to_be_bytes());
        hdr[12] = 0x50; // data offset = 5
        hdr[13] = flags::SYN;
        let s = parse_tcp(&hdr).unwrap();
        assert_eq!(s.src, Port::new(12345));
        assert_eq!(s.dst, Port::new(80));
        assert_eq!(s.seq, 100);
        assert_eq!(s.flags, flags::SYN);
    }
}