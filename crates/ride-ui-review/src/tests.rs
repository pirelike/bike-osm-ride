//! CSS house rules and server-side render smoke tests of the review page.

use super::*;
use ride_model::{RecordPoint, RideSummary};
use std::collections::HashSet;

// ── css ────────────────────────────────────────────────────────────────────

/// Custom properties the components define inline (not part of the token set).
const LOCAL_VARS: [&str; 1] = ["--c"];

fn defined_tokens(css: &str) -> HashSet<String> {
    let bytes = css.as_bytes();
    let mut out = HashSet::new();
    let mut i = 0usize;
    while i + 2 < bytes.len() {
        if bytes[i] == b'-' && bytes[i + 1] == b'-' {
            let start = i + 2;
            let mut j = start;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'-') {
                j += 1;
            }
            if j > start && j < bytes.len() && bytes[j] == b':' {
                out.insert(css[i..j].to_string());
                i = j;
                continue;
            }
            i = j.max(i + 1);
        } else {
            i += 1;
        }
    }
    out
}

#[test]
fn review_css_only_uses_tokens_the_kit_defines() {
    let kit = include_str!("../../ride-ui-kit/assets/kit.css");
    let defined = defined_tokens(kit);
    assert!(defined.contains("--series-1") && defined.contains("--font-ui"));
    let mut rest = CSS;
    let mut seen = 0usize;
    while let Some(at) = rest.find("var(--") {
        let after = &rest[at + 6..];
        let end = after.find([')', ',']).expect("var() is closed");
        let name = after[..end].trim();
        assert!(
            defined.contains(&format!("--{name}"))
                || LOCAL_VARS.contains(&format!("--{name}").as_str()),
            "--{name} is not defined by the design kit"
        );
        seen += 1;
        rest = &after[end..];
    }
    assert!(seen > 40, "found only {seen} token uses");
}

#[test]
fn review_css_follows_the_house_rules() {
    assert!(
        !CSS.contains('#'),
        "hex colours are not allowed, use tokens"
    );
    assert!(!CSS.contains("rgb"), "rgb() colours are not allowed");
    for banned in [
        "blur(",
        "backdrop-filter",
        "text-shadow",
        "drop-shadow",
        "gradient",
        "text-transform",
        "box-shadow",
        "opacity",
    ] {
        assert!(!CSS.contains(banned), "{banned} is against docs/DESIGN.md");
    }
    // radius: square, or one of the tokens (4 px at most)
    for part in CSS.split("border-radius:").skip(1) {
        let value = part.split(';').next().unwrap_or("").trim();
        assert!(
            value == "0" || value.starts_with("var(--radius-"),
            "border-radius: {value}"
        );
    }
    // no literal pixel sizes inside a `font:` shorthand
    for part in CSS.split("font:").skip(1) {
        let value = part.split(';').next().unwrap_or("");
        assert!(!value.contains("px"), "font: {value} uses a literal size");
    }
}

#[test]
fn every_class_of_this_crate_is_prefixed() {
    let mut rest = CSS;
    let mut seen = 0usize;
    while let Some(at) = rest.find('.') {
        let after = &rest[at + 1..];
        let end = after
            .find(|c: char| !(c.is_alphanumeric() || c == '-' || c == '_'))
            .unwrap_or(after.len());
        let class = &after[..end];
        if class
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic())
        {
            assert!(
                class.starts_with("rv-") || class.starts_with("is-"),
                ".{class} must be prefixed `rv-`"
            );
            seen += 1;
        }
        rest = &after[end..];
    }
    assert!(seen > 40, "found only {seen} class selectors");
}

// ── server-side render smoke tests ─────────────────────────────────────────

fn route() -> Arc<Route> {
    let mut route = ride_model::testing::straight_route(
        &[(1_500.0, 0.0), (1_500.0, 5.0), (1_000.0, -2.0)],
        120.0,
    );
    route.id = "route-1".into();
    route.name = "Teszt kör".into();
    route.climbs = vec![ride_model::Climb {
        start_dist: 1_500.0,
        end_dist: 3_000.0,
        length: 1_500.0,
        gain: 75.0,
        avg_grade: 5.0,
        max_grade: 6.0,
        start_ele: 120.0,
        end_ele: 195.0,
        category: None,
    }];
    Arc::new(route)
}

