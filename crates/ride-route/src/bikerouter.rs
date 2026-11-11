//! bikerouter.de / brouter-web share links → BRouter API request → GPX.
//!
//! # Supported link formats
//!
//! Verified against `nrenner/brouter-web` (MIT), commit `d69a38f` (2024-10-22):
//! `js/plugin/leaflet-fullHash.js` (`L.Hash.parseHash` / `formatHash`),
//! `js/index.js` (`onHashChangeCb` → `url2params`, `onInvalidHashChangeCb`,
//! `additionalCb`) and `js/router/BRouter.js` (`getUrl`, `getUrlParams`,
//! `parseUrlParams`, `_parseLonLats`, `_parseBeelines`, `_parseNogos*`,
//! `_parseProfile`); and against BRouter's `RoutingParamCollector.java`.
//!
//! 1. **Share link** (what the browser address bar shows on bikerouter.de or any
//!    brouter-web instance). The state lives in the URL *fragment*:
//!    `https://bikerouter.de/#map=<zoom>/<lat>/<lon>/<layers>&lonlats=<lon>,<lat>;<lon>,<lat>[;…][&straight=i,j][&nogos=…][&polylines=…][&polygons=…][&profile=<name>][&alternativeidx=<0..3>]`
//!    * `lonlats`: `lon,lat` pairs (note the order: longitude first), separated by
//!      `;` (human readable form brouter-web writes) or `|` (BRouter API form).
//!    * `profile` and `alternativeidx` are **omitted** by brouter-web when they have
//!      their default values (`trekking`, `0`); we apply the same defaults.
//!    * `straight`: comma separated indices of beeline (straight line) legs.
//!    * `nogos`: `lon,lat,radius[,weight]` groups separated by `;`/`|`;
//!      `polylines` / `polygons`: `lon,lat,lon,lat,…[,weight]` groups.
//!    * values may be percent-encoded (`%7C` for `|`, `%2C` for `,`); brouter-web runs
//!      `decodeURIComponent` on every value, so do we. `&amp;` is accepted as
//!      separator like brouter-web does when the string contains it.
//!    * the `map=` part (zoom/lat/lon/layers) is irrelevant for routing and ignored.
//! 2. **Fragment without `map=`**: `https://bikerouter.de/#lonlats=…&profile=…`
//!    (brouter-web itself would not restore it, but it is unambiguous).
//! 3. **Legacy fragment**: `#zoom=13&lat=47.5&lon=19.0&layer=OpenStreetMap&lonlats=…`
//!    (rewritten by `onInvalidHashChangeCb` in brouter-web; we simply ignore the
//!    map keys).
//! 4. **Query string**: `https://bikerouter.de/?lonlats=…&profile=…` (the fragment
//!    wins when both are present).
//! 5. **Raw BRouter API URL**: `https://brouter.de/brouter?lonlats=lon,lat|lon,lat&profile=trekking&alternativeidx=0&format=gpx`.
//!    A URL whose path ends in `/brouter` is treated as the API endpoint and *its
//!    host is used* (e.g. a local `http://localhost:17777/brouter?…` server); for every
//!    other link the public `https://brouter.de/brouter` is called. `format` is
//!    always replaced by `gpx`.
//! 6. **Bare parameter string**: `lonlats=19.04,47.49;19.1,47.5&profile=fastbike`.
//!
//! Known profile renames from brouter-web (`hiking-beta` → `hiking-mountain`) are
//! applied; any other profile name is passed on as is (the server knows the current
//! list). `pois`, `ringgo`/`circlego` and other UI-only parameters are ignored; a
//! third field in a `lonlats` entry (BRouter's `d`/`m`/`r` waypoint flags) is dropped.
//!
//! Limits: 2..=[`MAX_WAYPOINTS`] waypoints, every coordinate must be inside the
//! WGS-84 range, parameter values may only contain digits and `.,|+-eE`.

use std::time::Duration;

use crate::{err, http, Result};
use ride_i18n::{t, tf};

/// Public BRouter endpoint used unless a raw API URL names another host.
pub const DEFAULT_API: &str = "https://brouter.de/brouter";
pub const DEFAULT_PROFILE: &str = "trekking";
pub const MAX_WAYPOINTS: usize = 200;
const MAX_PARAM_LEN: usize = 6000;

