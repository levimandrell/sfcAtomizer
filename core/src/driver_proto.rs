//! Driver command protocol (SPEC §20.1).
//!
//! M1.0 ships protocol constants and enums only. The actual driver
//! ASM lands at M1.5. The host-side caller (also M1.5) uses these
//! constants to build host command packets and validate driver
//! acks; M1.0 just locks the byte values.

/// Host-issued command codes (SPEC §20.1, "Commands" table).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DriverCommand {
    Stop = 0x01,
    ResetToIpl = 0x02,
    /// Diagnostic no-op.
    Ping = 0x7F,
}

impl DriverCommand {
    pub fn from_u8(b: u8) -> Option<DriverCommand> {
        match b {
            0x01 => Some(DriverCommand::Stop),
            0x02 => Some(DriverCommand::ResetToIpl),
            0x7F => Some(DriverCommand::Ping),
            _ => None,
        }
    }

    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}

/// Driver-emitted ack codes (SPEC §20.1, "Acks" block).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DriverAck {
    StopAck = 0x81,
    ResetAck = 0x82,
    /// Returned for the `PING` command.
    PingAck = 0xFF,
    /// Returned when the host sends a `command_code` the driver
    /// doesn't recognise. Status flag `error` is also set.
    InvalidCommand = 0xEE,
}

impl DriverAck {
    pub fn from_u8(b: u8) -> Option<DriverAck> {
        match b {
            0x81 => Some(DriverAck::StopAck),
            0x82 => Some(DriverAck::ResetAck),
            0xFF => Some(DriverAck::PingAck),
            0xEE => Some(DriverAck::InvalidCommand),
            _ => None,
        }
    }

    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}

// =============================================================================
// Status flags (driver_out_3) — SPEC §20.1
// =============================================================================

/// Bit-flag wrapper for `driver_out_3`. Reserved bits 5..7 must be
/// zero; [`StatusFlags::is_valid`] checks that.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatusFlags(pub u8);

impl StatusFlags {
    pub const VOICE0_ACTIVE: u8 = 0x01;
    pub const ECHO_ENABLED: u8 = 0x02;
    pub const STOPPED: u8 = 0x04;
    pub const ERROR: u8 = 0x08;
    pub const RESET_TO_IPL_PENDING: u8 = 0x10;

    /// Mask of all valid bits (0..=4); reserved bits 5..7 must be 0.
    pub const VALID_MASK: u8 = Self::VOICE0_ACTIVE
        | Self::ECHO_ENABLED
        | Self::STOPPED
        | Self::ERROR
        | Self::RESET_TO_IPL_PENDING;

    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn bits(self) -> u8 {
        self.0
    }

    /// Reserved bits 5..7 must be zero per SPEC §20.1.
    pub const fn is_valid(&self) -> bool {
        (self.0 & !Self::VALID_MASK) == 0
    }

    pub const fn voice0_active(self) -> bool {
        (self.0 & Self::VOICE0_ACTIVE) != 0
    }
    pub const fn echo_enabled(self) -> bool {
        (self.0 & Self::ECHO_ENABLED) != 0
    }
    pub const fn stopped(self) -> bool {
        (self.0 & Self::STOPPED) != 0
    }
    pub const fn error(self) -> bool {
        (self.0 & Self::ERROR) != 0
    }
    pub const fn reset_to_ipl_pending(self) -> bool {
        (self.0 & Self::RESET_TO_IPL_PENDING) != 0
    }
}

// =============================================================================
// Driver ready signature — SPEC §20.1
// =============================================================================

/// First byte of the driver-ready signature (`driver_out_0`).
pub const DRIVER_READY_SIG_0: u8 = 0xA5;
/// Second byte of the driver-ready signature (`driver_out_1`).
pub const DRIVER_READY_SIG_1: u8 = 0x5A;
/// Driver version on `driver_out_2` for M1.
pub const DRIVER_VERSION_M1: u8 = 0x01;

// =============================================================================
// Bounded host-side spin counts — SPEC §19.2
// =============================================================================

/// Bounded spin counts the 65816 host uses to fail fast on missed
/// IPL / driver acknowledgements (SPEC §19.2).
pub mod host_timeouts {
    pub const WAIT_IPL_READY_POLLS: u32 = 0x0020_0000;
    pub const WAIT_BLOCK_KICK_ACK_POLLS: u32 = 0x0002_0000;
    pub const WAIT_BYTE_ACK_POLLS: u32 = 0x0000_4000;
    pub const WAIT_DRIVER_READY_POLLS: u32 = 0x0020_0000;
    pub const WAIT_COMMAND_ACK_POLLS: u32 = 0x0002_0000;
    pub const WAIT_RESET_TO_IPL_POLLS: u32 = 0x0020_0000;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn driver_command_byte_values_match_spec() {
        assert_eq!(DriverCommand::Stop.as_u8(), 0x01);
        assert_eq!(DriverCommand::ResetToIpl.as_u8(), 0x02);
        assert_eq!(DriverCommand::Ping.as_u8(), 0x7F);
    }

