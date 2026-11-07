//! The workout view of the profile panel: the blocks of the running workout over time, the
//! rider's place in them and what comes next.
//!
//! In ERG mode the trainer holds the target whatever the grade, so while a workout runs the
//! useful picture is not the terrain but the block sequence: how long the current block still
//! lasts and how hard the next one is. Drawn like the elevation profile — flat zone-coloured
//! blocks in a stretched `0 0 1000 1000` SVG, the done part covered by the surface colour, a
//! hairline and a disc at "now", a time ruler along the bottom — so switching between the two
//! views keeps the same reading habits.

use std::rc::Rc;

use dioxus::prelude::*;
use ride_model::{ProfileMode, ProfileView, RideSnapshot, RideState, Workout};
use ride_ui_kit::format::{fmt_duration, fmt_hm_text};
use ride_ui_kit::palette::power_zone_var;
use ride_ui_kit::workout_chart::{self, Block};
use ride_ui_kit::{Icon, IconName, Segmented};

use crate::{measure_width, pct_text, pointer_frac, swipe, Pages, Pair, ViewSwitch, TIP_FLIP_FRAC};
use ride_i18n::{t, tf};

/// Length of the look-ahead window, seconds (the whole workout when it is shorter).
pub const LOOKAHEAD_S: f64 = 20.0 * 60.0;

/// Share of the look-ahead window kept behind the rider, so the block just finished stays in
/// sight.
const BEHIND_FRAC: f64 = 0.15;

/// Pixels per ruler numeral the ruler aims for.
const TICK_PX: f64 = 96.0;

/// The time window (seconds from the workout start) the view shows.
pub fn window(total_s: f64, mode: ProfileMode, elapsed_s: f64) -> (f64, f64) {
    if !(total_s.is_finite() && total_s > 0.0) {
        return (0.0, 1.0);
    }
    let len = LOOKAHEAD_S.min(total_s);
    match mode {
        ProfileMode::Full => (0.0, total_s),
        ProfileMode::Lookahead if len >= total_s => (0.0, total_s),
        ProfileMode::Lookahead => {
            let elapsed = if elapsed_s.is_finite() {
                elapsed_s
            } else {
                0.0
            };
            let start = (elapsed - len * BEHIND_FRAC).clamp(0.0, total_s - len);
            (start, start + len)
        }
    }
}

/// Ruler step (seconds) for a window of `span_s` drawn `width_px` wide: about one numeral per
/// [`TICK_PX`] pixels, on whole minutes.
pub fn tick_step(span_s: f64, width_px: f64) -> f64 {
    const STEPS: [f64; 8] = [60.0, 120.0, 300.0, 600.0, 900.0, 1800.0, 3600.0, 7200.0];
    let width = if width_px.is_finite() && width_px > 1.0 {
        width_px
    } else {
        800.0
    };
    let fits = (width / TICK_PX).max(2.0);
    STEPS
        .into_iter()
        .find(|step| span_s / step <= fits)
        .unwrap_or(7200.0)
}

/// CSS `top` of a height given in percent of the plot (`0` = top), below the headroom the
/// plot keeps free over the highest block (`--wpad`, see `profile.css`).
fn top_css(pct: f64) -> String {
    format!(
        "calc(var(--wpad) + (100% - var(--wpad)) * {:.4})",
        (if pct.is_finite() { pct } else { 0.0 }).clamp(0.0, 100.0) / 100.0
    )
}

/// Watts of `pct` percent of FTP.
fn watts(pct: f64, ftp_w: f64) -> f64 {
    pct * ftp_w / 100.0
}

/// Target text of a block: `250 W`, `180→260 W` for a ramp, `szabad` for a free step.
fn block_target(block: &Block, ftp_w: f64) -> String {
    if block.zone == 0 {
        return "szabad".to_string();
    }
    let (a, b) = (
        watts(block.from, ftp_w).round(),
        watts(block.to, ftp_w).round(),
    );
    if (a - b).abs() < 1.0 {
        format!("{a:.0} W")
    } else {
        format!("{a:.0}→{b:.0} W")
    }
}