/// A parsed BRouter routing request.
#[derive(Debug, Clone, PartialEq)]
pub struct BrouterRequest {
    /// Endpoint without query string, e.g. `https://brouter.de/brouter`.
    pub api: String,
    /// Waypoints as `(lon, lat)` (BRouter order).
    pub lonlats: Vec<(f64, f64)>,
    pub profile: String,
    pub alternative_idx: u8,
    pub straight: Option<String>,
    pub nogos: Option<String>,
    pub polylines: Option<String>,
    pub polygons: Option<String>,
}

impl BrouterRequest {
    /// The GET URL (parameter order as brouter-web writes it, `format=gpx`).
    pub fn api_url(&self) -> String {
        let lonlats = self
            .lonlats
            .iter()
            .map(|&(lon, lat)| format!("{},{}", fmt6(lon), fmt6(lat)))
            .collect::<Vec<_>>()
            .join("|");
        let mut url = format!("{}?lonlats={lonlats}", self.api);
        for (key, val) in [
            ("straight", &self.straight),
            ("nogos", &self.nogos),
            ("polylines", &self.polylines),
            ("polygons", &self.polygons),
        ] {
            if let Some(v) = val {
                url.push_str(&format!("&{key}={v}"));
            }
        }
        url.push_str(&format!(
            "&profile={}&alternativeidx={}&format=gpx",
            self.profile, self.alternative_idx
        ));
        url
    }
}

/// `L.Util.formatNum(x, 6)`: at most six decimals, no trailing zeros.
fn fmt6(x: f64) -> String {
    let s = format!("{x:.6}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-0" {
        "0".to_string()
    } else {
        s.to_string()
    }
}

/// Lenient `decodeURIComponent`: invalid escapes are kept literally.
fn percent_decode(s: &str) -> String {
    if !s.contains('%') {
        return s.to_string();
    }
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let hex = |c: u8| (c as char).to_digit(16);
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// brouter-web `url2params`: `;`→`|`, `&` (or `&amp;`) separated `key=value`, value decoded.
fn parse_params(raw: &str) -> Vec<(String, String)> {
    // (brouter-web replaces ';' first, which breaks its own '&amp;' detection; we do it the
    // other way round so that HTML-escaped links work.)
    let s = raw.replace("&amp;", "&").replace(';', "|");
    s.split('&')
        .filter_map(|piece| {
            let (k, v) = piece.split_once('=')?;
            Some((k.trim().to_string(), percent_decode(v).replace(';', "|")))
        })
        .collect()
}

fn get<'a>(params: &'a [(String, String)], key: &str) -> Option<&'a str> {
    params
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
        .filter(|v| !v.is_empty())
}

/// `Some(endpoint)` when `before_query` is an http(s) URL whose path ends in `/brouter`.
fn api_endpoint(before_query: &str) -> Option<String> {
    let lower = before_query.to_ascii_lowercase();
    let (scheme, rest) = if lower.starts_with("https://") {
        ("https", &before_query[8..])
    } else if lower.starts_with("http://") {
        ("http", &before_query[7..])
    } else {
        return None;
    };
    let (host, path) = rest.split_once('/')?;
    let host_ok = !host.is_empty()
        && !host.contains('@')
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '[' | ']'));
    let path = path.trim_matches('/');
    let last = path.rsplit('/').next().unwrap_or("");
    (host_ok && last.eq_ignore_ascii_case("brouter")).then(|| format!("{scheme}://{host}/{path}"))
}

fn numeric_list(name: &str, v: &str) -> Result<String> {
    if v.len() > MAX_PARAM_LEN
        || !v
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '.' | ',' | '|' | '+' | '-' | 'e' | 'E'))
    {
        return err(tf(
            "route.bikerouter.a_linkben_ervenytelen_a_z_name_parameter",
            &[("name", name)],
        ));
    }
    Ok(v.to_string())
}

