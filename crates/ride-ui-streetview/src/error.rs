//! Error type of the Street View lookups; the user-facing texts live in the catalogs.
//!
//! No variant carries text that came from the network or from a URL: the API key can
//! therefore never end up in a `Debug` / `Display` of an error, a toast or a log line.
//! (`reqwest::Error` formats the full request URL, key included, so it is never formatted
//! anywhere in this crate: [`SvError::from_transport`] only looks at its *kind*.)

/// Why a metadata lookup (or the whole feature) cannot deliver a panorama.
use ride_i18n::t;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SvError {
    /// No API key configured.
    MissingKey,
    /// `REQUEST_DENIED` / HTTP 403: wrong key, API not enabled, billing off, restricted key.
    KeyRejected,
    /// `INVALID_REQUEST` / HTTP 400.
    InvalidRequest,
    /// `OVER_QUERY_LIMIT` / HTTP 429.
    QuotaExceeded,
    /// `ZERO_RESULTS` / `NOT_FOUND`: there is no panorama near the point.
    NoCoverage,
    /// Timeout, no connection, TLS problem, connection reset.
    Network,
    /// HTTP 5xx or `UNKNOWN_ERROR` (usually temporary).
    Server,
    /// A reply that is not the documented JSON, or carries an unusable panorama id.
    BadResponse,
    /// The endpoint is not a Google (or loopback test) address; nothing was sent.
    UntrustedEndpoint,
    /// Local image persistence failed; stop before spending more image quota.
    Storage(std::io::ErrorKind),
}

impl SvError {
    /// Short headline for the panel, in the UI language.
    pub fn title(self) -> &'static str {
        match self {
            SvError::MissingKey => t("ui_streetview.error.street_view_api_kulcs_szukseges"),
            SvError::KeyRejected => t("ui_streetview.error.a_google_elutasitotta_a_kulcsot"),
            SvError::InvalidRequest => t("ui_streetview.error.ervenytelen_street_view_keres"),
            SvError::QuotaExceeded => t("ui_streetview.error.elfogyott_a_google_kvota"),
            SvError::NoCoverage => t("ui_streetview.error.itt_nincs_street_view"),
            SvError::Network => t("ui_streetview.error.nincs_kapcsolat_a_google_lel"),
            SvError::Server => t("ui_streetview.error.a_google_atmenetileg_nem_valaszol"),
            SvError::BadResponse => t("ui_streetview.error.varatlan_valasz_a_google_tol"),
            SvError::Storage(_) => t("ui_streetview.error.storage_failed"),
            SvError::UntrustedEndpoint => t("ui_streetview.error.nem_engedelyezett_cim"),
        }
    }

    /// A few words for the small header readout.
    pub fn short(self) -> &'static str {
        match self {
            SvError::MissingKey => t("ui_streetview.error.api_kulcs_szukseges"),
            SvError::KeyRejected => t("ui_streetview.error.kulcs_elutasitva"),
            SvError::InvalidRequest => t("ui_streetview.error.ervenytelen_keres"),
            SvError::QuotaExceeded => t("ui_streetview.error.kvota_elfogyott"),
            SvError::NoCoverage => t("ui_streetview.error.itt_nincs_street_view"),
            SvError::Network => t("ui_streetview.error.nincs_kapcsolat"),
            SvError::Server => t("ui_streetview.error.google_hiba"),
            SvError::BadResponse => t("ui_streetview.error.varatlan_valasz"),
            SvError::Storage(std::io::ErrorKind::StorageFull) => {
                t("ui_streetview.error.storage_full")
            }
            SvError::Storage(std::io::ErrorKind::PermissionDenied) => {
                t("ui_streetview.error.storage_denied")
            }
            SvError::Storage(_) => t("ui_streetview.error.storage_failed"),
            SvError::UntrustedEndpoint => t("ui_streetview.error.nem_engedelyezett_cim"),
        }
    }

    /// One or two sentences that tell the rider what to do about it.
    pub fn hint(self) -> &'static str {
        match self {
            SvError::MissingKey => {
                t("ui_streetview.error.hozz_letre_api_kulcsot_a_google_cloud_konzolban_enge")
            }
            SvError::KeyRejected => {
                t("ui_streetview.error.ellenorizd_hogy_a_kulcs_helyes_e_a_maps_embed_api_es")
            }
            SvError::InvalidRequest => {
                t("ui_streetview.error.valoszinuleg_hibas_a_kulcs_formatuma_masold_be_ujra_")
            }
            SvError::QuotaExceeded => {
                t("ui_streetview.error.probald_kesobb_vagy_emeld_a_kvotat_a_google_cloud_ko")
            }
            SvError::NoCoverage => {
                t("ui_streetview.error.ezen_a_szakaszon_nincs_street_view_felvetel")
            }
            SvError::Network => {
                t("ui_streetview.error.ellenorizd_az_internetkapcsolatot_hamarosan_automati")
            }
            SvError::Server => t("ui_streetview.error.hamarosan_automatikusan_ujraprobaljuk"),
            SvError::BadResponse => t("ui_streetview.error.hamarosan_automatikusan_ujraprobaljuk"),
            SvError::Storage(_) => t("ui_streetview.error.storage_hint"),
            SvError::UntrustedEndpoint => {
                t("ui_streetview.error.a_street_view_csak_a_google_szolgaltatasaihoz_kapcso")
            }
        }
    }

    /// Will the same request fail the same way until the user changes something?
    /// (The prefetcher stops for these instead of hammering the API.)
    pub fn is_permanent(self) -> bool {
        matches!(
            self,
            SvError::MissingKey
                | SvError::KeyRejected
                | SvError::InvalidRequest
                | SvError::UntrustedEndpoint
                | SvError::Storage(_)
        )
    }

    /// A normal answer ("no panorama here"), not a failure of the service.
    pub fn is_no_coverage(self) -> bool {
        self == SvError::NoCoverage
    }

    /// Worth retrying later (network, 5xx, quota).
    pub fn is_transient(self) -> bool {
        matches!(
            self,
            SvError::Network | SvError::Server | SvError::BadResponse | SvError::QuotaExceeded
        )
    }

    /// Maps the Street View metadata `status` field.
    pub fn from_status(status: &str) -> Option<SvError> {
        match status {
            "OK" => None,
            "ZERO_RESULTS" | "NOT_FOUND" => Some(SvError::NoCoverage),
            "REQUEST_DENIED" => Some(SvError::KeyRejected),
            "OVER_QUERY_LIMIT" => Some(SvError::QuotaExceeded),
            "INVALID_REQUEST" => Some(SvError::InvalidRequest),
            "UNKNOWN_ERROR" => Some(SvError::Server),
            _ => Some(SvError::BadResponse),
        }
    }

    /// Maps an HTTP status when the body had no usable `status` field.
    pub fn from_http(code: u16) -> SvError {
        match code {
            400 => SvError::InvalidRequest,
            401 | 403 => SvError::KeyRejected,
            429 => SvError::QuotaExceeded,
            500..=599 => SvError::Server,
            _ => SvError::BadResponse,
        }
    }

    /// Classifies a transport error by kind only; its text (which contains the URL and so
    /// the key) is dropped on purpose.
    pub(crate) fn from_transport(e: &reqwest::Error) -> SvError {
        if e.is_timeout() || e.is_connect() || e.is_request() {
            SvError::Network
        } else if e.is_decode() || e.is_body() {
            SvError::BadResponse
        } else {
            SvError::Network
        }
    }
}

