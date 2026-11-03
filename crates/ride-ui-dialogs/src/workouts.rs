//! Workout picker: category and tag filters, the embedded library and the selected workout's
//! steps, its chart and the import of a MyWhoosh / Zwift file.
//!
//! Selecting a card applies it immediately (the ride screen and the trainer follow at once);
//! the dialog stays open so the rider can browse. "Edzés nélkül" clears the selection. Tags
//! combine with AND; every chip shows how many workouts would remain, and a chip that would
//! empty the list is disabled.

use dioxus::prelude::*;
use ride_model::{StepTarget, TagGroup, Workout, WorkoutCategory, WorkoutLibrary, WorkoutTag};
use ride_ui_kit::palette::{power_zone_name, power_zone_var};
use ride_ui_kit::{Dialog, Icon, IconName};

use ride_ui_kit::workout_chart;

/// Tags shown on a list card (the detail column shows all of them).
const CARD_TAGS: usize = 3;

/// Structured workout picker. `selected` is the applied workout id; `on_select` emits the
/// new id (or `None` for a free ride). `imported` holds the workouts imported from files and
/// `on_import` asks the app to open the file dialog. `ftp_w` scales watt targets in the chart.
#[component]
pub fn WorkoutsDialog(
    open: bool,
    selected: Option<String>,
    #[props(default = 200.0)] ftp_w: f64,
    #[props(default)] imported: ReadSignal<Vec<Workout>>,
    #[props(default)] on_import: EventHandler<()>,
    on_select: EventHandler<Option<String>>,
    on_close: EventHandler<()>,
) -> Element {
    if !open {
        return rsx! {};
    }
    let imported_list = imported.read().clone();
    let imported_count = imported_list.len();
    let library = use_memo(move || WorkoutLibrary::with_imported(imported_list.clone()));
    let lib = library.read();
    let mut category = use_signal(|| None::<WorkoutCategory>);
    let mut picked = use_signal(Vec::<WorkoutTag>::new);
    // Phone: the three columns do not fit, so the list and the details are two pages of the
    // dialog (tabs on top) and the tag filters sit behind a button. On wider screens the CSS
    // ignores all three.
    let mut details_page = use_signal(|| false);
    let mut tags_open = use_signal(|| false);
    let filter = *category.read();
    let tags = picked.read().clone();
    let workouts: Vec<&Workout> = lib.filter(filter, &tags);
    let current = selected.as_deref().and_then(|id| lib.get(id));
    let chart = current.map(|w| workout_chart::chart(w, ftp_w));
    // category counts follow the tag selection, so the rider sees where the matches are
    let categories: Vec<(WorkoutCategory, usize)> = lib
        .categories()
        .into_iter()
        .map(|(c, _)| (c, lib.filter(Some(c), &tags).len()))
        .collect();
    let total = lib.filter(None, &tags).len();
    let tag_counts = lib.tag_counts(filter, &tags);
    let shown = workouts.len();

    rsx! {
        Dialog {
            open: true,
            title: "Edzés".to_string(),
            on_close,
            wide: true,
            flush: true,
            foot: rsx! {
                span { class: "dlg-foot__note",
                    "A célwattok az FTP-ből számolódnak (Beállítások → FTP), a trainer ERG-módban tartja őket. Saját edzés: MyWhoosh / Zwift fájl (XML, ZWO, JSON)."
                }
                div { class: "dlg-foot__end",
                    button {
                        class: "btn btn--ghost btn--sm",
                        r#type: "button",
                        title: "MyWhoosh / Zwift edzésfájl (XML, ZWO, JSON) importálása",
                        onclick: move |_| on_import.call(()),
                        "Edzés importálása…"
                    }
                    button {
                        class: "btn btn--ghost",
                        r#type: "button",
                        onclick: move |_| on_select.call(None),
                        "Edzés nélkül"
                    }
                }
            },
            div {
                class: "dlg-workouts",
                "data-page": if *details_page.read() { "details" } else { "list" },
                "data-tags": if *tags_open.read() { "open" } else { "closed" },
                div { class: "dlg-phonebar", role: "tablist", aria_label: "Edzésválasztó",
                    button {
                        class: if *details_page.read() { "dlg-phonetab" } else { "dlg-phonetab dlg-phonetab--on" },
                        r#type: "button",
                        role: "tab",
                        "aria-selected": if *details_page.read() { "false" } else { "true" },
                        onclick: move |_| details_page.set(false),
                        "Lista"
                        span { class: "dlg-phonetab__n num", "{shown}" }
                    }
                    button {
                        class: if *details_page.read() { "dlg-phonetab dlg-phonetab--on" } else { "dlg-phonetab" },
                        r#type: "button",
                        role: "tab",
                        "aria-selected": if *details_page.read() { "true" } else { "false" },
                        onclick: move |_| details_page.set(true),
                        "Részletek"
                    }
                    button {
                        class: if *tags_open.read() { "dlg-phonetab dlg-phonetab--filter dlg-phonetab--on" } else { "dlg-phonetab dlg-phonetab--filter" },
                        r#type: "button",
                        "aria-pressed": if *tags_open.read() { "true" } else { "false" },
                        onclick: move |_| {
                            let open = *tags_open.peek();
                            tags_open.set(!open);
                            details_page.set(false);
                        },
                        "Címkék"
                        if !tags.is_empty() {
                            span { class: "dlg-phonetab__n num", "{tags.len()}" }
                        }
                    }
                }
                aside { class: "dlg-workouts__cats", aria_label: "Edzéskategóriák",
                    div { class: "dlg-cats",
                    button {
                        class: if filter.is_none() { "dlg-cat dlg-cat--active" } else { "dlg-cat" },
                        r#type: "button",
                        "aria-pressed": if filter.is_none() { "true" } else { "false" },
                        onclick: move |_| category.set(None),
                        span { class: "dlg-cat__name", "Mind" }
                        span { class: "dlg-cat__count num", "{total}" }
                    }
                    for (cat, n) in categories {
                        button {
                            key: "cat-{cat.label()}",
                            class: match (filter == Some(cat), n) {
                                (true, _) => "dlg-cat dlg-cat--active",
                                (false, 0) => "dlg-cat dlg-cat--empty",
                                _ => "dlg-cat",
                            },
                            r#type: "button",
                            "aria-pressed": if filter == Some(cat) { "true" } else { "false" },
                            title: "{cat.hint()}",
                            onclick: move |_| category.set(Some(cat)),
                            span { class: "dlg-cat__name", "{cat.label()}" }
                            span { class: "dlg-cat__count num", "{n}" }
                        }
                    }
                    }
                    for group in TagGroup::ALL {
                        div { key: "tags-{group.label()}", class: "dlg-tags", role: "group", aria_label: "{group.label()}",
                            span { class: "dlg-tags__title label", "{group.label()}" }
                            div { class: "dlg-tags__chips",
                                for (tag, n) in tag_counts.iter().copied().filter(|(t, _)| t.group() == group) {
                                    button {
                                        key: "tag-{tag.id()}",
                                        class: if tags.contains(&tag) { "chip chip--accent dlg-tag" } else { "chip dlg-tag" },
                                        r#type: "button",
                                        "aria-pressed": if tags.contains(&tag) { "true" } else { "false" },
                                        disabled: n == 0 && !tags.contains(&tag),
                                        title: "{tag.hint()}",
                                        onclick: move |_| {
                                            let mut list = picked.write();
                                            if let Some(i) = list.iter().position(|t| *t == tag) {
                                                list.remove(i);
                                            } else {
                                                list.push(tag);
                                            }
                                        },
                                        span { class: "dlg-tag__name", "{tag.label()}" }
                                        span { class: "dlg-tag__count num", "{n}" }
                                    }
                                }
                            }
                        }
                    }
                }
                section { class: "dlg-workouts__list", aria_label: "Edzések",
                    if imported_count > 0 || !tags.is_empty() {
                        div { class: "dlg-workouts__bar",
                            if imported_count > 0 {
                                span { class: "dlg-workouts__found num", "{imported_count} importált" }
                            }
                            if !tags.is_empty() {
                                span { class: "dlg-workouts__found num", "{shown} találat" }
                                button {
                                    class: "btn btn--ghost btn--sm",
                                    r#type: "button",
                                    onclick: move |_| picked.write().clear(),
                                    "Címkék törlése"
                                }
                            }
                        }
                    }
                    if workouts.is_empty() {
                        p { class: "dlg-workouts__none",
                            "Ebben a kategóriában nincs a címkéknek megfelelő edzés. Válaszd a „Mind” nézetet, vagy vegyél le egy címkét."
                        }
                    }
                    for workout in workouts {
                        button {
                            key: "{workout.id}",
                            class: if selected.as_deref() == Some(workout.id.as_str()) {
                                "dlg-workout dlg-workout--active"
                            } else {
                                "dlg-workout"
                            },
                            r#type: "button",
                            "aria-pressed": if selected.as_deref() == Some(workout.id.as_str()) { "true" } else { "false" },
                            onclick: {
                                let id = workout.id.clone();
                                move |_| {
                                    on_select.call(Some(id.clone()));
                                    // phone: show what was picked
                                    details_page.set(true);
                                }
                            },
                            div { class: "dlg-workout__top",
                                span { class: "dlg-workout__name", "{workout.name}" }
                                span { class: "dlg-workout__time num", "{workout.duration_text()}" }
                            }
                            div { class: "dlg-workout__meta",
                                span { class: "tag", "{workout.category.label()}" }
                                span { class: "dlg-workout__pct num", "{workout.ftp_pct}% FTP" }
                                if workout.is_imported() {
                                    span { class: "tag tag--data", "Importált" }
                                }
                                if selected.as_deref() == Some(workout.id.as_str()) {
                                    span { class: "tag tag--solid", "Kiválasztva" }
                                }
                            }
                            p { class: "dlg-workout__goal", "{workout.goal}" }
                            div { class: "dlg-workout__tags",
                                for tag in card_tags(workout) {
                                    span { key: "{tag.id()}", class: "tag dlg-workout__tag", "{tag.label()}" }
                                }
                            }
                        }
                    }
                }
                aside { class: "dlg-workouts__detail", aria_label: "Edzés részletei",
                    if let Some(w) = current {
                        header { class: "dlg-workout-detail__head",
                            h3 { class: "dlg-workout-detail__name", "{w.name}" }
                            span { class: "dlg-workout-detail__meta",
                                "{w.category.label()} · {w.duration_text()} · {w.ftp_pct}% FTP"
                            }
                            if w.is_imported() {
                                span { class: "tag tag--data", "Importált" }
                            }
                        }
                        div { class: "dlg-workout-detail__tags",
                            for tag in w.tag_list() {
                                span {
                                    key: "{tag.id()}",
                                    class: if tag.group() == TagGroup::Goal { "tag tag--data" } else { "tag" },
                                    title: "{tag.hint()}",
                                    "{tag.label()}"
                                }
                            }
                        }
                        p { class: "dlg-workout-detail__text", "{w.description}" }
                        if let Some(chart) = &chart {
                            div {
                                class: "dlg-wchart",
                                role: "img",
                                aria_label: "Az edzés profilja: {w.steps.len()} blokk, {w.duration_text()}, {w.ftp_pct}% FTP",
                                div { class: "dlg-wchart__plot",
                                    svg {
                                        class: "dlg-wchart__svg",
                                        view_box: "0 0 1000 1000",
                                        preserve_aspect_ratio: "none",
                                        "aria-hidden": "true",
                                        for (index, block) in chart.blocks.iter().enumerate() {
                                            polygon {
                                                key: "block-{index}",
                                                class: if workout_chart::gets_gap(block.width) { "dlg-wchart__block dlg-wchart__block--gap" } else { "dlg-wchart__block" },
                                                points: "{workout_chart::polygon(block, chart.ceiling)}",
                                                fill: if block.zone == 0 { "var(--rule-ink)".to_string() } else { power_zone_var(block.zone).to_string() },
                                            }
                                        }
                                    }
                                    // the FTP line: the one reference every block is measured against
                                    div {
                                        class: "dlg-wchart__ftp",
                                        style: "top: calc(10px + (100% - 10px) * {workout_chart::y_of(100.0, chart.ceiling) / 1000.0:.4})",
                                        span { "FTP" }
                                    }
                                }
                                div { class: "dlg-wchart__legend",
                                    for zone in 1..=7u8 {
                                        span {
                                            key: "zone-{zone}",
                                            class: "dlg-wchart__zone",
                                            title: "{power_zone_name(zone)}",
                                            span { class: "dlg-wchart__swatch", style: "background: {power_zone_var(zone)}" }
                                            span { class: "dlg-wchart__zname num", "Z{zone}" }
                                        }
                                    }
                                    span { class: "dlg-wchart__note num", "0–{chart.ceiling:.0}% FTP" }
                                }
                            }
                        }
                        div { class: "dlg-steps__head",
                            span { class: "label", "Blokkok" }
                            span { class: "dlg-steps__sum num", "{w.steps.len()} blokk" }
                        }
                        ol { class: "dlg-steps",
                            for step in w.steps.iter() {
                                li { class: "dlg-step",
                                    span { class: "dlg-step__time num", "{step_duration(step.duration_s)}" }
                                    div { class: "dlg-step__body",
                                        span { class: "dlg-step__name", "{step.name}" }
                                        span { class: "dlg-step__kind", "{step.kind.label()}" }
                                        if let Some(note) = step.note.as_deref() {
                                            span { class: "dlg-step__note", "{note}" }
                                        }
                                    }
                                    span { class: "dlg-step__target num", "{target_text(&step.target)}" }
                                }
                            }
                        }
                        if !w.goal.is_empty() {
                            p { class: "dlg-workout-detail__goal", "{w.goal}" }
                        }
                        if !w.sources.is_empty() {
                            div { class: "dlg-workout-detail__sources",
                                span { class: "label", "Források" }
                                ul {
                                    for url in w.sources.iter() {
                                        li { key: "{url}", title: "{url}", "{source_host(url)}" }
                                    }
                                }
                            }
                        }
                    } else {
                        div { class: "dlg-workout-detail__empty",
                            Icon { name: IconName::Bike, size: 28 }
                            p { "Válassz egy edzést a listából, vagy tekerj szabadon." }
                        }
                    }
                }
            }
        }
    }
}

