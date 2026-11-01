//! The btleplug (WinRT / BlueZ / CoreBluetooth) implementation of the transport traits.
//! This is the only module that names btleplug types.

use std::time::Duration;

use btleplug::api::{
    Central, CentralEvent, CharPropFlags, Characteristic, Manager as _, Peripheral as _,
    ScanFilter, WriteType,
};
use btleplug::platform::{Adapter, Manager, Peripheral};
use futures::stream::{BoxStream, StreamExt};

use crate::error::{bounded, BleError};
use crate::transport::{
    AdapterEvent, Advertisement, BleAdapter, CharProps, GattChar, GattPeripheral, Notification,
};

/// How long creating the platform manager and listing adapters may take.
const INIT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone)]
pub struct BtleAdapter {
    inner: Adapter,
}

impl BtleAdapter {
    /// Create the platform manager and take the first adapter.
    pub async fn init() -> Result<Self, BleError> {
        let manager = bounded("a Bluetooth-kezelő indítása", INIT_TIMEOUT, async {
            Manager::new().await.map_err(BleError::from)
        })
        .await?;
        let adapters = bounded("az adapterek listázása", INIT_TIMEOUT, async {
            manager.adapters().await.map_err(BleError::from)
        })
        .await?;
        adapters
            .into_iter()
            .next()
            .map(|inner| Self { inner })
            .ok_or_else(|| BleError::Unavailable("nincs Bluetooth-adapter a gépen".into()))
    }
}

#[derive(Clone)]
pub struct BtlePeripheral {
    inner: Peripheral,
}

impl BtlePeripheral {
    fn raw_char(&self, ch: &GattChar) -> Result<Characteristic, BleError> {
        self.inner
            .characteristics()
            .into_iter()
            .find(|c| c.uuid == ch.uuid && c.service_uuid == ch.service)
            .ok_or_else(|| BleError::Gatt("ismeretlen karakterisztika".into()))
    }
}

fn props_of(flags: CharPropFlags) -> CharProps {
    CharProps {
        read: flags.contains(CharPropFlags::READ),
        write: flags.contains(CharPropFlags::WRITE),
        write_without_response: flags.contains(CharPropFlags::WRITE_WITHOUT_RESPONSE),
        notify: flags.contains(CharPropFlags::NOTIFY),
        indicate: flags.contains(CharPropFlags::INDICATE),
    }
}

impl GattPeripheral for BtlePeripheral {
    fn id(&self) -> String {
        self.inner.id().to_string()
    }

    async fn connect(&self) -> Result<(), BleError> {
        self.inner.connect().await.map_err(BleError::from)
    }

    async fn disconnect(&self) -> Result<(), BleError> {
        self.inner.disconnect().await.map_err(BleError::from)
    }

    async fn is_connected(&self) -> Result<bool, BleError> {
        self.inner.is_connected().await.map_err(BleError::from)
    }

    async fn discover_services(&self) -> Result<(), BleError> {
        self.inner.discover_services().await.map_err(BleError::from)
    }

    fn characteristics(&self) -> Vec<GattChar> {
        self.inner
            .characteristics()
            .into_iter()
            .map(|c| GattChar {
                service: c.service_uuid,
                uuid: c.uuid,
                props: props_of(c.properties),
            })
            .collect()
    }

    async fn read(&self, ch: &GattChar) -> Result<Vec<u8>, BleError> {
        let raw = self.raw_char(ch)?;
        self.inner.read(&raw).await.map_err(BleError::from)
    }

    async fn write(&self, ch: &GattChar, data: &[u8], with_response: bool) -> Result<(), BleError> {
        let raw = self.raw_char(ch)?;
        let kind = if with_response {
            WriteType::WithResponse
        } else {
            WriteType::WithoutResponse
        };
        self.inner
            .write(&raw, data, kind)
            .await
            .map_err(BleError::from)
    }

