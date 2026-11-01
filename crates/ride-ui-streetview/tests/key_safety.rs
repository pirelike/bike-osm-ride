//! The API key must never leak: not through `Debug` / `Display`, not through logging, not to
//! a host that is not Google (or a loopback mock). One test function: it edits the process
//! environment.

use ride_ui_streetview::error::SvError;
use ride_ui_streetview::mock::MockServer;
use ride_ui_streetview::url::{ApiKey, EmbedTarget, Endpoints, View, BASE_ENV};
use ride_ui_streetview::{MetadataClient, PanelStatus, PanelView};

const KEY: &str = "AIzaSyTOPSECRETKEYvalue_123456789012";

const ALL_ERRORS: [SvError; 9] = [
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

fn view() -> View {
    View {
        heading: 10.0,
        pitch: 2.0,
        fov: 90.0,
    }
}

#[test]
fn the_key_never_leaks() {
    let key = ApiKey::new(KEY).expect("key");

    // Debug / Display of everything that can hold or describe a key
    let embed = Endpoints::google().embed_url(
        &key,
        &EmbedTarget::Location {
            lat: 47.5,
            lng: 19.0,
            radius_m: 50,
        },
        view(),
    );
    let client = MetadataClient::new(Endpoints::google(), key.clone()).expect("client");
    let mut panel = PanelView::new(PanelStatus::Ok);
    panel.frames = [Some(embed.clone()), None];
    let mut text = format!(
        "{key:?} {key:#?} {embed:?} {embed:#?} {client:?} {client:#?} {panel:?} {panel:#?}"
    );
    for e in ALL_ERRORS {
        text.push_str(&format!("{e:?} {e} {} {}", e.title(), e.hint()));
        let status = PanelStatus::Error(e);
        text.push_str(&format!(
            "{status:?} {:?} {:?}",
            status.title(),
            status.hint()
        ));
    }
    assert!(
        !text.contains("TOPSECRET"),
        "key leaked into a Debug/Display: {text}"
    );

    // the only place the key legitimately appears is the finished request / embed URL
    assert!(embed.as_str().contains("TOPSECRET"));
    assert!(Endpoints::google()
        .metadata_url(&key, 1.0, 2.0, 50)
        .contains("TOPSECRET"));

    // no logging statements in the crate sources (a log line is the other way to leak)
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for entry in std::fs::read_dir(&src).expect("src dir").flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "rs") {
            let code = std::fs::read_to_string(&path).expect("read");
            // strip the test modules: they may print on purpose
            let production = code.split("#[cfg(test)]").next().unwrap_or("");
            for banned in [
                "println!",
                "eprintln!",
                "dbg!(",
                "log::",
                "tracing::",
                "print!(",
            ] {
                assert!(
                    !production.contains(banned),
                    "{} contains {banned}",
                    path.display()
                );
            }
        }
    }

    // the endpoint override only accepts loopback addresses: any other value falls back to
    // Google, so the key cannot be redirected to a third party through the environment
    for hostile in [
        "https://evil.example.com",
        "http://attacker.test:8080",
        "http://127.0.0.1.evil.com",
        "ftp://127.0.0.1",
        "",
    ] {
        std::env::set_var(BASE_ENV, hostile);
        let ep = Endpoints::from_env();
        assert!(!ep.is_local(), "{hostile}");
        let url = ep.metadata_url(&key, 1.0, 2.0, 50);
        assert!(
            url.starts_with("https://maps.googleapis.com/"),
            "{hostile}: {url}"
        );
        assert!(ep.trusts(&url));
    }
    std::env::remove_var(BASE_ENV);
    assert!(!Endpoints::from_env().is_local());

    // and the loopback mock does receive requests when it is configured
    let server = MockServer::streetview(Default::default());
    std::env::set_var(BASE_ENV, &server.base);
    let ep = Endpoints::from_env();
    assert!(ep.is_local());
    assert!(ep.trusts(&ep.metadata_url(&key, 1.0, 2.0, 50)));
    // ...but still refuses to send to a non-Google, non-loopback address
    assert!(!ep.trusts("https://evil.example.com/maps/api/streetview/metadata?key=x"));
    std::env::remove_var(BASE_ENV);
}