fn record(started: f64, speed: f64, estimated: bool) -> Arc<RideRecord> {
    let route = route();
    let points: Vec<RecordPoint> = (0..)
        .map(|i| i as f64)
        .take_while(|t| t * speed <= 4_000.0)
        .map(|t| {
            let at = route.locate(t * speed);
            RecordPoint {
                t,
                unix_s: started + t,
                dist: t * speed,
                lat: at.lat,
                lon: at.lon,
                ele: at.ele,
                speed_ms: speed,
                power_w: Some(180.0 + (t % 20.0)),
                cadence_rpm: Some(86.0),
                hr_bpm: Some(140.0 + t / 30.0),
                grade: at.grade,
            }
        })
        .collect();
    Arc::new(RideRecord {
        version: 1,
        route_id: "route-1".into(),
        route_name: "Teszt kör".into(),
        simulated: false,
        summary: RideSummary {
            route_name: "Teszt kör".into(),
            route_id: "route-1".into(),
            started_unix_s: started,
            elapsed_s: 4_000.0 / speed,
            dist: 4_000.0,
            ascent: 75.0,
            avg_speed_ms: speed,
            max_speed_ms: speed,
            avg_power_w: Some(190.0),
            max_power_w: None,
            normalized_power_w: None,
            intensity_factor: None,
            tss: None,
            work_kj: 0.0,
            kcal: 0.0,
            avg_hr: Some(150.0),
            max_hr: None,
            avg_cadence: Some(86.0),
            power_zone_secs: [0.0; 7],
            hr_zone_secs: [0.0; 5],
        },
        points,
        source: if estimated {
            RideSource::Imported
        } else {
            RideSource::Recorded
        },
        power_estimated: estimated,
        estimate: estimated.then(|| profile_params(&RiderSettings::default())),
    })
}

fn meta_of(record: &RideRecord) -> RideMeta {
    RideMeta {
        stem: stem_of(record),
        started_unix_s: record.summary.started_unix_s,
        elapsed_s: record.summary.elapsed_s,
        end_dist: 4_000.0,
        complete: true,
        source: record.source,
        power_estimated: record.power_estimated,
        avg_power_w: record.summary.avg_power_w,
        avg_hr: record.summary.avg_hr,
        route_name: record.route_name.clone(),
        ..RideMeta::default()
    }
}

#[derive(Clone, PartialEq, Props)]
struct HarnessProps {
    records: Vec<Arc<RideRecord>>,
    listed: Vec<Arc<RideRecord>>,
    compact: bool,
    with_route: bool,
    notes: Vec<String>,
}

#[component]
fn Harness(props: HarnessProps) -> Element {
    let listed = props.listed.clone();
    let routes = use_signal(move || {
        if listed.is_empty() {
            Vec::new()
        } else {
            vec![RouteRides {
                route_id: "route-1".into(),
                route_name: "Teszt kör".into(),
                rides: listed.iter().map(|r| meta_of(r)).collect(),
            }]
        }
    });
    let route_id = use_signal(|| Some("route-1".to_string()));
    let with_route = props.with_route;
    let route_sig = use_signal(move || with_route.then(route));
    let stems: Vec<String> = props.records.iter().map(|r| stem_of(r)).collect();
    let selected = use_signal(move || stems.clone());
    let records = props.records.clone();
    let loaded = use_signal(move || Arc::new(records.clone()));
    let busy = use_signal(|| false);
    let notes_v = props.notes.clone();
    let notes = use_signal(move || notes_v.clone());
    let rider = use_signal(RiderSettings::default);
    let theme = use_signal(|| Theme::Dark);
    let map_style = use_signal(|| MapStyle::Dark);
    let analysis = use_signal(|| Arc::new(Analysis::default()));
    let ftp_w = use_signal(|| 200.0_f64);
    rsx! {
        ReviewPage {
            routes,
            route_id,
            route: route_sig,
            selected,
            loaded,
            busy,
            notes,
            rider,
            theme,
            map_style,
            analysis,
            ftp_w,
            compact: props.compact,
            on_close: move |_| {},
            on_route: move |_| {},
            on_toggle: move |_| {},
            on_import: move |_| {},
            on_delete: move |_| {},
            on_reestimate: move |_| {},
            on_map_style: move |_| {},
            on_dismiss_notes: move |_| {},
            on_apply_ftp: move |_| {},
        }
    }
}

fn render(props: HarnessProps) -> String {
    let mut vdom = VirtualDom::new_with_props(Harness, props);
    vdom.rebuild_in_place();
    dioxus_ssr::render(&vdom)
}

fn two_rides() -> HarnessProps {
    let a = record(1_790_000_000.0, 8.0, false);
    let b = record(1_790_100_000.0, 9.0, false);
    HarnessProps {
        records: vec![a.clone(), b.clone()],
        listed: vec![b, a],
        compact: false,
        with_route: true,
        notes: Vec::new(),
    }
}

