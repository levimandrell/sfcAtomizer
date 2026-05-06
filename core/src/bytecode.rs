//! M2 sequence bytecode v2 (`SEQ2`) — type skeleton.
//!
//! Locks the byte values for the opcode table and the region-header
//! layout per SPEC §14.3. Implementation (compiler / driver
//! interpreter) lands at M2.4+.

/// Region magic placed at the head of the M2 bytecode region in
/// ARAM. ASCII `"SEQ2"`.
pub const SEQUENCE_REGION_MAGIC: [u8; 4] = *b"SEQ2";

/// Bytecode version locked for the `multi_voice_atom` profile.
pub const BYTECODE_VERSION_M2: u8 = 2;

/// Total length of the SPEC §14.3 region header before bytecode.
pub const SEQUENCE_HEADER_LEN: usize = 8;

/// SPEC §14.3 opcode byte values. Locked at M2.0; driver and
/// compiler must agree on these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BytecodeOpcode {
    End = 0x00,
    Wait = 0x01,
    SetSrc = 0x10,
    SetVol = 0x11,
    Kon = 0x12,
    Koff = 0x13,
    VolSlide = 0x20,
    SetPitch = 0x30,
}

impl BytecodeOpcode {
    /// Inverse of `as u8`. Returns `None` for unrecognised bytes —
    /// the driver/compiler treats this as `bytecode_error`
    /// (status-flag bit) per SPEC §14.2 invalid-command rule.
    pub fn from_byte(b: u8) -> Option<Self> {
        Some(match b {
            0x00 => Self::End,
            0x01 => Self::Wait,
            0x10 => Self::SetSrc,
            0x11 => Self::SetVol,
            0x12 => Self::Kon,
            0x13 => Self::Koff,
            0x20 => Self::VolSlide,
            0x30 => Self::SetPitch,
            _ => return None,
        })
    }

    /// Number of operand bytes that follow each opcode. Used by the
    /// compiler / driver to step through the bytecode stream.
    pub fn operand_len(self) -> usize {
        match self {
            Self::End => 0,
            Self::Wait => 1,
            Self::SetSrc => 2,
            Self::SetVol => 3,
            Self::Kon => 1,
            Self::Koff => 1,
            Self::VolSlide => 4,
            Self::SetPitch => 3, // u8 voice + u16 pitch_le
        }
    }
}

/// Region header at the top of the bytecode region. Bytes layout
/// (little-endian for multi-byte fields):
///
/// ```text
/// 0x00..0x04  magic = "SEQ2"
/// 0x04        bytecode_version = 2
/// 0x05        reserved = 0
/// 0x06..0x08  bytecode_len_le
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SequenceHeader {
    pub bytecode_version: u8,
    pub bytecode_len: u16,
}

impl SequenceHeader {
    /// Encode the 8-byte header.
    pub fn to_bytes(&self) -> [u8; SEQUENCE_HEADER_LEN] {
        let mut out = [0u8; SEQUENCE_HEADER_LEN];
        out[0..4].copy_from_slice(&SEQUENCE_REGION_MAGIC);
        out[4] = self.bytecode_version;
        out[5] = 0; // reserved
        out[6..8].copy_from_slice(&self.bytecode_len.to_le_bytes());
        out
    }

    /// Best-effort parse. Returns `None` on magic mismatch or
    /// truncated input — the caller decides what's fatal.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < SEQUENCE_HEADER_LEN {
            return None;
        }
        if bytes[0..4] != SEQUENCE_REGION_MAGIC {
            return None;
        }
        Some(Self {
            bytecode_version: bytes[4],
            bytecode_len: u16::from_le_bytes([bytes[6], bytes[7]]),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opcode_byte_values_locked() {
        assert_eq!(BytecodeOpcode::End as u8, 0x00);
        assert_eq!(BytecodeOpcode::Wait as u8, 0x01);
        assert_eq!(BytecodeOpcode::SetSrc as u8, 0x10);
        assert_eq!(BytecodeOpcode::SetVol as u8, 0x11);
        assert_eq!(BytecodeOpcode::Kon as u8, 0x12);
        assert_eq!(BytecodeOpcode::Koff as u8, 0x13);
        assert_eq!(BytecodeOpcode::VolSlide as u8, 0x20);
        assert_eq!(BytecodeOpcode::SetPitch as u8, 0x30);
    }

    #[test]
    fn from_byte_round_trip_for_all_opcodes() {
        for op in [
            BytecodeOpcode::End,
            BytecodeOpcode::Wait,
            BytecodeOpcode::SetSrc,
            BytecodeOpcode::SetVol,
            BytecodeOpcode::Kon,
            BytecodeOpcode::Koff,
            BytecodeOpcode::VolSlide,
            BytecodeOpcode::SetPitch,
        ] {
            assert_eq!(BytecodeOpcode::from_byte(op as u8), Some(op));
        }
        assert_eq!(BytecodeOpcode::from_byte(0xFF), None);
        assert_eq!(BytecodeOpcode::from_byte(0x14), None); // gap between Koff and VolSlide
    }

    #[test]
    fn operand_lengths_match_spec() {
        // SPEC §14.3 — locked.
        assert_eq!(BytecodeOpcode::End.operand_len(), 0);
        assert_eq!(BytecodeOpcode::Wait.operand_len(), 1);
        assert_eq!(BytecodeOpcode::SetSrc.operand_len(), 2);
        assert_eq!(BytecodeOpcode::SetVol.operand_len(), 3);
        assert_eq!(BytecodeOpcode::Kon.operand_len(), 1);
        assert_eq!(BytecodeOpcode::Koff.operand_len(), 1);
        assert_eq!(BytecodeOpcode::VolSlide.operand_len(), 4);
        assert_eq!(BytecodeOpcode::SetPitch.operand_len(), 3);
    }

    #[test]
    fn header_round_trip() {
        let h = SequenceHeader {
            bytecode_version: BYTECODE_VERSION_M2,
            bytecode_len: 0x1234,
        };
        let bytes = h.to_bytes();
        assert_eq!(&bytes[0..4], b"SEQ2");
        assert_eq!(bytes[4], 2);
        assert_eq!(bytes[5], 0);
        assert_eq!(bytes[6], 0x34);
        assert_eq!(bytes[7], 0x12);
        let parsed = SequenceHeader::from_bytes(&bytes).expect("parses");
        assert_eq!(parsed, h);
    }

    #[test]
    fn header_rejects_bad_magic() {
        let mut bytes = [0u8; SEQUENCE_HEADER_LEN];
        bytes[0..4].copy_from_slice(b"SEQ1");
        assert!(SequenceHeader::from_bytes(&bytes).is_none());
    }
}