    #[test]
    fn driver_command_round_trip() {
        for c in [
            DriverCommand::Stop,
            DriverCommand::ResetToIpl,
            DriverCommand::Ping,
        ] {
            assert_eq!(DriverCommand::from_u8(c.as_u8()), Some(c));
        }
        for b in [0x00u8, 0x03, 0x10, 0x80, 0xFF] {
            assert_eq!(DriverCommand::from_u8(b), None);
        }
    }

    #[test]
    fn driver_ack_byte_values_match_spec() {
        assert_eq!(DriverAck::StopAck.as_u8(), 0x81);
        assert_eq!(DriverAck::ResetAck.as_u8(), 0x82);
        assert_eq!(DriverAck::PingAck.as_u8(), 0xFF);
        assert_eq!(DriverAck::InvalidCommand.as_u8(), 0xEE);
    }

    #[test]
    fn driver_ack_round_trip() {
        for a in [
            DriverAck::StopAck,
            DriverAck::ResetAck,
            DriverAck::PingAck,
            DriverAck::InvalidCommand,
        ] {
            assert_eq!(DriverAck::from_u8(a.as_u8()), Some(a));
        }
        for b in [0x00u8, 0x80, 0xED, 0xFE] {
            assert_eq!(DriverAck::from_u8(b), None);
        }
    }

    #[test]
    fn status_flag_bit_values_match_spec() {
        assert_eq!(StatusFlags::VOICE0_ACTIVE, 0x01);
        assert_eq!(StatusFlags::ECHO_ENABLED, 0x02);
        assert_eq!(StatusFlags::STOPPED, 0x04);
        assert_eq!(StatusFlags::ERROR, 0x08);
        assert_eq!(StatusFlags::RESET_TO_IPL_PENDING, 0x10);
    }

    #[test]
    fn status_flags_is_valid_accepts_allowed_patterns() {
        for bits in [
            0x00u8,
            StatusFlags::VOICE0_ACTIVE,
            StatusFlags::ECHO_ENABLED,
            StatusFlags::STOPPED,
            StatusFlags::ERROR,
            StatusFlags::RESET_TO_IPL_PENDING,
            StatusFlags::VALID_MASK,
            StatusFlags::STOPPED | StatusFlags::ERROR,
        ] {
            assert!(
                StatusFlags(bits).is_valid(),
                "expected {bits:#04X} to be valid"
            );
        }
    }

    #[test]
    fn status_flags_is_valid_rejects_reserved_bits() {
        for bits in [0x20u8, 0x40, 0x80, 0xE0, 0xFF, 0x21, 0x05 | 0x80] {
            assert!(
                !StatusFlags(bits).is_valid(),
                "expected {bits:#04X} to be invalid (reserved bit set)"
            );
        }
    }

    #[test]
    fn status_flag_accessors() {
        let s = StatusFlags(StatusFlags::VOICE0_ACTIVE | StatusFlags::ECHO_ENABLED);
        assert!(s.voice0_active());
        assert!(s.echo_enabled());
        assert!(!s.stopped());
        assert!(!s.error());
        assert!(!s.reset_to_ipl_pending());
    }

    #[test]
    fn ready_signature_constants_match_spec() {
        assert_eq!(DRIVER_READY_SIG_0, 0xA5);
        assert_eq!(DRIVER_READY_SIG_1, 0x5A);
        assert_eq!(DRIVER_VERSION_M1, 0x01);
    }

    #[test]
    fn host_timeout_constants_match_spec() {
        use super::host_timeouts::*;
        assert_eq!(WAIT_IPL_READY_POLLS, 0x0020_0000);
        assert_eq!(WAIT_BLOCK_KICK_ACK_POLLS, 0x0002_0000);
        assert_eq!(WAIT_BYTE_ACK_POLLS, 0x0000_4000);
        assert_eq!(WAIT_DRIVER_READY_POLLS, 0x0020_0000);
        assert_eq!(WAIT_COMMAND_ACK_POLLS, 0x0002_0000);
        assert_eq!(WAIT_RESET_TO_IPL_POLLS, 0x0020_0000);
    }
}