#[test]
fn renders_two_rides_with_the_chart_the_overview_and_the_table() {
    let html = render(two_rides());
    assert!(html.contains("Edzések áttekintése"));
    assert!(html.contains("rv-ride"), "the list");
    assert!(html.contains("Teszt kör · 2 edzés"), "the route selector");
    assert!(html.contains("rv-overview"), "the overview profile");
    assert!(html.contains("rv-plot"), "the chart");
    assert!(html.contains("var(--series-1)") && html.contains("var(--series-2)"));
    assert!(
        html.contains("stroke-dasharray"),
        "the second ride is dashed"
    );
    assert!(html.contains("rv-table"), "the section table");
    assert!(html.contains("Átl. watt"));
    assert!(html.contains("1. emelkedő"), "the climb quick pick");
    assert!(html.contains("rv-hit"), "the selection layer");
    assert!(html.contains("2026-"), "ride dates");
    assert!(!html.contains("NaN"), "no NaN anywhere");
    assert!(
        !html.contains("rv-estimate"),
        "no estimate panel for measured watts"
    );
}

#[test]
fn marks_estimated_watts_and_offers_the_setup() {
    let a = record(1_790_000_000.0, 8.0, true);
    let html = render(HarnessProps {
        records: vec![a.clone()],
        listed: vec![a],
        compact: false,
        with_route: true,
        notes: vec!["A teljesítményt becsültük.".into()],
    });
    assert!(html.contains("Becsült watt"), "the tag");
    assert!(html.contains("Importált"));
    assert!(html.contains('~'), "estimated numbers carry a tilde");
    assert!(
        html.contains("A teljesítményt becsültük."),
        "the import notes"
    );
    assert!(html.contains("A becslés beállításai"), "the setup button");
}

#[test]
fn an_empty_log_explains_itself() {
    let html = render(HarnessProps {
        records: Vec::new(),
        listed: Vec::new(),
        compact: false,
        with_route: false,
        notes: Vec::new(),
    });
    assert!(html.contains("Még nincs mentett edzés"));
    assert!(html.contains("Edzés importálása"));
    assert!(!html.contains("rv-table"));
}

#[test]
fn a_route_whose_file_is_gone_still_lists_its_rides() {
    let mut props = two_rides();
    props.with_route = false;
    let html = render(props);
    assert!(html.contains("rv-ride"));
    assert!(html.contains("már nem érhető el"), "the missing route note");
    assert!(
        html.contains("rv-plot"),
        "the chart does not need the route file"
    );
}

#[test]
fn nothing_ticked_asks_for_a_ride() {
    let mut props = two_rides();
    props.records = Vec::new();
    let html = render(props);
    assert!(
        html.contains("Jelölj ki"),
        "a hint instead of an empty chart"
    );
    assert!(!html.contains("NaN"));
}

#[test]
fn the_phone_layout_has_tabs() {
    let mut props = two_rides();
    props.compact = true;
    let html = render(props);
    assert!(html.contains("rv-page--compact"));
    assert!(html.contains("rv-tab-rides"));
    for tab in ["Edzések", "Térkép", "Grafikon", "Szakasz"] {
        assert!(html.contains(tab), "tab {tab}");
    }
}

#[test]
fn the_phone_table_has_a_row_per_number_and_a_column_per_ride() {
    let mut props = two_rides();
    props.compact = true;
    let html = render(props);
    assert!(html.contains("rv-table--by-metric"), "the turned table");
    // one header cell per ride, lettered, and every number as a row head
    assert!(html.contains(">A</span>") && html.contains(">B</span>"));
    for head in table_head() {
        assert!(
            html.contains(&format!("<th scope=\"row\">{head}</th>")),
            "row {head}"
        );
    }
    assert!(!html.contains("NaN"));
}

#[test]
fn the_phone_header_is_one_row_with_icon_buttons() {
    let mut props = two_rides();
    props.compact = true;
    let html = render(props);
    assert!(
        html.contains("rv-head-views"),
        "the views live in the header"
    );
    assert!(!html.contains("rv-views"), "no second row for them");
    assert!(
        !html.contains("rv-title"),
        "the top bar already names the page"
    );
    assert!(
        html.contains("aria-label=\"Edzés importálása\""),
        "the icon button keeps its name"
    );
    assert!(!html.contains("rv-panel-bar"), "the tabs name the sections");
}