fn parse_lonlats(v: &str) -> Result<Vec<(f64, f64)>> {
    let mut out = Vec::new();
    for (i, group) in v.split('|').filter(|g| !g.trim().is_empty()).enumerate() {
        let mut nums = group.split(',');
        let (lon, lat) = (nums.next().map(str::trim), nums.next().map(str::trim));
        let parse = |s: Option<&str>| {
            s.and_then(|s| s.parse::<f64>().ok())
                .filter(|x| x.is_finite())
        };
        let (Some(lon), Some(lat)) = (parse(lon), parse(lat)) else {
            return err(tf(
                "route.bikerouter.a_linkben_a_z_utpont_koordinataja_nem_ertelmezheto",
                &[("n", &(i + 1).to_string())],
            ));
        };
        if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
            let hint = if (-90.0..=90.0).contains(&lon) && (-180.0..=180.0).contains(&lat) {
                t("route.bikerouter.felcserelt_hosszusag_szelesseg")
            } else {
                ""
            };
            return err(tf(
                "route.bikerouter.a_linkben_a_z_utpont_koordinataja_ervenytelen_hint",
                &[("n", &(i + 1).to_string()), ("hint", hint)],
            ));
        }
        out.push((lon, lat));
    }
    Ok(out)
}

/// Parse any supported link (see the module documentation) into a request.
pub fn parse_link(link: &str) -> Result<BrouterRequest> {
    let cleaned: String = link
        .trim()
        .trim_matches(|c| matches!(c, '"' | '\'' | '<' | '>' | '(' | ')'))
        .split_whitespace()
        .collect();
    if cleaned.is_empty() {
        return err(t("app.controller.illessz_be_egy_bikerouter_linket"));
    }
    let (before, fragment) = match cleaned.split_once('#') {
        Some((b, f)) => (b.to_string(), Some(f.to_string())),
        None => (cleaned.clone(), None),
    };
    let (path_part, query) = match before.split_once('?') {
        Some((p, q)) => (p.to_string(), Some(q.to_string())),
        None => (before.clone(), None),
    };

    let mut sources: Vec<Vec<(String, String)>> = Vec::new();
    if let Some(f) = &fragment {
        sources.push(parse_params(f));
    }
    if let Some(q) = &query {
        sources.push(parse_params(q));
    }
    if fragment.is_none() && query.is_none() && cleaned.contains('=') {
        sources.push(parse_params(&cleaned));
    }
    let Some(params) = sources.into_iter().find(|p| get(p, "lonlats").is_some()) else {
        return err(t(
            "route.bikerouter.a_linkben_nincs_utvonal_hianyzik_a_lonlats_resz_terv",
        ));
    };

    let lonlats = parse_lonlats(get(&params, "lonlats").unwrap_or(""))?;
    if lonlats.len() < 2 {
        return err(t(
            "route.bikerouter.a_linkben_legalabb_ket_utpont_start_es_cel_szukseges",
        ));
    }
    if lonlats.len() > MAX_WAYPOINTS {
        return err(tf(
            "route.bikerouter.a_linkben_tul_sok_utpont_van_legfeljebb_max_waypoint",
            &[("max", &MAX_WAYPOINTS.to_string())],
        ));
    }

    let profile = match get(&params, "profile") {
        None => DEFAULT_PROFILE.to_string(),
        Some(p) => {
            let p = if p == "hiking-beta" {
                "hiking-mountain"
            } else {
                p
            };
            if p.len() > 64
                || !p
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
            {
                return err(t("route.bikerouter.a_linkben_ervenytelen_a_profil_neve"));
            }
            p.to_string()
        }
    };
    let alternative_idx = get(&params, "alternativeidx")
        .and_then(|v| v.trim().parse::<u8>().ok())
        .filter(|&i| i <= 3)
        .unwrap_or(0);

    let straight = get(&params, "straight")
        .map(|v| {
            v.split(',')
                .filter_map(|t| t.trim().parse::<usize>().ok())
                .filter(|&i| i < lonlats.len())
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(",")
        })
        .filter(|s| !s.is_empty());
    let list = |key: &str| -> Result<Option<String>> {
        get(&params, key).map(|v| numeric_list(key, v)).transpose()
    };

    Ok(BrouterRequest {
        api: api_endpoint(&path_part).unwrap_or_else(|| DEFAULT_API.to_string()),
        lonlats,
        profile,
        alternative_idx,
        straight,
        nogos: list("nogos")?,
        polylines: list("polylines")?,
        polygons: list("polygons")?,
    })
}

fn looks_like_gpx(body: &str) -> bool {
    let head: String = body.chars().take(400).collect();
    head.contains("<gpx") || head.contains("<?xml")
}