/// Workout blocks with the rider's position, a Full / Look-ahead toggle (`on_mode`, the same
/// setting as the elevation profile's) and the Elevation / Workout switch (`on_view`).
#[component]
pub fn WorkoutProfile(
    workout: ReadSignal<Option<Workout>>,
    snapshot: ReadSignal<RideSnapshot>,
    ftp_w: ReadSignal<f64>,
    mode: ReadSignal<ProfileMode>,
    on_mode: EventHandler<ProfileMode>,
    on_view: EventHandler<ProfileView>,
    /// Phone: the views are pages turned by swiping, instead of the header buttons.
    #[props(default)]
    pages: Option<Pages>,
) -> Element {
    let mut hover = use_signal(|| None::<f64>);
    let width = use_signal(|| 0.0_f64);
    let mut mounted = use_signal(|| None::<Rc<MountedData>>);

    // phone: a sideways swipe over the plot turns the page (see `swipe.rs`)
    let mut swipe_from = use_signal(|| None::<(f64, f64)>);
    let mut swiped = use_signal(|| false);
    let chart = use_memo(move || {
        workout
            .read()
            .as_ref()
            .map(|w| workout_chart::chart(w, *ftp_w.read()))
    });

    let chart_ref = chart.read();
    let Some(chart) = chart_ref.as_ref().filter(|c| !c.blocks.is_empty()) else {
        return rsx! {
            div { class: "profile profile--empty", "aria-label": t("ui_profile.edzes"),
                div { class: "profile-empty",
                    Icon { name: IconName::Bike, size: 24 }
                    span { class: "profile-empty-title", {t("ui_profile.workout.nincs_kivalasztott_edzes")} }
                    span { class: "profile-empty-hint", {t("ui_profile.workout.az_edzes_gombbal_valaszthatsz_a_felso_savban")} }
                }
            }
        };
    };
    let ftp = *ftp_w.read();
    let mode_v = *mode.read();
    let snap = snapshot.read();
    let progress = snap.workout.as_ref();
    let total = chart.total_s;
    let elapsed = progress
        .map(|p| if p.finished { total } else { p.total_elapsed_s })
        .filter(|t| t.is_finite())
        .unwrap_or(0.0)
        .clamp(0.0, total);
    let started = snap.state != RideState::Empty && elapsed > 0.0;
    let (t0, t1) = window(total, mode_v, elapsed);
    let span = t1 - t0;
    let x_pct = |t: f64| (t - t0) / span * 100.0;
    let ceiling = chart.ceiling;
    let y_pct = |pct: f64| workout_chart::y_of(pct, ceiling) / 10.0;

    let current = chart.block_at(elapsed).copied();
    let next = current.and_then(|c| chart.blocks.get(c.step + 1).copied());
    let finished = progress.is_some_and(|p| p.finished);

    // blocks of the window: (key, points, fill, classes)
    let blocks: Vec<(usize, String, &'static str, &'static str)> = chart
        .blocks
        .iter()
        .filter_map(|block| {
            let points = workout_chart::polygon_in(block, ceiling, t0, t1)?;
            let width = block.duration_s / span * 100.0;
            let now = current.is_some_and(|c| c.step == block.step) && !finished;
            let class = match (now, workout_chart::gets_gap(width)) {
                (true, _) => "profile-wblock profile-wblock--now",
                (false, true) => "profile-wblock profile-wblock--gap",
                (false, false) => "profile-wblock",
            };
            let fill = if block.zone == 0 {
                "var(--rule-ink)"
            } else {
                power_zone_var(block.zone)
            };
            Some((block.step, points, fill, class))
        })
        .collect();

    // time ruler
    let step = tick_step(span, *width.read());
    let mut ticks: Vec<(f64, f64, Option<String>)> = Vec::new();
    let minor = if step <= 120.0 {
        step / 2.0
    } else {
        step / 5.0
    };
    let mut tick = (t0 / minor).ceil() * minor;
    while tick <= t1 + 1e-6 {
        let major = (tick / step).round() * step;
        let label = ((tick - major).abs() < 1e-6).then(|| fmt_hm_text(tick));
        ticks.push((tick, x_pct(tick), label));
        tick += minor;
    }

    // read-out
    let target_now = progress
        .and_then(|p| p.target_w)
        .map(|w| format!("{w:.0}"))
        .unwrap_or_else(|| "—".to_string());
    let left_now = progress.map(|p| fmt_duration(p.step_remaining_s));
    let hover_info = hover.read().and_then(|frac| {
        let t = t0 + frac * span;
        chart.block_at(t).map(|b| (frac, *b))
    });
    let steps = workout.read();
    let step_name = |index: usize| {
        steps
            .as_ref()
            .and_then(|w| w.steps.get(index))
            .map(|s| s.name.clone())
            .unwrap_or_default()
    };
    let next_text = next.map(|n| (step_name(n.step), block_target(&n, ftp)));
    let next_in = next.map(|n| fmt_duration(n.start_s - elapsed));
    let summary = match (&current, &next_text) {
        (Some(c), Some((name, target))) => tf(
            "ui_profile.workout.edzes_kovetkezik_name_target",
            &[
                ("step", &step_name(c.step)),
                ("current", &block_target(c, ftp)),
                ("name", name),
                ("target", target),
            ],
        ),
        _ => t("ui_profile.workout.edzes_blokkjai").to_string(),
    };
    let ftp_top = y_pct(100.0);
    let now_x = x_pct(elapsed);
    let now_y = y_pct(current.map(|c| c.pct_at(elapsed)).unwrap_or(0.0));
    // where the next block starts, for its countdown flag (only when it is in the window)
    let next_flag = next
        .filter(|n| n.start_s > t0 && n.start_s < t1 && !finished)
        .map(|n| (x_pct(n.start_s), y_pct(n.from.max(n.to))));

    rsx! {
        div { class: if pages.as_ref().is_some_and(|p| p.hint) { "profile profile--workout profile--nudge" } else { "profile profile--workout" },
            div { class: if pages.is_some() { "profile-head profile-head--pages" } else { "profile-head" },
                div { class: "profile-readout",
                    if let Some((_, b)) = hover_info {
                        Pair { label: t("ui_profile.workout.blokk").to_string(), value: step_name(b.step) }
                        Pair { label: t("ui_profile.workout.cel").to_string(), value: block_target(&b, ftp) }
                        Pair { label: t("ui_profile.workout.hossz").to_string(), value: fmt_duration(b.duration_s) }
                    } else if finished {
                        Pair { label: t("ui_profile.edzes").to_string(), value: t("ui_profile.workout.kesz").to_string() }
                    } else {
                        Pair { label: t("ui_profile.workout.cel").to_string(), value: target_now, unit: "W".to_string() }
                        if let Some(left) = left_now {
                            Pair { label: t("ui_profile.workout.blokk_vege").to_string(), value: left }
                        }
                        if let (Some((name, target)), Some(when)) = (next_text.clone(), next_in.clone()) {
                            Pair {
                                label: tf("ui_hud.kovetkezik_name", &[("name", &name)]),
                                value: target,
                                unit: tf("ui_profile.workout.when_mulva", &[("when", &when)]),
                            }
                        }
                    }
                }
                if let Some(pages) = pages.clone() {
                    swipe::PageDots { pages }
                } else {
                    div { class: "profile-switches",
                        Segmented {
                            options: vec![
                                ("full".to_string(), t("ui_profile.teljes").to_string()),
                                ("lookahead".to_string(), t("ui_profile.eloretekintes").to_string()),
                            ],
                            value: if mode_v == ProfileMode::Full { "full".to_string() } else { "lookahead".to_string() },
                            on_change: move |value: String| {
                                on_mode.call(if value == "full" { ProfileMode::Full } else { ProfileMode::Lookahead });
                            },
                            aria_label: t("ui_profile.workout.edzes_nezet").to_string(),
                            small: true,
                        }
                        ViewSwitch { view: ProfileView::Workout, on_view }
                    }
                }
            }
            div { class: "profile-plot", role: "img", "aria-label": "{summary}",
                svg {
                    class: "profile-svg profile-wsvg",
                    view_box: "0 0 1000 1000",
                    preserve_aspect_ratio: "none",
                    "aria-hidden": "true",
                    for (key , points , fill , class) in blocks {
                        polygon { key: "wb-{key}", class, points, fill }
                    }
                }
                div { class: "profile-grid-h", style: "top: {top_css(y_pct(50.0))}" }
                div { class: "profile-ftp", style: "top: {top_css(ftp_top)}",
                    span { "FTP {ftp:.0} W" }
                }
                if started {
                    div { class: "profile-done", style: "width: {pct_text(now_x)}" }
                }
                if let (Some((x, y)), Some(when)) = (next_flag, next_in.clone()) {
                    div {
                        class: if x > 100.0 * TIP_FLIP_FRAC { "profile-wnext profile-wnext--flip" } else { "profile-wnext" },
                        style: "left: {pct_text(x)}; top: max(24px, {top_css(y)})",
                        "{when}"
                    }
                }
                div { class: "profile-guide", style: "left: clamp(0px, {pct_text(now_x)}, calc(100% - 1px))" }
                svg {
                    class: "profile-dot",
                    style: "left: clamp(8px, {pct_text(now_x)}, calc(100% - 8px)); top: clamp(8px, {top_css(now_y)}, calc(100% - 8px))",
                    width: "16",
                    height: "16",
                    view_box: "0 0 16 16",
                    "aria-hidden": "true",
                    circle {
                        cx: "8",
                        cy: "8",
                        r: "6.5",
                        fill: "var(--accent)",
                        stroke: "var(--marker-ring)",
                        stroke_width: "3",
                    }
                }
                if let Some((frac, _)) = hover_info {
                    div {
                        class: "profile-guide profile-guide--hover",
                        style: "left: {pct_text(frac * 100.0)}",
                    }
                }
                div {
                    class: if pages.is_some() { "profile-hit profile-hit--readonly profile-hit--swipe" } else { "profile-hit profile-hit--readonly" },
                    onpointerdown: move |event: PointerEvent| {
                        let p = event.client_coordinates();
                        swipe_from.set(Some((p.x, p.y)));
                        swiped.set(false);
                    },
                    onpointercancel: move |_| swipe_from.set(None),
                    onpointerup: {
                        let pages = pages.clone();
                        move |event: PointerEvent| {
                            let Some((x0, y0)) = swipe_from.take() else {
                                return;
                            };
                            let p = event.client_coordinates();
                            let turned = pages.as_ref().and_then(|pages| {
                                let to = pages.step(swipe::swipe_step(p.x - x0, p.y - y0)?)?;
                                pages.on_page.call(to);
                                Some(())
                            });
                            if turned.is_some() {
                                swiped.set(true);
                                hover.set(None);
                            }
                        }
                    },
                    onmounted: move |event: MountedEvent| {
                        mounted.set(Some(event.data()));
                        measure_width(mounted, width);
                    },
                    onmouseenter: move |_| measure_width(mounted, width),
                    onmousemove: move |event: MouseEvent| {
                        let w = *width.peek();
                        if let Some(frac) = pointer_frac(event.element_coordinates().x, w) {
                            hover.set(Some(frac));
                        }
                    },
                    onmouseleave: move |_| hover.set(None),
                }
            }
            div { class: "profile-ruler", "aria-hidden": "true",
                for (t , left , label) in ticks {
                    div {
                        key: "wt-{t:.0}",
                        class: if label.is_some() { "profile-tick profile-tick--major" } else { "profile-tick" },
                        style: "left: {pct_text(left)}",
                        if let Some(label) = &label {
                            span {
                                class: if left < 1.0 {
                                    "profile-tick-label profile-tick-label--start"
                                } else if left > 97.0 {
                                    "profile-tick-label profile-tick-label--end"
                                } else {
                                    "profile-tick-label"
                                },
                                "{label}"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_follows_the_rider_and_stays_inside_the_workout() {
        assert_eq!(window(3600.0, ProfileMode::Full, 900.0), (0.0, 3600.0));
        // at the start: from zero
        assert_eq!(window(3600.0, ProfileMode::Lookahead, 0.0), (0.0, 1200.0));
        // in the middle: a little behind the rider
        let (a, b) = window(3600.0, ProfileMode::Lookahead, 1800.0);
        assert!((a - (1800.0 - 180.0)).abs() < 1e-9 && (b - a - 1200.0).abs() < 1e-9);
        // at the end: the last 20 minutes
        assert_eq!(
            window(3600.0, ProfileMode::Lookahead, 3590.0),
            (2400.0, 3600.0)
        );
        // a short workout is shown whole
        assert_eq!(window(600.0, ProfileMode::Lookahead, 300.0), (0.0, 600.0));
        assert_eq!(window(0.0, ProfileMode::Full, 0.0), (0.0, 1.0));
        assert_eq!(
            window(600.0, ProfileMode::Lookahead, f64::NAN),
            (0.0, 600.0)
        );
    }

    #[test]
    fn ruler_steps_are_whole_minutes_and_fit_the_width() {
        assert_eq!(tick_step(1200.0, 800.0), 300.0);
        assert_eq!(tick_step(3600.0, 800.0), 600.0);
        assert_eq!(tick_step(600.0, 960.0), 60.0);
        assert_eq!(tick_step(3600.0, 200.0), 1800.0);
        assert_eq!(tick_step(1200.0, f64::NAN), 300.0);
    }

    #[test]
    fn block_targets_read_in_watts() {
        let block = Block {
            step: 0,
            start_s: 0.0,
            duration_s: 60.0,
            left: 0.0,
            width: 10.0,
            from: 50.0,
            to: 100.0,
            zone: 3,
        };
        assert_eq!(block_target(&block, 250.0), "125→250 W");
        let steady = Block { to: 50.0, ..block };
        assert_eq!(block_target(&steady, 250.0), "125 W");
        let free = Block { zone: 0, ..block };
        assert_eq!(block_target(&free, 250.0), "szabad");
    }
}