#[test]
fn the_desktop_sections_are_titled_panels() {
    let html = render(two_rides());
    assert!(html.contains("rv-panel-bar"));
    for title in [
        "Edzések",
        "Térkép",
        "Szakasz",
        "Grafikon",
        "A szakasz adatai",
    ] {
        assert!(
            html.contains(&format!("<h2 class=\"rv-panel-title\">{title}</h2>")),
            "panel {title}"
        );
    }
    assert!(
        html.contains(" km</span>"),
        "the distance axes carry the unit"
    );
}

#[test]
fn without_a_cursor_the_legend_shows_the_window_mean() {
    let html = render(two_rides());
    assert!(html.contains("Ø "), "a mean instead of a dash");
}

#[test]
fn the_window_value_is_the_mean_and_the_final_time_difference() {
    let a =
        DistSeries::from_points(&record(1_790_000_000.0, 8.0, false).points, GRID_M).map(Arc::new);
    let b =
        DistSeries::from_points(&record(1_790_100_000.0, 10.0, false).points, GRID_M).map(Arc::new);
    let rides: Vec<Arc<DistSeries>> = [a, b].into_iter().flatten().collect();
    assert_eq!(rides.len(), 2);
    let cadence = chart::window_value(&rides, 0, chart::Metric::Cadence, 500.0, 3_500.0);
    assert!(
        cadence.is_some_and(|v| (v - 86.0).abs() < 0.5),
        "{cadence:?}"
    );
    let speed = chart::window_value(&rides, 1, chart::Metric::Speed, 500.0, 3_500.0);
    assert!(speed.is_some_and(|v| (v - 36.0).abs() < 0.5), "{speed:?}");
    // the faster ride is ahead at the end of the window: a negative difference
    let delta = chart::window_value(&rides, 1, chart::Metric::Delta, 500.0, 3_500.0);
    assert!(delta.is_some_and(|d| d < -50.0), "{delta:?}");
    assert_eq!(
        chart::window_value(&rides, 0, chart::Metric::Delta, 500.0, 3_500.0),
        Some(0.0)
    );
    assert_eq!(
        chart::window_value(&rides, 5, chart::Metric::Power, 0.0, 1.0),
        None
    );
    assert_eq!(
        chart::window_value(&rides, 0, chart::Metric::Power, 3_000.0, 1_000.0),
        None
    );
}

#[test]
fn a_finger_can_still_scroll_over_the_plots() {
    assert!(
        CSS.contains("touch-action: pan-y"),
        "vertical swipes must scroll the page"
    );
    assert!(!CSS.contains("touch-action: none"));
}

// ── the power view ─────────────────────────────────────────────────────────

use ride_sim::power_curve::{analyze, Analysis, DURATIONS};

/// Renders `PowerView` alone with `analysis`, an FTP of `ftp` W and the given ticked rides.
fn render_power(analysis: Analysis, ftp: f64, records: Vec<Arc<RideRecord>>) -> String {
    #[derive(Clone, PartialEq, Props)]
    struct PowerProps {
        analysis: Analysis,
        ftp: f64,
        records: Vec<Arc<RideRecord>>,
    }
    #[component]
    fn PowerHarness(props: PowerProps) -> Element {
        let analysis = props.analysis.clone();
        let analysis = use_signal(move || Arc::new(analysis.clone()));
        let ftp = props.ftp;
        let ftp_w = use_signal(move || ftp);
        let records = props.records.clone();
        let loaded = use_signal(move || Arc::new(records.clone()));
        rsx! {
            power::PowerView { analysis, ftp_w, loaded, on_apply_ftp: move |_| {} }
        }
    }
    let mut vdom = VirtualDom::new_with_props(
        PowerHarness,
        PowerProps {
            analysis,
            ftp,
            records,
        },
    );
    vdom.rebuild_in_place();
    dioxus_ssr::render(&vdom)
}

fn curve_meta(stem: &str, started: f64, values: [f32; 5]) -> RideMeta {
    RideMeta {
        stem: stem.into(),
        started_unix_s: started,
        elapsed_s: 4_000.0,
        tss: Some(80.0),
        mmp: Some(values.iter().map(|v| Some(*v)).collect()),
        ..RideMeta::default()
    }
}

fn analysis_of(rides: Vec<RideMeta>, now: f64) -> Analysis {
    analyze(
        &[RouteRides {
            route_id: "route-1".into(),
            route_name: "Teszt kör".into(),
            rides,
        }],
        now,
    )
}

