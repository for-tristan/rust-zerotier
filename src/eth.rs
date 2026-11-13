//! Ethernet frame header parsing.
//!
//! Mirrors the minimal parsing done by the original `vswitch.py`:
//! a 14-byte header split into destination MAC and source MAC.

use std::fmt;

/// A 48-bit Ethernet MAC address.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct MacAddr(pub [u8; 6]);

impl MacAddr {
    /// The Ethernet broadcast address `ff:ff:ff:ff:ff:ff`.
    pub const BROADCAST: MacAddr = MacAddr([0xff; 6]);

    /// True for the broadcast address.
    pub fn is_broadcast(&self) -> bool {
        self.0 == MacAddr::BROADCAST.0
    }
}

impl fmt::Display for MacAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Match the original output: lowercase, colon-separated, zero-padded.
        let m = self.0;
        write!(
            f,
            "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            m[0], m[1], m[2], m[3], m[4], m[5]
        )
    }
}

/// A 14-byte Ethernet header.
#[derive(Clone, Copy)]
pub struct EtherHeader {
    pub dst: MacAddr,
    pub src: MacAddr,
}

impl EtherHeader {
    /// Parse the leading 14 bytes of an Ethernet frame.
    /// Returns `None` if the buffer is shorter than 14 bytes.
    pub fn parse(buf: &[u8]) -> Option<EtherHeader> {
        if buf.len() < 14 {
            return None;
        }
        let mut dst = [0u8; 6];
        let mut src = [0u8; 6];
        dst.copy_from_slice(&buf[0..6]);
        src.copy_from_slice(&buf[6..12]);
        Some(EtherHeader {
            dst: MacAddr(dst),
            src: MacAddr(src),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mac_display_matches_original_format() {
        let mac = MacAddr([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
        assert_eq!(mac.to_string(), "aa:bb:cc:dd:ee:ff");
    }

    #[test]
    fn broadcast_is_recognised() {
        assert!(MacAddr::BROADCAST.is_broadcast());
        assert!(!MacAddr([1, 2, 3, 4, 5, 6]).is_broadcast());
    }

    #[test]
    fn parses_14_byte_header() {
        let frame = [
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, // dst broadcast
            0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, // src
            0x08, 0x06, // ethertype (ARP) — not used by the switch, only MACs are
        ];
        let h = EtherHeader::parse(&frame).unwrap();
        assert!(h.dst.is_broadcast());
        assert_eq!(h.src.to_string(), "aa:bb:cc:dd:ee:ff");
    }

    #[test]
    fn too_short_returns_none() {
        assert!(EtherHeader::parse(&[0u8; 10]).is_none());
    }
}
