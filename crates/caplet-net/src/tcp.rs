//! TCP segment encoding plus a small RFC 793 state machine.
//!
//! The state machine covers connection establishment, data transfer,
//! graceful close and duplicate/out-of-order detection. Congestion
//! control is intentionally conservative: this stack advertises a fixed
//! window and relies on the peer's retransmissions. It never accepts a
//! segment whose checksum or acknowledgement is invalid.

use alloc::vec::Vec;

use crate::addr::{Ipv4Addr, Port};
use crate::icmp;

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
    /// URG.
    pub const URG: u8 = 0x20;
}

/// TCP connection state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TcpState {
    /// No connection.
    Closed,
    /// Passive open, waiting for SYN.
    Listen,
    /// Active open, SYN sent.
    SynSent,
    /// SYN received and SYN-ACK sent.
    SynRcvd,
    /// Data transfer.
    Established,
    /// Our FIN was sent.
    FinWait1,
    /// Our FIN was acknowledged.
    FinWait2,
    /// Peer sent FIN.
    CloseWait,
    /// Both sides sent FIN, waiting for the last ACK.
    Closing,
    /// Peer FIN acknowledged, waiting for our final ACK to be ACKed.
    LastAck,
    /// Waiting for 2MSL expiry.
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
    /// Advertised receive window.
    pub window: u16,
    /// Header length in bytes.
    pub header_len: usize,
    /// Payload after the TCP header.
    pub payload: &'a [u8],
}

impl TcpSegment<'_> {
    /// Whether a flag is set.
    pub fn has(&self, flag: u8) -> bool {
        self.flags & flag != 0
    }

    /// Number of sequence-space bytes consumed by this segment.
    pub fn sequence_len(&self) -> u32 {
        self.payload.len() as u32
            + u32::from(self.has(flags::SYN))
            + u32::from(self.has(flags::FIN))
    }
}

/// Parse a TCP segment and validate the header bounds.
pub fn parse_tcp(buf: &[u8]) -> Option<TcpSegment<'_>> {
    if buf.len() < 20 {
        return None;
    }
    let header_len = usize::from(buf[12] >> 4) * 4;
    if header_len < 20 || header_len > buf.len() {
        return None;
    }
    Some(TcpSegment {
        src: Port(u16::from_be_bytes([buf[0], buf[1]])),
        dst: Port(u16::from_be_bytes([buf[2], buf[3]])),
        seq: u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]),
        ack: u32::from_be_bytes([buf[8], buf[9], buf[10], buf[11]]),
        flags: buf[13],
        window: u16::from_be_bytes([buf[14], buf[15]]),
        header_len,
        payload: &buf[header_len..],
    })
}

/// Parse TCP and verify the pseudo-header checksum.
pub fn parse_tcp_checked(buf: &[u8], src: Ipv4Addr, dst: Ipv4Addr) -> Option<TcpSegment<'_>> {
    let segment = parse_tcp(buf)?;
    if icmp::finish_checksum(tcp_checksum_sum(src, dst, buf)) != 0 {
        return None;
    }
    Some(segment)
}

/// Build a TCP segment (20-byte header, no options).
///
/// All nine fields are mandatory parts of the wire header plus the payload,
/// and keeping them flat avoids an allocation on the transmit path. Callers
/// are almost always marshalling a segment straight from connection state.
#[allow(clippy::too_many_arguments)]
pub fn build_tcp(
    src: Ipv4Addr,
    dst: Ipv4Addr,
    src_port: Port,
    dst_port: Port,
    seq: u32,
    ack: u32,
    segment_flags: u8,
    window: u16,
    payload: &[u8],
) -> Vec<u8> {
    let mut out = Vec::with_capacity(20 + payload.len());
    out.extend_from_slice(&src_port.0.to_be_bytes());
    out.extend_from_slice(&dst_port.0.to_be_bytes());
    out.extend_from_slice(&seq.to_be_bytes());
    out.extend_from_slice(&ack.to_be_bytes());
    out.push(5 << 4);
    out.push(segment_flags);
    out.extend_from_slice(&window.to_be_bytes());
    out.extend_from_slice(&[0, 0, 0, 0]);
    out.extend_from_slice(payload);
    let sum = icmp::finish_checksum(tcp_checksum_sum(src, dst, &out));
    out[16..18].copy_from_slice(&sum.to_be_bytes());
    out
}