/// The card's tags: what the workout is for first, then the evidence/key-session marks.
fn card_tags(workout: &Workout) -> Vec<WorkoutTag> {
    let all = workout.tag_list();
    let goals = all.iter().copied().filter(|t| t.group() == TagGroup::Goal);
    let marks = all
        .iter()
        .copied()
        .filter(|t| matches!(t, WorkoutTag::EvidenceBased | WorkoutTag::KeySession));
    goals.take(CARD_TAGS).chain(marks).collect()
}

/// The host of a source URL (`www.` dropped), for the compact source list.
fn source_host(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let host = rest.split('/').next().unwrap_or(rest);
    host.strip_prefix("www.").unwrap_or(host)
}

/// `m:ss` of one step.
fn step_duration(seconds: u32) -> String {
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

/// Human readable target of a step.
fn target_text(target: &StepTarget) -> String {
    match *target {
        StepTarget::PctFtp { value } => format!("{value:.0}%"),
        StepTarget::Watts { value } => format!("{value:.0} W"),
        StepTarget::RampPct { from, to } => format!("{from:.0}→{to:.0}%"),
        StepTarget::RampWatts { from, to } => format!("{from:.0}→{to:.0} W"),
        StepTarget::StepWatts {
            start,
            step,
            step_s,
            max,
        } => format!("{start:.0} W +{step:.0}/{step_s:.0}s (max {max:.0})"),
        StepTarget::Free => "szabad".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(selected: Option<String>) -> String {
        render_with(selected, Vec::new())
    }

    fn render_with(selected: Option<String>, imported: Vec<Workout>) -> String {
        #[derive(Clone, PartialEq, Props)]
        struct P {
            selected: Option<String>,
            imported: Vec<Workout>,
        }
        #[component]
        fn Host(props: P) -> Element {
            let imported = use_signal(|| props.imported.clone());
            rsx! {
                WorkoutsDialog {
                    open: true,
                    selected: props.selected.clone(),
                    imported,
                    on_import: move |_| {},
                    on_select: move |_| {},
                    on_close: move |_| {},
                }
            }
        }
        let mut vdom = VirtualDom::new_with_props(Host, P { selected, imported });
        vdom.rebuild_in_place();
        dioxus_ssr::render(&vdom)
    }

    fn imported_workout() -> Workout {
        use ride_model::{StepKind, WorkoutStep};
        let step = |duration_s: u32, target: StepTarget| WorkoutStep {
            kind: StepKind::Steady,
            name: "blokk".into(),
            duration_s,
            target,
            cadence: None,
            note: None,
            messages: Vec::new(),
        };
        Workout {
            id: "import-00000000000000aa".into(),
            name: "Pacing #2".into(),
            category: WorkoutCategory::Endurance,
            goal: "Judge your pacing.".into(),
            description: "Importált leírás".into(),
            ftp_pct: 70,
            duration_min: 20,
            tags: vec![],
            sources: vec!["Pacing #2.xml".into()],
            steps: vec![
                step(600, StepTarget::PctFtp { value: 50.0 }),
                step(300, StepTarget::PctFtp { value: 90.0 }),
                step(
                    300,
                    StepTarget::RampPct {
                        from: 60.0,
                        to: 105.0,
                    },
                ),
            ],
        }
    }

    #[test]
    fn lists_categories_and_the_library() {
        let html = render(None);
        assert!(html.contains("Mind"), "{html}");
        assert!(html.contains("Állóképesség"));
        assert!(html.contains("Küszöb"));
        assert!(html.contains("Ramp teszt"), "some workout name is listed");
        assert_eq!(
            html.matches("dlg-workout\"").count(),
            WorkoutLibrary::embedded().len(),
            "all workouts are listed in the all view"
        );
        assert!(html.contains("Edzés nélkül"));
    }

    #[test]
    fn the_selected_workout_has_its_steps() {
        let lib = WorkoutLibrary::embedded();
        let w = lib
            .by_category(WorkoutCategory::Threshold)
            .first()
            .copied()
            .expect("a threshold workout");
        let html = render(Some(w.id.clone()));
        assert!(html.contains("Kiválasztva"));
        assert!(html.contains("Blokkok"));
        assert!(html.contains(&w.steps[0].name), "the first step is listed");
        assert!(html.contains("Bemelegítés"), "step kind labels");
        assert!(html.contains("FTP"), "targets are explained");
    }

    #[test]
    fn the_selected_workout_is_drawn_as_zone_coloured_blocks() {
        let workout = imported_workout();
        let html = render_with(Some(workout.id.clone()), vec![workout.clone()]);
        assert!(html.contains("dlg-wchart"), "the chart is missing");
        // three steps, three blocks; the ramp is a trapezoid, so every block is a polygon
        assert_eq!(html.matches("<polygon").count(), 3, "one block per step");
        assert!(html.contains("var(--z1)"), "the 50 % block is zone 1");
        assert!(html.contains("var(--z4)"), "the 90 % block is zone 4");
        assert!(
            html.contains("0–140% FTP"),
            "the axis tops out above the ramp"
        );
        assert!(
            html.contains("dlg-wchart__legend"),
            "the zone legend is missing"
        );
        assert!(!html.contains("NaN"));
    }

    #[test]
    fn an_imported_workout_is_listed_marked_and_importable() {
        let workout = imported_workout();
        let html = render_with(None, vec![workout.clone()]);
        assert!(html.contains("Pacing #2"), "the imported workout is listed");
        assert!(html.contains("Importált"), "it is marked as imported");
        assert_eq!(
            html.matches("dlg-workout\"").count(),
            WorkoutLibrary::embedded().len() + 1,
            "the library keeps its size plus the import"
        );
        assert!(html.contains("1 importált"), "the count is shown");
        assert!(
            html.contains("Edzés importálása"),
            "the import button is there"
        );
    }

    #[test]
    fn tag_filters_cards_and_sources_are_rendered() {
        let html = render(None);
        for group in TagGroup::ALL {
            assert!(html.contains(group.label()), "{}", group.label());
        }
        assert_eq!(
            html.matches("dlg-tag\"").count(),
            WorkoutTag::ALL.len(),
            "one chip per tag, none selected"
        );
        assert!(html.contains("Kutatáson alapul"));
        assert!(
            !html.contains("disabled"),
            "every tag has workouts in the all view"
        );
        assert!(
            !html.contains("Címkék törlése"),
            "no clear button without a selection"
        );

        let lib = WorkoutLibrary::embedded();
        let w = lib.get("d-vo2-ronnestad-30-15").expect("the 30/15 workout");
        let html = render(Some(w.id.clone()));
        assert!(html.contains("dlg-workout-detail__tags"));
        assert!(html.contains("Források"));
        assert!(html.contains("trainright.com"), "source hosts are listed");
        assert!(html.contains("dlg-step__note"), "step notes are shown");
    }

    #[test]
    fn card_tags_lead_with_the_purpose() {
        let lib = WorkoutLibrary::embedded();
        for w in lib.all() {
            let tags = card_tags(w);
            assert!(!tags.is_empty(), "{}", w.id);
            assert_eq!(tags[0].group(), TagGroup::Goal, "{}", w.id);
            assert!(tags.len() <= CARD_TAGS + 2);
        }
        assert_eq!(
            source_host("https://www.frontiersin.org/a/b"),
            "frontiersin.org"
        );
        assert_eq!(
            source_host("https://pubmed.ncbi.nlm.nih.gov/17414804/"),
            "pubmed.ncbi.nlm.nih.gov"
        );
        assert_eq!(source_host("nope"), "nope");
    }

    #[test]
    fn target_and_duration_formatting() {
        assert_eq!(step_duration(65), "1:05");
        assert_eq!(step_duration(3600), "60:00");
        assert_eq!(target_text(&StepTarget::PctFtp { value: 88.0 }), "88%");
        assert_eq!(target_text(&StepTarget::Watts { value: 250.0 }), "250 W");
        assert_eq!(
            target_text(&StepTarget::RampPct {
                from: 45.0,
                to: 75.0
            }),
            "45→75%"
        );
        assert!(target_text(&StepTarget::Free).contains("szabad"));
        assert!(target_text(&StepTarget::StepWatts {
            start: 100.0,
            step: 20.0,
            step_s: 60.0,
            max: 600.0
        })
        .contains("+20/60s"));
    }
}
