//! Swiping between the views of the profile panel on a phone.
//!
//! On a phone the header buttons (Teljes / Előretekintés, Magasság / Edzés) cost a row of an
//! already short screen. There the views become pages: a horizontal swipe over the plot turns
//! the page (left = next, right = previous), and the header shows where the rider is with the
//! page name and a row of dots, the usual sign of a swipeable strip. The dots can be tapped too.
//! Until the rider has swiped once, a "húzd oldalra" hint stands next to them and the plot nudges
//! sideways twice, so the gesture is discovered rather than explained.

use dioxus::prelude::*;
use ride_i18n::t;

/// A swipe has to travel this far sideways, CSS pixels.
pub const SWIPE_MIN_PX: f64 = 48.0;

/// The pages the profile can turn through, and where it is.
#[derive(Clone, PartialEq)]
pub struct Pages {
    pub labels: Vec<String>,
    pub current: usize,
    /// The rider has not swiped yet: show the hint.
    pub hint: bool,
    /// The page to turn to.
    pub on_page: EventHandler<usize>,
}

impl Pages {
    /// The page a step of `delta` lands on, `None` past either end.
    pub fn step(&self, delta: i32) -> Option<usize> {
        let next = self.current as i64 + i64::from(delta);
        (0..self.labels.len() as i64)
            .contains(&next)
            .then_some(next as usize)
    }
}

/// The page step of a pointer travel `(dx, dy)`: `+1` for a swipe to the left (next page), `-1`
/// to the right, `None` for a tap, a short drag or a mostly vertical one (a scroll).
pub fn swipe_step(dx: f64, dy: f64) -> Option<i32> {
    if !(dx.is_finite() && dy.is_finite()) || dx.abs() < SWIPE_MIN_PX || dx.abs() < dy.abs() * 1.5 {
        return None;
    }
    Some(if dx < 0.0 { 1 } else { -1 })
}

/// The page name, the dots and (until the first swipe) the hint, in the header.
#[component]
pub fn PageDots(pages: Pages) -> Element {
    let label = pages.labels.get(pages.current).cloned().unwrap_or_default();
    let on_page = pages.on_page;
    rsx! {
        div { class: "profile-pages", role: "tablist", aria_label: t("ui_profile.swipe.a_profil_nezetei").to_string(),
            if pages.hint {
                span { class: "profile-pages__hint", {t("ui_profile.swipe.huzd_oldalra")} }
            }
            span { class: "profile-pages__label", "{label}" }
            span { class: "profile-pages__dots",
                for (i , name) in pages.labels.iter().enumerate() {
                    button {
                        key: "{i}",
                        class: if i == pages.current { "profile-pages__dot is-current" } else { "profile-pages__dot" },
                        r#type: "button",
                        role: "tab",
                        "aria-selected": if i == pages.current { "true" } else { "false" },
                        aria_label: "{name}",
                        onclick: move |_| on_page.call(i),
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
    fn a_sideways_drag_turns_the_page_and_a_tap_or_a_scroll_does_not() {
        assert_eq!(swipe_step(-80.0, 10.0), Some(1), "to the left: next");
        assert_eq!(swipe_step(90.0, -20.0), Some(-1), "to the right: previous");
        assert_eq!(swipe_step(20.0, 0.0), None, "too short");
        assert_eq!(swipe_step(60.0, 70.0), None, "mostly vertical: a scroll");
        assert_eq!(swipe_step(f64::NAN, 0.0), None);
    }
}