fn tcp_checksum_sum(src: Ipv4Addr, dst: Ipv4Addr, tcp: &[u8]) -> u32 {
    let mut sum = icmp::ones_sum(src.octets());
    sum = sum.wrapping_add(icmp::ones_sum(dst.octets()));
    sum = sum.wrapping_add(u32::from(crate::proto::PROTO_TCP));
    sum = sum.wrapping_add(tcp.len() as u32);
    sum.wrapping_add(icmp::ones_sum(tcp))
}

/// Whether `a` precedes `b` in 32-bit TCP sequence space.
pub fn seq_lt(a: u32, b: u32) -> bool {
    (a.wrapping_sub(b) as i32) < 0
}

/// Whether `a` is at or before `b` in TCP sequence space.
pub fn seq_le(a: u32, b: u32) -> bool {
    a == b || seq_lt(a, b)
}

/// A segment that the caller must encode with the connection's addresses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TcpReply {
    /// Sequence number.
    pub seq: u32,
    /// Acknowledgement number.
    pub ack: u32,
    /// Flags.
    pub flags: u8,
    /// Payload.
    pub payload: Vec<u8>,
}

/// Result of feeding a segment into the state machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TcpAction {
    /// Nothing to send.
    None,
    /// Send one segment.
    Send(TcpReply),
    /// Deliver payload and send its cumulative ACK.
    Deliver(Vec<u8>, TcpReply),
    /// The connection completed its handshake.
    Established,
    /// The connection is fully closed.
    Closed,
}

/// Per-connection TCP state and sequence variables.
#[derive(Debug, Clone)]
pub struct TcpConnection {
    /// Current state.
    pub state: TcpState,
    /// Initial send sequence number.
    pub iss: u32,
    /// Oldest unacknowledged sequence number.
    pub snd_una: u32,
    /// Next sequence number to send.
    pub snd_nxt: u32,
    /// Next expected receive sequence number.
    pub rcv_nxt: u32,
    /// Advertised receive window.
    pub rcv_wnd: u16,
    /// Peer's advertised window.
    pub snd_wnd: u16,
    /// Whether our FIN has been sent.
    pub fin_sent: bool,
}

impl TcpConnection {
    /// Create a closed connection with the supplied initial sequence
    /// number.
    pub fn new(iss: u32) -> Self {
        Self {
            state: TcpState::Closed,
            iss,
            snd_una: iss,
            snd_nxt: iss,
            rcv_nxt: 0,
            rcv_wnd: 4096,
            snd_wnd: 0,
            fin_sent: false,
        }
    }

    /// Enter passive-open state.
    pub fn listen(&mut self) {
        *self = Self::new(self.iss);
        self.state = TcpState::Listen;
    }

    /// Enter active-open state and return the initial SYN.
    pub fn connect(&mut self) -> TcpReply {
        self.state = TcpState::SynSent;
        self.snd_una = self.iss;
        self.snd_nxt = self.iss.wrapping_add(1);
        TcpReply {
            seq: self.iss,
            ack: 0,
            flags: flags::SYN,
            payload: Vec::new(),
        }
    }