    async fn subscribe(&self, ch: &GattChar) -> Result<(), BleError> {
        let raw = self.raw_char(ch)?;
        self.inner.subscribe(&raw).await.map_err(BleError::from)
    }

    async fn unsubscribe(&self, ch: &GattChar) -> Result<(), BleError> {
        let raw = self.raw_char(ch)?;
        self.inner.unsubscribe(&raw).await.map_err(BleError::from)
    }

    async fn notifications(&self) -> Result<BoxStream<'static, Notification>, BleError> {
        let stream = self.inner.notifications().await.map_err(BleError::from)?;
        Ok(stream
            .map(|n| Notification {
                uuid: n.uuid,
                value: n.value,
            })
            .boxed())
    }
}

impl BleAdapter for BtleAdapter {
    type Peripheral = BtlePeripheral;

    async fn start_scan(&self) -> Result<(), BleError> {
        // Unfiltered on purpose: WinRT service-UUID filters are unreliable.
        self.inner
            .start_scan(ScanFilter::default())
            .await
            .map_err(BleError::from)
    }

    async fn stop_scan(&self) -> Result<(), BleError> {
        self.inner.stop_scan().await.map_err(BleError::from)
    }

    async fn events(&self) -> Result<BoxStream<'static, AdapterEvent<BtlePeripheral>>, BleError> {
        let stream = self.inner.events().await.map_err(BleError::from)?;
        let adapter = self.inner.clone();
        Ok(stream
            .filter_map(move |event| {
                let adapter = adapter.clone();
                async move {
                    match event {
                        CentralEvent::DeviceDiscovered(id)
                        | CentralEvent::DeviceUpdated(id)
                        | CentralEvent::ManufacturerDataAdvertisement { id, .. }
                        | CentralEvent::ServiceDataAdvertisement { id, .. }
                        | CentralEvent::ServicesAdvertisement { id, .. }
                        | CentralEvent::RssiUpdate { id, .. } => adapter
                            .peripheral(&id)
                            .await
                            .ok()
                            .map(|inner| AdapterEvent::Seen(BtlePeripheral { inner })),
                        CentralEvent::DeviceDisconnected(id) => {
                            Some(AdapterEvent::Lost(id.to_string()))
                        }
                        _ => None,
                    }
                }
            })
            .boxed())
    }

    async fn peripherals(&self) -> Result<Vec<BtlePeripheral>, BleError> {
        let list = self.inner.peripherals().await.map_err(BleError::from)?;
        Ok(list
            .into_iter()
            .map(|inner| BtlePeripheral { inner })
            .collect())
    }

    async fn advertisement(&self, p: &BtlePeripheral) -> Result<Option<Advertisement>, BleError> {
        let props = p.inner.properties().await.map_err(BleError::from)?;
        Ok(props.map(|pr| Advertisement {
            name: pr.local_name.or(pr.advertisement_name),
            services: pr.services,
            rssi: pr.rssi,
            zwift_controller: pr
                .manufacturer_data
                .get(&crate::zwift::MANUFACTURER_ID)
                .and_then(|data| crate::zwift::advertised_type(data)),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Creates the btleplug manager and prints how many adapters exist. It never scans,
    /// connects, or prints device names. Run manually with
    /// `cargo test -p ride-ble -- --ignored smoke_adapter_count --nocapture`.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "needs the machine's Bluetooth stack; run manually"]
    async fn smoke_adapter_count() {
        let manager = Manager::new()
            .await
            .expect("a Bluetooth-kezelő létrehozása");
        let count = manager.adapters().await.expect("adapterek listázása").len();
        println!("Bluetooth adapters: {count}");
    }

    #[test]
    fn property_flags_map() {
        let p = props_of(CharPropFlags::READ | CharPropFlags::NOTIFY | CharPropFlags::WRITE);
        assert!(p.read && p.notify && p.write);
        assert!(!p.indicate && !p.write_without_response);
    }
}
