//! Base URL of the app's own custom protocols (`tiles`, `svimg`) on the current webview.
//!
//! The handlers are registered by `ride-app` as Dioxus custom protocols, but the three webview
//! engines address them differently, so the same registration has three URL spellings:
//!
//! | webview | URL of the `tiles` protocol |
//! |---|---|
//! | WebView2 (Windows) | `http://tiles.localhost/<path>` |
//! | WKWebView (macOS), WebKitGTK (Linux) | `tiles://localhost/<path>` |
//! | Android WebView | `https://tiles.localhost/<path>` |
//!
//! Android gets the `https` form because Dioxus turns `with_https_scheme` on there. Every
//! handler only looks at [`uri().path()`], so the base is the only thing that has to differ.
//!
//! [`uri().path()`]: https://docs.rs/http/latest/http/uri/struct.Uri.html#method.path

/// Base URL (scheme + host, no trailing slash) of the custom protocol `scheme`, as the webview
/// of this platform expects it. `scheme` is a bare identifier such as `"tiles"`.
pub fn base(scheme: &str) -> String {
    // One of the three arms is compiled; they are mutually exclusive (Android is not Windows).
    #[cfg(target_os = "android")]
    {
        format!("https://{scheme}.localhost")
    }
    #[cfg(all(windows, not(target_os = "android")))]
    {
        format!("http://{scheme}.localhost")
    }
    #[cfg(not(any(windows, target_os = "android")))]
    {
        format!("{scheme}://localhost")
    }
}

/// `base(scheme)` followed by `path` (`"/a/b"` or `"a/b"`), for the components that build a URL.
pub fn url(scheme: &str, path: &str) -> String {
    format!("{}/{}", base(scheme), path.trim_start_matches('/'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_follows_the_webview_of_the_platform() {
        let tiles = base("tiles");
        if cfg!(target_os = "android") {
            assert_eq!(tiles, "https://tiles.localhost");
        } else if cfg!(windows) {
            assert_eq!(tiles, "http://tiles.localhost");
        } else {
            assert_eq!(tiles, "tiles://localhost");
        }
        // the scheme is the only variable part, and there is never a trailing slash
        assert_eq!(base("svimg"), tiles.replace("tiles", "svimg"));
        assert!(!tiles.ends_with('/'));
    }

    #[test]
    fn url_joins_with_exactly_one_slash() {
        assert_eq!(
            url("tiles", "osm/1/2/3.png"),
            format!("{}/osm/1/2/3.png", base("tiles"))
        );
        assert_eq!(
            url("tiles", "/osm/1/2/3.png"),
            format!("{}/osm/1/2/3.png", base("tiles"))
        );
        assert_eq!(url("svimg", ""), format!("{}/", base("svimg")));
    }
}