impl std::fmt::Display for SvError {
    /// The headline, in the UI language: `Display` is what a log line or a toast shows.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.title())
    }
}

impl std::error::Error for SvError {}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [SvError; 10] = [
        SvError::MissingKey,
        SvError::KeyRejected,
        SvError::InvalidRequest,
        SvError::QuotaExceeded,
        SvError::NoCoverage,
        SvError::Network,
        SvError::Server,
        SvError::BadResponse,
        SvError::UntrustedEndpoint,
        SvError::Storage(std::io::ErrorKind::Other),
    ];

    #[test]
    fn status_mapping_follows_the_documentation() {
        assert_eq!(SvError::from_status("OK"), None);
        assert_eq!(
            SvError::from_status("ZERO_RESULTS"),
            Some(SvError::NoCoverage)
        );
        assert_eq!(SvError::from_status("NOT_FOUND"), Some(SvError::NoCoverage));
        assert_eq!(
            SvError::from_status("REQUEST_DENIED"),
            Some(SvError::KeyRejected)
        );
        assert_eq!(
            SvError::from_status("OVER_QUERY_LIMIT"),
            Some(SvError::QuotaExceeded)
        );
        assert_eq!(
            SvError::from_status("INVALID_REQUEST"),
            Some(SvError::InvalidRequest)
        );
        assert_eq!(SvError::from_status("UNKNOWN_ERROR"), Some(SvError::Server));
        assert_eq!(SvError::from_status("???"), Some(SvError::BadResponse));
        assert_eq!(SvError::from_http(403), SvError::KeyRejected);
        assert_eq!(SvError::from_http(429), SvError::QuotaExceeded);
        assert_eq!(SvError::from_http(503), SvError::Server);
        assert_eq!(SvError::from_http(302), SvError::BadResponse);
    }

    #[test]
    fn every_error_has_hungarian_texts_and_class() {
        for e in ALL {
            assert!(!e.title().is_empty() && !e.hint().is_empty() && !e.short().is_empty());
            assert!(e.short().chars().count() <= 30, "{e:?}");
            assert!(!e.to_string().is_empty());
            // a permanent error is never "transient"
            assert!(!(e.is_permanent() && e.is_transient()), "{e:?}");
        }
        assert!(SvError::MissingKey.hint().contains("Maps Embed API"));
        assert!(SvError::MissingKey
            .hint()
            .contains("Street View Static API"));
        assert_eq!(SvError::NoCoverage.title(), "Itt nincs Street View");
        assert!(SvError::KeyRejected.is_permanent());
        assert!(!SvError::QuotaExceeded.is_permanent());
        assert!(SvError::NoCoverage.is_no_coverage());
    }
}