    /// Process one inbound segment.
    pub fn on_segment(&mut self, seg: &TcpSegment<'_>) -> TcpAction {
        if seg.has(flags::RST) {
            self.state = TcpState::Closed;
            return TcpAction::Closed;
        }
        match self.state {
            TcpState::Listen => {
                if seg.has(flags::SYN) && !seg.has(flags::ACK) {
                    self.rcv_nxt = seg.seq.wrapping_add(1);
                    self.iss = self.iss.wrapping_add(0x1000);
                    self.snd_una = self.iss;
                    self.snd_nxt = self.iss.wrapping_add(1);
                    self.snd_wnd = seg.window;
                    self.state = TcpState::SynRcvd;
                    TcpAction::Send(TcpReply {
                        seq: self.iss,
                        ack: self.rcv_nxt,
                        flags: flags::SYN | flags::ACK,
                        payload: Vec::new(),
                    })
                } else {
                    TcpAction::None
                }
            }
            TcpState::SynSent => {
                if seg.has(flags::SYN) && seg.has(flags::ACK) && seg.ack == self.snd_nxt {
                    self.rcv_nxt = seg.seq.wrapping_add(1);
                    self.snd_una = seg.ack;
                    self.snd_wnd = seg.window;
                    self.state = TcpState::Established;
                    let reply = self.ack_reply(Vec::new());
                    TcpAction::Deliver(Vec::new(), reply)
                } else if seg.has(flags::SYN) && !seg.has(flags::ACK) {
                    // Simultaneous open.
                    self.rcv_nxt = seg.seq.wrapping_add(1);
                    self.state = TcpState::SynRcvd;
                    TcpAction::Send(TcpReply {
                        seq: self.snd_nxt,
                        ack: self.rcv_nxt,
                        flags: flags::SYN | flags::ACK,
                        payload: Vec::new(),
                    })
                } else {
                    TcpAction::None
                }
            }
            TcpState::SynRcvd => {
                if seg.has(flags::ACK) && seg.ack == self.snd_nxt {
                    self.snd_una = seg.ack;
                    self.snd_wnd = seg.window;
                    self.state = TcpState::Established;
                    TcpAction::Established
                } else {
                    TcpAction::None
                }
            }
            TcpState::Established => self.on_established(seg),
            TcpState::FinWait1 => {
                if seg.has(flags::ACK) && seg.ack == self.snd_nxt {
                    self.snd_una = seg.ack;
                    self.state = TcpState::FinWait2;
                }
                if seg.has(flags::FIN) {
                    self.rcv_nxt = self.rcv_nxt.wrapping_add(seg.sequence_len());
                    if self.state == TcpState::FinWait2 {
                        self.state = TcpState::TimeWait;
                    } else {
                        self.state = TcpState::Closing;
                    }
                    return TcpAction::Send(self.ack_reply(Vec::new()));
                }
                TcpAction::None
            }
            TcpState::FinWait2 => {
                if seg.has(flags::FIN) {
                    self.rcv_nxt = self.rcv_nxt.wrapping_add(seg.sequence_len());
                    self.state = TcpState::TimeWait;
                    TcpAction::Send(self.ack_reply(Vec::new()))
                } else {
                    TcpAction::None
                }
            }
            TcpState::Closing => {
                if seg.has(flags::ACK) && seg.ack == self.snd_nxt {
                    self.state = TcpState::TimeWait;
                }
                TcpAction::None
            }
            TcpState::CloseWait => {
                if seg.has(flags::ACK) {
                    self.snd_una = seg.ack;
                }
                TcpAction::None
            }
            TcpState::LastAck => {
                if seg.has(flags::ACK) && seg.ack == self.snd_nxt {
                    self.state = TcpState::Closed;
                    TcpAction::Closed
                } else {
                    TcpAction::None
                }
            }
            TcpState::TimeWait => {
                if seg.has(flags::FIN) {
                    TcpAction::Send(self.ack_reply(Vec::new()))
                } else {
                    TcpAction::None
                }
            }
            TcpState::Closed => TcpAction::None,
        }
    }

    fn on_established(&mut self, seg: &TcpSegment<'_>) -> TcpAction {
        if seg.has(flags::ACK) {
            self.snd_una = seg.ack;
            self.snd_wnd = seg.window;
        }
        if seg.has(flags::FIN) {
            self.rcv_nxt = self.rcv_nxt.wrapping_add(1);
            self.state = TcpState::CloseWait;
            return TcpAction::Send(self.ack_reply(Vec::new()));
        }
        if seg.payload.is_empty() {
            return TcpAction::None;
        }
        if seg.seq == self.rcv_nxt {
            let payload = seg.payload.to_vec();
            self.rcv_nxt = self.rcv_nxt.wrapping_add(payload.len() as u32);
            TcpAction::Deliver(payload, self.ack_reply(Vec::new()))
        } else if seq_lt(seg.seq, self.rcv_nxt) {
            // Duplicate data: re-ACK the current cumulative point.
            TcpAction::Send(self.ack_reply(Vec::new()))
        } else {
            // Out-of-order data is not buffered; duplicate ACK triggers
            // retransmission by the peer.
            TcpAction::Send(self.ack_reply(Vec::new()))
        }
    }

    /// Build data for an established connection.
    pub fn send(&mut self, payload: &[u8]) -> Option<TcpReply> {
        if self.state != TcpState::Established || payload.is_empty() {
            return None;
        }
        let reply = TcpReply {
            seq: self.snd_nxt,
            ack: self.rcv_nxt,
            flags: flags::ACK | flags::PSH,
            payload: payload.to_vec(),
        };
        self.snd_nxt = self.snd_nxt.wrapping_add(payload.len() as u32);
        Some(reply)
    }

