//! Error type of the BLE layer. The messages are Hungarian because they can end up in
//! a toast (`SensorEvent::Notice` / `ConnState::Failed`).

use ride_i18n::{t, tf};
use std::future::Future;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BleError {
    /// No adapter, Bluetooth switched off, permission denied, runtime start-up failure.
    Unavailable(String),
    /// The peripheral is not (or no longer) known to the adapter.
    NotFound,
    /// The link dropped while an operation was in flight.
    NotConnected,
    /// An operation took longer than its budget.
    Timeout(String),
    /// The connected device does not expose what its role needs.
    MissingService(String),
    /// Anything else the platform stack reported.
    Gatt(String),
}

impl std::fmt::Display for BleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // the catalogue sentences carry the `{reason}` placeholder themselves
            BleError::Unavailable(reason) => f.write_str(&tf(
                "ble.error.a_bluetooth_nem_erheto_el_0",
                &[("reason", reason)],
            )),
            BleError::NotFound => {
                f.write_str(t("ble.error.az_eszkoz_nem_talalhato_indits_uj_keresest"))
            }
            BleError::NotConnected => f.write_str(t("ble.error.nincs_kapcsolat_az_eszkozzel")),
            BleError::Timeout(reason) => {
                f.write_str(&tf("ble.error.idotullepes_0", &[("reason", reason)]))
            }
            BleError::MissingService(reason) => f.write_str(&tf(
                "ble.error.az_eszkoz_nem_kinalja_a_szukseges_szolgaltatast_0",
                &[("reason", reason)],
            )),
            BleError::Gatt(reason) => {
                f.write_str(&tf("ble.error.bluetooth_hiba", &[("reason", reason)]))
            }
        }
    }
}

impl std::error::Error for BleError {}

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
            E::PermissionDenied => {
                BleError::Unavailable(t("ble.error.hozzaferes_megtagadva").into())
            }
            E::NoAdapterAvailable => {
                BleError::Unavailable(t("ble.error.nincs_bluetooth_adapter").into())
            }
            E::TimedOut(d) => BleError::Timeout(tf(
                "ble.error.seconds_short",
                &[("s", &d.as_secs().to_string())],
            )),
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
    fn the_reason_fills_the_placeholder_once() {
        let text = BleError::Timeout("nincs válasz".into()).to_string();
        assert!(!text.contains('{'), "{text}");
        assert_eq!(text.matches("nincs válasz").count(), 1, "{text}");
        for e in [
            BleError::Unavailable("q1".into()),
            BleError::MissingService("q2".into()),
            BleError::Gatt("q3".into()),
        ] {
            let text = e.to_string();
            assert!(
                !text.contains('{') && text.matches('q').count() == 1,
                "{text}"
            );
        }
        let parse = crate::parsers::ParseError::Invalid("y").to_string();
        assert!(!parse.contains('{'), "{parse}");
    }

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
