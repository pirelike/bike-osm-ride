//! The narrow GATT abstraction the whole crate is written against. The real
//! implementation lives in `btle.rs` (btleplug); tests use the fake in `fake.rs`.
//! Nothing outside `btle.rs` touches btleplug types, so every layer above can be
//! exercised without Bluetooth hardware.

use std::future::Future;

use futures::stream::BoxStream;
use uuid::Uuid;

use crate::error::BleError;

/// Low 96 bits of the Bluetooth base UUID `0000xxxx-0000-1000-8000-00805F9B34FB`.
const SIG_BASE_LOW96: u128 = 0x0000_1000_8000_0080_5F9B_34FB;

/// Expand a 16-bit SIG-assigned number to a full UUID.
pub const fn sig_uuid(short: u16) -> Uuid {
    Uuid::from_u128(((short as u128) << 96) | SIG_BASE_LOW96)
}

/// The 16-bit SIG number of `uuid`, if it is one.
pub fn short_id(uuid: &Uuid) -> Option<u16> {
    let v = uuid.as_u128();
    let low_mask = (1u128 << 96) - 1;
    if v & low_mask == SIG_BASE_LOW96 && (v >> 112) == 0 {
        Some((v >> 96) as u16)
    } else {
        None
    }
}

/// GATT characteristic property bits that matter to us.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CharProps {
    pub read: bool,
    pub write: bool,
    pub write_without_response: bool,
    pub notify: bool,
    pub indicate: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GattChar {
    pub service: Uuid,
    pub uuid: Uuid,
    pub props: CharProps,
}

impl GattChar {
    pub fn short(&self) -> Option<u16> {
        short_id(&self.uuid)
    }
    pub fn service_short(&self) -> Option<u16> {
        short_id(&self.service)
    }
    pub fn pushes(&self) -> bool {
        self.props.notify || self.props.indicate
    }
}

/// One GATT notification / indication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    pub uuid: Uuid,
    pub value: Vec<u8>,
}

/// What an advertisement told us about a device.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Advertisement {
    pub name: Option<String>,
    pub services: Vec<Uuid>,
    pub rssi: Option<i16>,
    /// The Zwift controller type byte from the manufacturer data (`zwift::advertised_type`).
    pub zwift_controller: Option<u8>,
}

#[derive(Debug, Clone)]
pub enum AdapterEvent<P> {
    /// An advertisement (new or updated) from this peripheral.
    Seen(P),
    /// The OS reported a disconnect of the peripheral with this id.
    Lost(String),
}

/// A remote device we can talk GATT to. All futures are `Send` so they can run on
/// the multi-threaded hub runtime.
pub trait GattPeripheral: Clone + Send + Sync + 'static {
    /// Stable opaque id (the BLE address on Windows).
    fn id(&self) -> String;
    fn connect(&self) -> impl Future<Output = Result<(), BleError>> + Send;
    fn disconnect(&self) -> impl Future<Output = Result<(), BleError>> + Send;
    fn is_connected(&self) -> impl Future<Output = Result<bool, BleError>> + Send;
    fn discover_services(&self) -> impl Future<Output = Result<(), BleError>> + Send;
    /// Characteristics found by the last `discover_services`.
    fn characteristics(&self) -> Vec<GattChar>;
    fn read(&self, ch: &GattChar) -> impl Future<Output = Result<Vec<u8>, BleError>> + Send;
    fn write(
        &self,
        ch: &GattChar,
        data: &[u8],
        with_response: bool,
    ) -> impl Future<Output = Result<(), BleError>> + Send;
    fn subscribe(&self, ch: &GattChar) -> impl Future<Output = Result<(), BleError>> + Send;
    fn unsubscribe(&self, ch: &GattChar) -> impl Future<Output = Result<(), BleError>> + Send;
    /// The merged notification/indication stream of all subscribed characteristics.
    /// Obtain it *before* subscribing so no early packet is lost.
    fn notifications(
        &self,
    ) -> impl Future<Output = Result<BoxStream<'static, Notification>, BleError>> + Send;
}

/// The local Bluetooth adapter.
pub trait BleAdapter: Send + Sync + 'static {
    type Peripheral: GattPeripheral;
    /// Start an **unfiltered** scan (service-UUID filters are unreliable on WinRT).
    fn start_scan(&self) -> impl Future<Output = Result<(), BleError>> + Send;
    fn stop_scan(&self) -> impl Future<Output = Result<(), BleError>> + Send;
    fn events(
        &self,
    ) -> impl Future<Output = Result<BoxStream<'static, AdapterEvent<Self::Peripheral>>, BleError>> + Send;
    /// Peripherals the adapter currently knows about.
    fn peripherals(&self) -> impl Future<Output = Result<Vec<Self::Peripheral>, BleError>> + Send;
    fn advertisement(
        &self,
        p: &Self::Peripheral,
    ) -> impl Future<Output = Result<Option<Advertisement>, BleError>> + Send;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sig_uuid_round_trip() {
        let u = sig_uuid(0x2AD2);
        assert_eq!(u.to_string(), "00002ad2-0000-1000-8000-00805f9b34fb");
        assert_eq!(short_id(&u), Some(0x2AD2));
        assert_eq!(short_id(&sig_uuid(0x1826)), Some(0x1826));
        assert_eq!(short_id(&sig_uuid(0x0000)), Some(0));
    }

    #[test]
    fn vendor_uuids_have_no_short_id() {
        let v = Uuid::parse_str("6e40fec2-b5a3-f393-e0a9-e50e24dcca9e").unwrap();
        assert_eq!(short_id(&v), None);
        // 32-bit SIG UUIDs (upper 16 bits non-zero) are not 16-bit numbers either
        let thirty_two = Uuid::from_u128((0x1234_5678u128 << 96) | SIG_BASE_LOW96);
        assert_eq!(short_id(&thirty_two), None);
    }
}