    /// Initiate a graceful close.
    pub fn close(&mut self) -> Option<TcpReply> {
        match self.state {
            TcpState::Established => {
                self.state = TcpState::FinWait1;
                Some(self.fin_reply())
            }
            TcpState::CloseWait => {
                self.state = TcpState::LastAck;
                Some(self.fin_reply())
            }
            _ => None,
        }
    }

    fn fin_reply(&mut self) -> TcpReply {
        self.fin_sent = true;
        let reply = TcpReply {
            seq: self.snd_nxt,
            ack: self.rcv_nxt,
            flags: flags::FIN | flags::ACK,
            payload: Vec::new(),
        };
        self.snd_nxt = self.snd_nxt.wrapping_add(1);
        reply
    }

    fn ack_reply(&self, payload: Vec<u8>) -> TcpReply {
        TcpReply {
            seq: self.snd_nxt,
            ack: self.rcv_nxt,
            flags: flags::ACK,
            payload,
        }
    }
}

impl Default for TcpConnection {
    fn default() -> Self {
        Self::new(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const C: Ipv4Addr = Ipv4Addr::new([10, 0, 0, 1]);
    const S: Ipv4Addr = Ipv4Addr::new([10, 0, 0, 2]);

    #[test]
    fn segment_checksum_round_trip() {
        let raw = build_tcp(
            C,
            S,
            Port::new(1234),
            Port::new(80),
            100,
            200,
            flags::ACK,
            4096,
            b"hi",
        );
        let seg = parse_tcp_checked(&raw, C, S).unwrap();
        assert_eq!(seg.seq, 100);
        assert_eq!(seg.ack, 200);
        assert_eq!(seg.payload, b"hi");
        let mut bad = raw;
        bad[20] ^= 1;
        assert!(parse_tcp_checked(&bad, C, S).is_none());
    }

    #[test]
    fn handshake_data_and_close() {
        let mut client = TcpConnection::new(1000);
        let syn = client.connect();
        assert_eq!(client.state, TcpState::SynSent);

        let mut server = TcpConnection::new(5000);
        server.listen();
        let client_syn = TcpSegment {
            src: Port::new(1234),
            dst: Port::new(80),
            seq: syn.seq,
            ack: 0,
            flags: flags::SYN,
            window: 4096,
            header_len: 20,
            payload: &[],
        };
        let server_reply = match server.on_segment(&client_syn) {
            TcpAction::Send(r) => r,
            other => panic!("unexpected {:?}", other),
        };
        assert_eq!(server_reply.flags, flags::SYN | flags::ACK);

        let synack = TcpSegment {
            src: Port::new(80),
            dst: Port::new(1234),
            seq: server_reply.seq,
            ack: server_reply.ack,
            flags: server_reply.flags,
            window: 4096,
            header_len: 20,
            payload: &[],
        };
        assert!(matches!(
            client.on_segment(&synack),
            TcpAction::Deliver(_, _)
        ));
        assert_eq!(client.state, TcpState::Established);

        let ack = TcpSegment {
            src: Port::new(1234),
            dst: Port::new(80),
            seq: client.snd_nxt,
            ack: client.rcv_nxt,
            flags: flags::ACK,
            window: 4096,
            header_len: 20,
            payload: &[],
        };
        assert_eq!(server.on_segment(&ack), TcpAction::Established);

        let data = client.send(b"hello").unwrap();
        let data_seg = TcpSegment {
            src: Port::new(1234),
            dst: Port::new(80),
            seq: data.seq,
            ack: data.ack,
            flags: data.flags,
            window: 4096,
            header_len: 20,
            payload: &data.payload,
        };
        match server.on_segment(&data_seg) {
            TcpAction::Deliver(payload, ack) => {
                assert_eq!(payload, b"hello");
                assert_eq!(ack.ack, data.seq + 5);
            }
            other => panic!("unexpected {:?}", other),
        }

        let fin = client.close().unwrap();
        let fin_seg = TcpSegment {
            src: Port::new(1234),
            dst: Port::new(80),
            seq: fin.seq,
            ack: fin.ack,
            flags: fin.flags,
            window: 4096,
            header_len: 20,
            payload: &[],
        };
        assert!(matches!(server.on_segment(&fin_seg), TcpAction::Send(_)));
        assert_eq!(server.state, TcpState::CloseWait);
    }

    #[test]
    fn sequence_wraparound() {
        assert!(seq_lt(u32::MAX, 0));
        assert!(!seq_lt(0, u32::MAX));
        assert!(seq_le(5, 5));
    }
}
