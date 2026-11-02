//! The details of a finished ride that Strava asks for (title, description, sport, how it felt,
//! private notes, who can see it, mute), edited in the summary dialog before the ride is saved
//! as a TCX or uploaded.

use dioxus::prelude::*;
use ride_ui_kit::{Segmented, Switch, TextField};

/// Who may see the activity on Strava.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Visibility {
    #[default]
    Everyone,
    Followers,
    OnlyMe,
}

impl Visibility {
    /// The value Strava's own form uses.
    pub fn strava_value(self) -> &'static str {
        match self {
            Visibility::Everyone => "everyone",
            Visibility::Followers => "followers_only",
            Visibility::OnlyMe => "only_me",
        }
    }

    fn from_key(key: &str) -> Self {
        match key {
            "followers" => Visibility::Followers,
            "only_me" => Visibility::OnlyMe,
            _ => Visibility::Everyone,
        }
    }

    fn key(self) -> &'static str {
        match self {
            Visibility::Everyone => "everyone",
            Visibility::Followers => "followers",
            Visibility::OnlyMe => "only_me",
        }
    }
}

/// What the rider says about the ride. An empty title is replaced at upload time by the time of
/// day ("Délutáni edzés").
#[derive(Clone, Debug, PartialEq)]
pub struct RideDetails {
    pub title: String,
    pub description: String,
    /// Strava's "Virtual Ride" (indoor trainer) instead of a plain "Ride".
    pub virtual_ride: bool,
    /// "How did that activity feel?", perceived exertion 1 to 10.
    pub feel: Option<u8>,
    /// Private notes, only the rider sees them.
    pub notes: String,
    pub visibility: Visibility,
    /// "Don't publish to Home or Club feeds".
    pub mute: bool,
}

impl Default for RideDetails {
    fn default() -> Self {
        Self {
            title: String::new(),
            description: String::new(),
            virtual_ride: true,
            feel: None,
            notes: String::new(),
            visibility: Visibility::Everyone,
            mute: false,
        }
    }
}

/// The exertion choices, Strava's own wording in Hungarian.
const FEEL: [(u8, &str); 5] = [
    (2, "Nagyon könnyű"),
    (4, "Könnyű"),
    (6, "Mérsékelt"),
    (8, "Nehéz"),
    (10, "Maximális erőfeszítés"),
];

/// The form; every edit calls `on_change` with the whole updated value.
#[component]
pub fn RideDetailsForm(
    details: ReadSignal<RideDetails>,
    on_change: EventHandler<RideDetails>,
) -> Element {
    let d = details.read().clone();
    let edit = move |f: fn(&mut RideDetails, String), value: String| {
        let mut next = details.read().clone();
        f(&mut next, value);
        on_change.call(next);
    };
    let feel_value = d.feel.map(|f| f.to_string()).unwrap_or_default();

    rsx! {
        section { class: "dlg-details", "aria-label": "Az edzés adatai",
            span { class: "t-eyebrow", "Az edzés adatai" }
            TextField {
                label: "Cím".to_string(),
                value: d.title.clone(),
                placeholder: "pl. Délutáni edzés (üresen a napszak alapján)".to_string(),
                on_input: move |v| edit(|d, v| d.title = v, v),
            }
            label { class: "field",
                span { class: "field__label", "Leírás" }
                textarea {
                    rows: "2",
                    value: "{d.description}",
                    placeholder: "Milyen volt? Írj róla pár szót.",
                    oninput: move |e| edit(|d, v| d.description = v, e.value()),
                }
            }
            div { class: "dlg-details__row",
                div { class: "dlg-control",
                    span { class: "label", "Sportág" }
                    Segmented {
                        options: vec![
                            ("virtual".to_string(), "Virtuális túra".to_string()),
                            ("ride".to_string(), "Kerékpározás".to_string()),
                        ],
                        value: if d.virtual_ride { "virtual".to_string() } else { "ride".to_string() },
                        aria_label: "Sportág".to_string(),
                        on_change: move |v: String| {
                            let mut next = details.read().clone();
                            next.virtual_ride = v == "virtual";
                            on_change.call(next);
                        },
                    }
                }
                label { class: "field",
                    span { class: "field__label", "Hogy esett?" }
                    select {
                        value: "{feel_value}",
                        onchange: move |e| {
                            let mut next = details.read().clone();
                            next.feel = e.value().parse::<u8>().ok();
                            on_change.call(next);
                        },
                        option { value: "", "Nem adom meg" }
                        for (value , label) in FEEL {
                            option { value: "{value}", selected: d.feel == Some(value), "{label}" }
                        }
                    }
                }
            }
            label { class: "field",
                span { class: "field__label", "Privát jegyzet" }
                textarea {
                    rows: "2",
                    value: "{d.notes}",
                    placeholder: "Csak te látod.",
                    oninput: move |e| edit(|d, v| d.notes = v, e.value()),
                }
            }
            div { class: "dlg-details__row",
                div { class: "dlg-control",
                    span { class: "label", "Ki láthatja" }
                    Segmented {
                        options: vec![
                            ("everyone".to_string(), "Mindenki".to_string()),
                            ("followers".to_string(), "Követők".to_string()),
                            ("only_me".to_string(), "Csak én".to_string()),
                        ],
                        value: d.visibility.key().to_string(),
                        aria_label: "Ki láthatja".to_string(),
                        on_change: move |v: String| {
                            let mut next = details.read().clone();
                            next.visibility = Visibility::from_key(&v);
                            on_change.call(next);
                        },
                    }
                }
                Switch {
                    checked: d.mute,
                    label: "Ne jelenjen meg a hírfolyamban".to_string(),
                    hint: "A profilodon így is látszik.".to_string(),
                    on_change: move |on: bool| {
                        let mut next = details.read().clone();
                        next.mute = on;
                        on_change.call(next);
                    },
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_ride_is_a_public_virtual_ride_with_nothing_said() {
        let d = RideDetails::default();
        assert!(d.virtual_ride && !d.mute && d.feel.is_none());
        assert_eq!(d.visibility.strava_value(), "everyone");
        assert!(d.title.is_empty() && d.description.is_empty() && d.notes.is_empty());
    }

    #[test]
    fn visibility_round_trips_through_its_key() {
        for v in [
            Visibility::Everyone,
            Visibility::Followers,
            Visibility::OnlyMe,
        ] {
            assert_eq!(Visibility::from_key(v.key()), v);
        }
        assert_eq!(Visibility::Followers.strava_value(), "followers_only");
        assert_eq!(Visibility::OnlyMe.strava_value(), "only_me");
    }
}