/// Translate a BRouter error reply (plain text) into a Hungarian message.
fn server_error(status: u16, body: &str) -> String {
    let text = http::short_server_text(body);
    let lower = text.to_ascii_lowercase();
    match status {
        429 | 503 => {
            t("route.bikerouter.a_brouter_szerver_tulterhelt_probald_ujra_egy_perc_m").to_string()
        }
        404 => {
            t("route.bikerouter.a_brouter_vegpont_nem_talalhato_404_ellenorizd_a_lin").to_string()
        }
        _ if lower.contains("not mapped") => {
            t("route.bikerouter.az_egyik_utpont_a_brouter_adatain_kivul_esik_ott_nin").to_string()
        }
        _ if lower.contains("timeout") => {
            t("route.bikerouter.a_brouter_idotullepes_miatt_megszakitotta_a_szamolas").to_string()
        }
        _ if lower.contains("profile") => tf(
            "route.bikerouter.a_brouter_nem_ismeri_a_kert_profilt_text",
            &[("text", &text)],
        ),
        _ if text.is_empty() => tf(
            "route.bikerouter.a_brouter_hibat_jelzett_http_status",
            &[("status", &status.to_string())],
        ),
        _ => tf(
            "route.bikerouter.a_brouter_nem_tudott_utvonalat_szamolni_text_http_st",
            &[("text", &text), ("status", &status.to_string())],
        ),
    }
}

