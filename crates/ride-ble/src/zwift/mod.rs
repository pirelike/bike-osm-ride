//! Zwift's own BLE protocols: the Click and Play virtual-shifting controllers (this module's
//! main job) and the codec of the Zwift Hub trainer messages.
//!
//! The protocol was re-implemented in Rust from the public-domain (Unlicense) `virtual-cog`
//! project and from Makinolo's interoperability notes; see `NOTICE.md`. This is unofficial
//! interoperability work and is not affiliated with Zwift.
//!
//! * [`proto`]: protobuf2 wire format and the message subset (no I/O).
//! * [`crypto`]: ECDH P-256 + HKDF-SHA256 + AES-CCM session (no I/O).
//! * [`controller`]: the controller state machine with the shift debouncer (no I/O).
//!
//! The BLE side (subscribing, writing the hello, forwarding shifts to the hub) lives in
//! [`crate::multi`]; the trainer side of the Hub messages is codec-only for now.

pub mod controller;
pub mod crypto;
pub mod proto;

use ride_i18n::t;
use uuid::Uuid;

pub use controller::{play_shift_buttons, ControllerEvent, ControllerSession, ShiftDebouncer};
pub use crypto::{ClickCrypto, Encryption};
pub use proto::ProtoError;

/// Zwift's custom GATT service (Click, Play and Hub all expose it).
pub const SERVICE_UUID: Uuid = Uuid::from_u128(0x0000_0001_19ca_4651_86e5_fa29_dcdd_09d1);
/// Notify: asynchronous key pad and idle packets.
pub const MEASUREMENT_UUID: Uuid = Uuid::from_u128(0x0000_0002_19ca_4651_86e5_fa29_dcdd_09d1);
/// Write: the control point (hello, haptic commands, Hub commands).
pub const CONTROL_UUID: Uuid = Uuid::from_u128(0x0000_0003_19ca_4651_86e5_fa29_dcdd_09d1);
/// Indicate / notify: command responses, the answer to the hello.
pub const RESPONSE_UUID: Uuid = Uuid::from_u128(0x0000_0004_19ca_4651_86e5_fa29_dcdd_09d1);

/// Bluetooth company identifier Zwift uses in advertisements.
pub const MANUFACTURER_ID: u16 = 0x094A;

/// Which Zwift controller an advertisement belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControllerKind {
    /// Zwift Click (type byte 9).
    Click,
    /// Zwift Play, right controller (type byte 2).
    PlayRight,
    /// Zwift Play, left controller (type byte 3).
    PlayLeft,
}

impl ControllerKind {
    /// The controller a manufacturer-data type byte names; other values are not controllers
    /// (a Zwift Hub trainer advertises a different type).
    pub fn from_advertised(controller_type: u8) -> Option<Self> {
        match controller_type {
            9 => Some(Self::Click),
            2 => Some(Self::PlayRight),
            3 => Some(Self::PlayLeft),
            _ => None,
        }
    }
}

/// The controller type byte of Zwift's manufacturer data. The company identifier is normally
/// stripped by the stack, but some stacks keep it (`0x4A 0x09` first); both forms are accepted.
pub fn advertised_type(manufacturer: &[u8]) -> Option<u8> {
    match manufacturer {
        [0x4A, 0x09, kind, ..] => Some(*kind),
        [first, ..] => Some(*first),
        [] => None,
    }
}

/// Failures of the Zwift protocol layer. Messages are Hungarian: they can end up in a log
/// line that is shown to the rider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZwiftError {
    BadHandshake,
    MissingKey,
    NotReady,
    ShortPacket,
    AuthFailed,
    Crypto,
}

impl std::fmt::Display for ZwiftError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ZwiftError::BadHandshake => f.write_str(t("ble.zwift.a_kezfogas_valasza_hibas")),
            ZwiftError::MissingKey => f.write_str(t("ble.zwift.nincs_helyi_kulcs_a_kezfogashoz")),
            ZwiftError::NotReady => f.write_str(t("ble.zwift.a_csatorna_meg_nincs_kesz")),
            ZwiftError::ShortPacket => f.write_str(t("ble.zwift.a_csomag_tul_rovid")),
            ZwiftError::AuthFailed => f.write_str(t("ble.zwift.a_csomag_hitelesitese_sikertelen")),
            ZwiftError::Crypto => f.write_str(t("ble.zwift.a_titkositas_nem_sikerult")),
        }
    }
}

impl std::error::Error for ZwiftError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuids_are_the_documented_ones() {
        assert_eq!(
            SERVICE_UUID.to_string(),
            "00000001-19ca-4651-86e5-fa29dcdd09d1"
        );
        assert_eq!(
            MEASUREMENT_UUID.to_string(),
            "00000002-19ca-4651-86e5-fa29dcdd09d1"
        );
        assert_eq!(
            CONTROL_UUID.to_string(),
            "00000003-19ca-4651-86e5-fa29dcdd09d1"
        );
        assert_eq!(
            RESPONSE_UUID.to_string(),
            "00000004-19ca-4651-86e5-fa29dcdd09d1"
        );
    }

    #[test]
    fn controller_types() {
        assert_eq!(
            ControllerKind::from_advertised(9),
            Some(ControllerKind::Click)
        );
        assert_eq!(
            ControllerKind::from_advertised(2),
            Some(ControllerKind::PlayRight)
        );
        assert_eq!(
            ControllerKind::from_advertised(3),
            Some(ControllerKind::PlayLeft)
        );
        assert_eq!(ControllerKind::from_advertised(1), None);
        assert_eq!(ControllerKind::from_advertised(0), None);
    }

    #[test]
    fn manufacturer_data_with_and_without_company_id() {
        assert_eq!(advertised_type(&[9]), Some(9));
        assert_eq!(advertised_type(&[0x4A, 0x09, 9, 0x01]), Some(9));
        assert_eq!(
            advertised_type(&[0x4A, 0x09]),
            Some(0x4A),
            "too short: first byte"
        );
        assert_eq!(advertised_type(&[]), None);
    }
}
