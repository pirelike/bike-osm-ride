//! Error type of the Street View lookups, with Hungarian user-facing texts.
//!
//! No variant carries text that came from the network or from a URL: the API key can
//! therefore never end up in a `Debug` / `Display` of an error, a toast or a log line.
//! (`reqwest::Error` formats the full request URL, key included, so it is never formatted
//! anywhere in this crate: [`SvError::from_transport`] only looks at its *kind*.)

/// Why a metadata lookup (or the whole feature) cannot deliver a panorama.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SvError {
    /// No API key configured.
    #[error("Nincs megadva Google Maps API-kulcs.")]
    MissingKey,
    /// `REQUEST_DENIED` / HTTP 403: wrong key, API not enabled, billing off, restricted key.
    #[error("A Google elutasította az API-kulcsot.")]
    KeyRejected,
    /// `INVALID_REQUEST` / HTTP 400.
    #[error("A Google érvénytelennek találta a kérést.")]
    InvalidRequest,
    /// `OVER_QUERY_LIMIT` / HTTP 429.
    #[error("A Google lekérdezési kvótája elfogyott.")]
    QuotaExceeded,
    /// `ZERO_RESULTS` / `NOT_FOUND`: there is no panorama near the point.
    #[error("Itt nincs elérhető Street View.")]
    NoCoverage,
    /// Timeout, no connection, TLS problem, connection reset.
    #[error("Nincs kapcsolat a Google szolgáltatásával.")]
    Network,
    /// HTTP 5xx or `UNKNOWN_ERROR` (usually temporary).
    #[error("A Google szolgáltatása átmenetileg hibát jelzett.")]
    Server,
    /// A reply that is not the documented JSON, or carries an unusable panorama id.
    #[error("A Google váratlan választ adott.")]
    BadResponse,
    /// The endpoint is not a Google (or loopback test) address; nothing was sent.
    #[error("A cím nem a Google-é, ezért a kérést nem küldtük el.")]
    UntrustedEndpoint,
}

impl SvError {
    /// Short headline for the panel (always Hungarian).
    pub fn title(self) -> &'static str {
        match self {
            SvError::MissingKey => "Street View: API-kulcs szükséges",
            SvError::KeyRejected => "A Google elutasította a kulcsot",
            SvError::InvalidRequest => "Érvénytelen Street View-kérés",
            SvError::QuotaExceeded => "Elfogyott a Google-kvóta",
            SvError::NoCoverage => "Itt nincs Street View",
            SvError::Network => "Nincs kapcsolat a Google-lel",
            SvError::Server => "A Google átmenetileg nem válaszol",
            SvError::BadResponse => "Váratlan válasz a Google-től",
            SvError::UntrustedEndpoint => "Nem engedélyezett cím",
        }
    }

    /// A few words for the small header readout.
    pub fn short(self) -> &'static str {
        match self {
            SvError::MissingKey => "API-kulcs szükséges",
            SvError::KeyRejected => "Kulcs elutasítva",
            SvError::InvalidRequest => "Érvénytelen kérés",
            SvError::QuotaExceeded => "Kvóta elfogyott",
            SvError::NoCoverage => "Itt nincs Street View",
            SvError::Network => "Nincs kapcsolat",
            SvError::Server => "Google hiba",
            SvError::BadResponse => "Váratlan válasz",
            SvError::UntrustedEndpoint => "Nem engedélyezett cím",
        }
    }

    /// One or two sentences that tell the rider what to do about it.
    pub fn hint(self) -> &'static str {
        match self {
            SvError::MissingKey => {
                "Hozz létre API-kulcsot a Google Cloud konzolban, engedélyezd a Maps Embed API-t \
                 és a Street View Static API-t, majd másold be a kulcsot a Beállításokban."
            }
            SvError::KeyRejected => {
                "Ellenőrizd, hogy a kulcs helyes-e, a Maps Embed API és a Street View Static API \
                 engedélyezve van-e a projektben, be van-e kapcsolva a számlázás, és hogy a kulcs \
                 korlátozásai nem tiltják-e ezeket."
            }
            SvError::InvalidRequest => {
                "Valószínűleg hibás a kulcs formátuma. Másold be újra a Beállításokban."
            }
            SvError::QuotaExceeded => {
                "Próbáld később, vagy emeld a kvótát a Google Cloud konzolban. A panoráma-\
                 azonosítók mentésével az ismételt túrákon nincs szükség új lekérdezésre."
            }
            SvError::NoCoverage => "Ezen a szakaszon nincs Street View-felvétel.",
            SvError::Network => {
                "Ellenőrizd az internetkapcsolatot; hamarosan automatikusan újrapróbáljuk."
            }
            SvError::Server => "Hamarosan automatikusan újrapróbáljuk.",
            SvError::BadResponse => "Hamarosan automatikusan újrapróbáljuk.",
            SvError::UntrustedEndpoint => {
                "A Street View csak a Google szolgáltatásaihoz kapcsolódik."
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

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [SvError; 9] = [
        SvError::MissingKey,
        SvError::KeyRejected,
        SvError::InvalidRequest,
        SvError::QuotaExceeded,
        SvError::NoCoverage,
        SvError::Network,
        SvError::Server,
        SvError::BadResponse,
        SvError::UntrustedEndpoint,
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