/// Call the BRouter server and return the GPX text.
pub async fn fetch_gpx(req: &BrouterRequest) -> Result<String> {
    let client = http::client(Duration::from_secs(90))?;
    let fetched = http::get_text(&client, &req.api_url(), "BRouter").await?;
    if fetched.status != 200 {
        return err(server_error(fetched.status, &fetched.body));
    }
    if !looks_like_gpx(&fetched.body) {
        return err(server_error(200, &fetched.body));
    }
    Ok(fetched.body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::testserver::{Canned, Server};

    fn ok(link: &str) -> BrouterRequest {
        parse_link(link).unwrap_or_else(|e| panic!("{link}: {e}"))
    }

    #[test]
    fn standard_share_link_with_defaults_omitted() {
        let r = ok("https://bikerouter.de/#map=13/47.4979/19.0402/standard&lonlats=19.040199,47.497899;19.0467,47.5008");
        assert_eq!(r.api, DEFAULT_API);
        assert_eq!(r.lonlats, vec![(19.040199, 47.497899), (19.0467, 47.5008)]);
        assert_eq!(r.profile, "trekking");
        assert_eq!(r.alternative_idx, 0);
        assert_eq!(
            r.api_url(),
            "https://brouter.de/brouter?lonlats=19.040199,47.497899|19.0467,47.5008&profile=trekking&alternativeidx=0&format=gpx"
        );
    }

    #[test]
    fn share_link_with_profile_alternative_layers_and_via_points() {
        let r = ok("https://www.bikerouter.de/#map=12/47.5390/18.9805/OpenStreetMap,Waymarked_Trails-Cycling&lonlats=18.9805,47.5390;18.95,47.62;18.8939,47.7188&profile=fastbike&alternativeidx=2");
        assert_eq!(r.lonlats.len(), 3);
        assert_eq!(r.profile, "fastbike");
        assert_eq!(r.alternative_idx, 2);
        assert!(r
            .api_url()
            .contains("&profile=fastbike&alternativeidx=2&format=gpx"));
    }

    #[test]
    fn nogos_polylines_polygons_and_straight_are_kept() {
        let r = ok("https://bikerouter.de/#map=14/47.5/19.0/standard&lonlats=19.0,47.5;19.02,47.52;19.05,47.55&straight=1&nogos=19.01,47.51,300;19.03,47.53,150,0.5&polylines=19.0,47.5,19.01,47.51,19.02,47.5&polygons=19.0,47.5,19.01,47.5,19.01,47.51,19.0,47.51&profile=gravel");
        assert_eq!(r.straight.as_deref(), Some("1"));
        assert_eq!(
            r.nogos.as_deref(),
            Some("19.01,47.51,300|19.03,47.53,150,0.5")
        );
        assert_eq!(
            r.polylines.as_deref(),
            Some("19.0,47.5,19.01,47.51,19.02,47.5")
        );
        assert_eq!(
            r.polygons.as_deref(),
            Some("19.0,47.5,19.01,47.5,19.01,47.51,19.0,47.51")
        );
        let url = r.api_url();
        assert!(
            url.contains("&straight=1&nogos=19.01,47.51,300|19.03,47.53,150,0.5&polylines="),
            "{url}"
        );
        assert!(url.ends_with("&profile=gravel&alternativeidx=0&format=gpx"));
    }

    #[test]
    fn percent_encoded_and_amp_escaped_values() {
        let r = ok("https://bikerouter.de/#map=13/47.5/19.0/standard&lonlats=19.04%2C47.49%7C19.1%2C47.5&profile=trekking");
        assert_eq!(r.lonlats, vec![(19.04, 47.49), (19.1, 47.5)]);
        let r = ok("https://bikerouter.de/#map=13/47.5/19.0/standard&amp;lonlats=19.04,47.49;19.1,47.5&amp;profile=safety");
        assert_eq!(r.lonlats.len(), 2);
        assert_eq!(r.profile, "safety");
    }

    #[test]
    fn other_accepted_shapes() {
        // fragment without map=
        assert_eq!(
            ok("https://bikerouter.de/#lonlats=19.0,47.5;19.1,47.6&profile=shortest").profile,
            "shortest"
        );
        // legacy hash
        let r = ok("https://bikerouter.de/#zoom=13&lat=47.5&lon=19.0&layer=OpenStreetMap&lonlats=19.0,47.5;19.1,47.6");
        assert_eq!(r.lonlats, vec![(19.0, 47.5), (19.1, 47.6)]);
        // query string
        assert_eq!(
            ok("https://bikerouter.de/?lonlats=19.0,47.5|19.1,47.6&profile=moped").profile,
            "moped"
        );
        // bare parameter string
        assert_eq!(
            ok("lonlats=19.0,47.5;19.1,47.6&profile=fastbike").profile,
            "fastbike"
        );
        // brouter-web served from brouter.de, surrounded by whitespace and quotes
        let r = ok("  \"https://brouter.de/brouter-web/#map=10/47.5/19.0/standard&lonlats=19.0,47.5;19.1,47.6\"\n");
        assert_eq!(r.api, DEFAULT_API);
        // renamed profile
        assert_eq!(
            ok("#lonlats=19.0,47.5;19.1,47.6&profile=hiking-beta").profile,
            "hiking-mountain"
        );
        // fragment wins over query
        assert_eq!(
            ok("https://x.org/?lonlats=1,1;2,2#lonlats=19.0,47.5;19.1,47.6").lonlats[0],
            (19.0, 47.5)
        );
    }

    #[test]
    fn raw_api_urls_keep_their_host() {
        let r = ok("https://brouter.de/brouter?lonlats=18.9805,47.5390|18.8939,47.7188&profile=trekking&alternativeidx=0&format=gpx");
        assert_eq!(r.api, "https://brouter.de/brouter");
        assert_eq!(r.lonlats.len(), 2);
        let r = ok("http://localhost:17777/brouter?lonlats=19.0,47.5|19.1,47.6&format=kml");
        assert_eq!(r.api, "http://localhost:17777/brouter");
        assert!(r.api_url().ends_with("&format=gpx"));
        // brouter-web is not the API
        assert_eq!(
            ok("https://evil.example/brouter-web/#lonlats=19.0,47.5;19.1,47.6").api,
            DEFAULT_API
        );
        assert_eq!(
            ok("ftp://evil.example/brouter?lonlats=19.0,47.5|19.1,47.6").api,
            DEFAULT_API
        );
        assert_eq!(
            ok("https://user:pw@evil.example/brouter?lonlats=19.0,47.5|19.1,47.6").api,
            DEFAULT_API
        );
    }

    #[test]
    fn straight_indices_are_range_checked_and_third_fields_dropped() {
        let r = ok("#lonlats=19.0,47.5,d;19.1,47.6;19.2,47.7&straight=0,5,x,2");
        assert_eq!(r.straight.as_deref(), Some("0,2"));
        assert_eq!(r.lonlats.len(), 3);
        assert!(!r.api_url().contains(",d"));
    }

    #[test]
    fn invalid_links_give_hungarian_errors() {
        for (link, needle) in [
            ("", "Illessz be"),
            ("   ", "Illessz be"),
            ("https://bikerouter.de/", "nincs útvonal"),
            (
                "https://bikerouter.de/#map=13/47.5/19.0/standard",
                "nincs útvonal",
            ),
            ("https://bikerouter.de/#lonlats=19.0,47.5", "legalább két"),
            (
                "https://bikerouter.de/#lonlats=19.0,47.5;abc,def",
                "2. útpont",
            ),
            ("https://bikerouter.de/#lonlats=19.0,47.5;19.1", "2. útpont"),
            (
                "https://bikerouter.de/#lonlats=19.0,47.5;19.1,95.0",
                "érvénytelen",
            ),
            (
                "https://bikerouter.de/#lonlats=34.05,-118.24;34.1,-118.2",
                "felcserélt",
            ),
            (
                "https://bikerouter.de/#lonlats=19.0,47.5;19.1,47.6&profile=../etc/passwd",
                "profil",
            ),
            (
                "https://bikerouter.de/#lonlats=19.0,47.5;19.1,47.6&nogos=19,47,1<script>",
                "nogos",
            ),
        ] {
            let e = parse_link(link).expect_err(link).to_string();
            assert!(e.contains(needle), "{link:?} -> {e:?} (wanted {needle:?})");
        }
        let many = (0..=MAX_WAYPOINTS)
            .map(|i| format!("19.{i:03},47.5"))
            .collect::<Vec<_>>()
            .join(";");
        assert!(parse_link(&format!("#lonlats={many}"))
            .unwrap_err()
            .to_string()
            .contains("túl sok"));
    }

    #[test]
    fn number_formatting_and_decoding_helpers() {
        assert_eq!(fmt6(19.040199), "19.040199");
        assert_eq!(fmt6(19.0), "19");
        assert_eq!(fmt6(47.5000004), "47.5");
        assert_eq!(fmt6(-0.0000001), "0");
        assert_eq!(fmt6(-3.25), "-3.25");
        assert_eq!(percent_decode("a%2Cb%7Cc"), "a,b|c");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz%4"), "%zz%4");
        assert_eq!(percent_decode("%C3%A9"), "é");
    }

    #[test]
    fn server_errors_are_translated() {
        assert!(server_error(503, "").contains("túlterhelt"));
        assert!(
            server_error(400, "from-position not mapped in existing datafile\n")
                .contains("adatain kívül")
        );
        assert!(server_error(500, "operation killed by timeout (120s)").contains("időtúllépés"));
        assert!(server_error(400, "profile xyz does not exist").contains("profilt"));
        assert!(server_error(400, "no track found at pass=3").contains("no track found"));
        assert!(server_error(418, "").contains("418"));
    }

    #[tokio::test]
    async fn fetch_gpx_roundtrip_against_a_local_server() {
        let gpx = r#"<?xml version="1.0"?><gpx><trk><trkseg><trkpt lat="47.5" lon="19.0"><ele>1</ele></trkpt></trkseg></trk></gpx>"#;
        let server = Server::start(vec![
            Canned {
                status: 200,
                content_type: "application/gpx+xml",
                body: gpx.into(),
            },
            Canned {
                status: 400,
                content_type: "text/plain",
                body: "from-position not mapped in existing datafile\n".into(),
            },
            Canned {
                status: 200,
                content_type: "text/html",
                body: "<html><body>Maintenance</body></html>".into(),
            },
        ]);
        let req = parse_link(&format!(
            "{}/brouter?lonlats=19.0,47.5|19.1,47.6&profile=safety",
            server.base
        ))
        .unwrap();
        assert_eq!(fetch_gpx(&req).await.unwrap(), gpx);
        let line = server.request_lines().remove(0);
        assert!(line.starts_with("GET /brouter?lonlats=19,47.5|19.1,47.6&profile=safety&alternativeidx=0&format=gpx HTTP/1.1"), "{line}");

        let e = fetch_gpx(&req).await.unwrap_err().to_string();
        assert!(e.contains("adatain kívül"), "{e}");
        let e = fetch_gpx(&req).await.unwrap_err().to_string();
        assert!(e.contains("BRouter"), "{e}");
    }

    #[tokio::test]
    async fn unreachable_server_gives_a_readable_error() {
        // Port 9 (discard) on localhost is closed on practically every machine.
        let req = parse_link("http://127.0.0.1:9/brouter?lonlats=19.0,47.5|19.1,47.6").unwrap();
        let e = fetch_gpx(&req).await.unwrap_err().to_string();
        assert!(e.starts_with("BRouter:"), "{e}");
    }
}
