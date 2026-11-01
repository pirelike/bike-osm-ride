//! Error type of the BLE layer. The messages are Hungarian because they can end up in
//! a toast (`SensorEvent::Notice` / `ConnState::Failed`).

use std::future::Future;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BleError {
    /// No adapter, Bluetooth switched off, permission denied, runtime start-up failure.
    #[error("A Bluetooth nem érhető el: {0}")]
    Unavailable(String),
    /// The peripheral is not (or no longer) known to the adapter.
    #[error("Az eszköz nem található (indíts új keresést)")]
    NotFound,
    /// The link dropped while an operation was in flight.
    #[error("Nincs kapcsolat az eszközzel")]
    NotConnected,
    /// An operation took longer than its budget.
    #[error("Időtúllépés: {0}")]
    Timeout(String),
    /// The connected device does not expose what its role needs.
    #[error("Az eszköz nem kínálja a szükséges szolgáltatást: {0}")]
    MissingService(String),
    /// Anything else the platform stack reported.
    #[error("Bluetooth hiba: {0}")]
    Gatt(String),
}

impl BleError {
    /// A retry cannot help (wrong kind of device, permission problem).
    pub fn is_permanent(&self) -> bool {
        matches!(self, BleError::MissingService(_) | BleError::Unavailable(_))
    }

    /// The link itself is gone (as opposed to a single failed request).
    pub fn is_link_lost(&self) -> bool {
        matches!(self, BleError::NotConnected | BleError::NotFound)
    }
}

impl From<btleplug::Error> for BleError {
    fn from(e: btleplug::Error) -> Self {
        use btleplug::Error as E;
        match e {
            E::NotConnected => BleError::NotConnected,
            E::DeviceNotFound => BleError::NotFound,
            E::PermissionDenied => BleError::Unavailable("hozzáférés megtagadva".into()),
            E::NoAdapterAvailable => BleError::Unavailable("nincs Bluetooth-adapter".into()),
            E::TimedOut(d) => BleError::Timeout(format!("{} mp", d.as_secs())),
            other => BleError::Gatt(other.to_string()),
        }
    }
}

/// Run `fut` with a deadline; the label names the step in the timeout message.
pub(crate) async fn bounded<T>(
    what: &str,
    limit: Duration,
    fut: impl Future<Output = Result<T, BleError>>,
) -> Result<T, BleError> {
    match tokio::time::timeout(limit, fut).await {
        Ok(r) => r,
        Err(_) => Err(BleError::Timeout(what.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapping_from_btleplug() {
        assert_eq!(
            BleError::from(btleplug::Error::NotConnected),
            BleError::NotConnected
        );
        assert_eq!(
            BleError::from(btleplug::Error::DeviceNotFound),
            BleError::NotFound
        );
        assert!(BleError::from(btleplug::Error::PermissionDenied).is_permanent());
        assert!(BleError::from(btleplug::Error::NotConnected).is_link_lost());
        assert!(!BleError::Gatt("x".into()).is_permanent());
    }

    #[tokio::test]
    async fn bounded_times_out() {
        let r: Result<(), _> = bounded("próba", Duration::from_millis(20), async {
            tokio::time::sleep(Duration::from_secs(5)).await;
            Ok(())
        })
        .await;
        assert_eq!(r, Err(BleError::Timeout("próba".into())));
    }
}