#[test]
fn the_power_view_shows_the_curve_the_ftp_and_the_trend() {
    let now = 1_790_200_000.0;
    let analysis = analysis_of(
        vec![
            curve_meta(
                "1790000000",
                1_790_000_000.0,
                [700.0, 400.0, 300.0, 250.0, 220.0],
            ),
            curve_meta(
                "1790100000",
                1_790_100_000.0,
                [650.0, 410.0, 290.0, 260.0, 0.0],
            ),
        ],
        now,
    );
    let html = render_power(analysis, 200.0, Vec::new());
    for text in [
        "Csúcsteljesítmény",
        "5 mp",
        "1 perc",
        "20 perc",
        "60 perc",
        "700", // the best 5 s
        "260", // the best 20 minutes
        "Teszt kör",
        "FTP",
        "247", // 95 % of 260
        "FTP beállítása 247 W-ra",
        "jelenleg beállítva: 200 W",
        "Csak mért watt számít",
        "Terhelés",
        "Heti",
        "Havi",
    ] {
        assert!(html.contains(text), "missing {text:?}");
    }
    assert_eq!(html.matches("rv-trend-col").count(), 12, "twelve weeks");
    assert!(html.contains("rv-bar"), "the bars of the curve");
    assert!(!html.contains("NaN"), "no NaN anywhere");
    assert_eq!(DURATIONS.len(), 5);
}

#[test]
fn an_ftp_that_is_already_set_cannot_be_applied_again() {
    let analysis = analysis_of(
        vec![curve_meta(
            "1",
            1_790_000_000.0,
            [700.0, 400.0, 300.0, 200.0, 190.0],
        )],
        1_790_100_000.0,
    );
    // 95 % of 200 W is 190 W: the setting already says so
    let html = render_power(analysis, 190.0, Vec::new());
    assert!(html.contains("Az FTP már ennyi"));
    assert!(html.contains("disabled"));
    assert!(!html.contains("FTP beállítása"));
}

#[test]
fn without_a_measured_twenty_minutes_the_page_explains_and_offers_nothing() {
    let html = render_power(Analysis::default(), 200.0, Vec::new());
    assert!(html.contains("Még nincs mért teljesítményű edzés"));
    assert!(html.contains("Még nincs 20 perces mért teljesítmény"));
    assert!(html.contains("FTP-teszt 20 perc"));
    assert!(!html.contains("FTP beállítása"));
    assert!(!html.contains("NaN"));
    // an all-empty log still draws an empty trend, not a broken one
    assert!(
        !html.contains("rv-trend-col"),
        "no periods without an analysis"
    );
}

#[test]
fn rides_still_waiting_for_their_numbers_are_announced() {
    let mut pending = curve_meta("1", 1_790_000_000.0, [0.0; 5]);
    pending.mmp = None;
    let analysis = analysis_of(vec![pending], 1_790_100_000.0);
    assert_eq!(analysis.rides_pending, 1);
    let html = render_power(analysis, 200.0, Vec::new());
    assert!(html.contains("1 régebbi edzés számai még nincsenek meg"));
}

#[test]
fn the_ticked_rides_get_a_column_each_and_estimated_watts_a_tilde() {
    let measured = record(1_790_000_000.0, 8.0, false);
    let estimated = record(1_790_100_000.0, 9.0, true);
    let html = render_power(
        analysis_of(Vec::new(), 1_790_200_000.0),
        200.0,
        vec![measured, estimated],
    );
    assert!(html.contains("var(--series-1)") && html.contains("var(--series-2)"));
    // the rides are ~8 minutes long: a value for 5 s, 1 and 5 minutes, none for 20 and 60
    assert!(html.contains('~'), "the estimated ride's watts are marked");
    assert!(html.contains("becsült vagy szimulált wattból"));
    assert!(!html.contains("NaN"));
}

#[test]
fn the_page_has_a_switch_between_the_comparison_and_the_power_view() {
    let html = render(two_rides());
    assert!(html.contains("rv-views"));
    assert!(html.contains("Összehasonlítás"));
    assert!(html.contains("Teljesítmény"));
    // it opens on the comparison
    assert!(html.contains("rv-table") && !html.contains("rv-power"));
}

#[test]
fn the_labels_of_the_trend_are_dates() {
    use ride_sim::power_curve::Period;
    // 2026-11-02 00:00 UTC is a Monday
    assert_eq!(power::period_label(1_793_577_600.0, Period::Week), "11.02.");
    assert_eq!(
        power::period_label(1_793_577_600.0, Period::Month),
        "2026-11"
    );
    assert_eq!(power::period_label(f64::NAN, Period::Week), DASH);
    assert_eq!(power::duration_label(5), "5 mp");
    assert_eq!(power::duration_label(1_200), "20 perc");
}
