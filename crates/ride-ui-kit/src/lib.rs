//! Design system crate: tokens + primitives CSS, palette, icons, formatting and the
//! small set of base components every other UI crate builds on (see docs/DESIGN.md).
//!
//! * [`CSS`] — embedded fonts (Barlow, Barlow Condensed), tokens (dark + light) and the primitive
//!   classes; the shell injects it first.
//! * [`palette`] — validated data colours for SVG/canvas drawing (zones, grades, climbs).
//! * [`Icon`] / [`IconName`] — the inline SVG icon set.
//! * [`Dialog`], [`Segmented`], [`Tabs`], [`Switch`], [`Slider`], [`NumberField`], [`TextField`],
//!   [`ToastHost`] — the interactive primitives, all keyboard accessible.
//! * [`format`] — every number the UI shows goes through these helpers.

use dioxus::prelude::*;
use ride_model::Theme;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

pub mod color;
pub mod format;
mod icons;
pub mod palette;

pub use icons::{icon_inner, icon_svg, IconName, ICON_LINECAP, ICON_LINEJOIN, ICON_STROKE_WIDTH};

/// Embedded fonts (`@font-face` with data URIs, see `assets/fonts.css`), then the tokens and
/// primitive classes (`.btn`, `.tag`, `.cells`/`.cell`, `.tabs`, `.seg`, `.switch`, `.num`,
/// `.zonebar` ...). The app injects this first.
pub const CSS: &str = concat!(
    include_str!("../assets/fonts.css"),
    include_str!("../assets/kit.css")
);

/// Value of the `data-theme` attribute on `<html>` for a theme (`"dark"` / `"light"`).
pub fn theme_attr(theme: Theme) -> &'static str {
    match theme {
        Theme::Dark => "dark",
        Theme::Light => "light",
    }
}

/// A complete, self-contained HTML document: kit CSS first, then every block of `css`, the
/// theme on `<html>` and `body_html` as content. Used by the SSR previews/screenshot checks
/// of all UI crates (the app itself injects the CSS differently).
pub fn html_document(theme: Theme, css: &[&str], body_html: &str) -> String {
    let mut out = String::with_capacity(CSS.len() + body_html.len() + 512);
    out.push_str("<!doctype html><html lang=\"hu\" data-theme=\"");
    out.push_str(theme_attr(theme));
    out.push_str("\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>bike-osm-ride</title><style>");
    out.push_str(CSS);
    for block in css {
        out.push('\n');
        out.push_str(block);
    }
    out.push_str("</style></head><body>");
    out.push_str(body_html);
    out.push_str("</body></html>");
    out
}

static NEXT_UID: AtomicU64 = AtomicU64::new(1);

fn next_uid() -> u64 {
    NEXT_UID.fetch_add(1, Ordering::Relaxed)
}

/// Runs a small script in the webview if the renderer has one (desktop); a no-op in SSR/tests.
fn run_js(script: String) {
    if try_consume_context::<Rc<dyn document::Document>>().is_some() {
        let _ = document::eval(&script);
    }
}

// ───────────────────────────── Icon ─────────────────────────────

/// Inline SVG icon, `currentColor`, 24×24 viewBox, 2 px square-capped stroke, scaled to `size` px.
/// Decorative by default (`aria-hidden`); pass `title` to expose it to assistive tech.
#[component]
pub fn Icon(
    name: IconName,
    #[props(default = 20)] size: u32,
    #[props(default)] title: Option<String>,
) -> Element {
    let inner = icon_inner(name);
    // solid glyphs carry no outline, line icons carry no fill
    let (fill, stroke) = if name.is_filled() {
        ("currentColor", "none")
    } else {
        ("none", "currentColor")
    };
    match title {
        Some(t) => rsx! {
            svg {
                class: "icon",
                view_box: "0 0 24 24",
                width: "{size}",
                height: "{size}",
                fill,
                stroke,
                stroke_width: ICON_STROKE_WIDTH,
                stroke_linecap: ICON_LINECAP,
                stroke_linejoin: ICON_LINEJOIN,
                role: "img",
                "aria-label": "{t}",
                dangerous_inner_html: inner,
            }
        },
        None => rsx! {
            svg {
                class: "icon",
                view_box: "0 0 24 24",
                width: "{size}",
                height: "{size}",
                fill,
                stroke,
                stroke_width: ICON_STROKE_WIDTH,
                stroke_linecap: ICON_LINECAP,
                stroke_linejoin: ICON_LINEJOIN,
                "aria-hidden": "true",
                dangerous_inner_html: inner,
            }
        },
    }
}

// ───────────────────────────── Dialog ─────────────────────────────

/// Modal dialog with backdrop, title bar and close button. Closes on Esc / backdrop click.
/// Focus moves into the dialog when it opens, Tab/Shift+Tab stay inside, and focus returns to
/// the previously focused element on close. Every dialog has the same frame and size (fixed width and
/// height, a scrolling body, a pinned `foot`), so none of them jumps around with its content.
#[component]
pub fn Dialog(
    open: bool,
    title: String,
    on_close: EventHandler<()>,
    #[props(default = false)] wide: bool,
    /// Content that goes edge to edge and manages its own scrolling (multi-pane layouts); the
    /// body then neither pads nor scrolls.
    #[props(default = false)]
    flush: bool,
    /// Pinned footer (buttons, a note). Stays visible while the body scrolls.
    #[props(default = VNode::empty())]
    foot: Element,
    children: Element,
) -> Element {
    if !open {
        return rsx! {};
    }
    rsx! {
        DialogPanel { title, on_close, wide, flush, foot, {children} }
    }
}

/// JS run after the panel is mounted: remember the opener, focus the first useful control and
/// install the Tab trap. `id` is generated by us (`kit-dlg-<n>`), never user input.
fn dialog_open_script(id: &str) -> String {
    format!(
        r#"(function(){{
var d=document.getElementById("{id}"); if(!d) return;
window.__kitDlgPrev=window.__kitDlgPrev||{{}}; window.__kitDlgPrev["{id}"]=document.activeElement;
var sel='a[href],button:not([disabled]),input:not([disabled]),select:not([disabled]),textarea:not([disabled]),[tabindex]:not([tabindex="-1"])';
var vis=function(x){{return x.offsetParent!==null||x===document.activeElement;}};
var first=d.querySelector('[data-autofocus]')||[].filter.call(d.querySelectorAll('.dialog__body '+sel),vis)[0]||[].filter.call(d.querySelectorAll('.dialog__foot '+sel),vis)[0]||d.querySelector('.dialog__close')||d;
try{{first.focus({{preventScroll:true}});}}catch(e){{}}
d.addEventListener('keydown',function(e){{
 if(e.key!=='Tab') return;
 var els=[].filter.call(d.querySelectorAll(sel),vis);
 if(!els.length){{e.preventDefault();d.focus();return;}}
 var a=document.activeElement,f=els[0],l=els[els.length-1];
 if(e.shiftKey&&(a===f||a===d)){{e.preventDefault();l.focus();}}
 else if(!e.shiftKey&&a===l){{e.preventDefault();f.focus();}}
}});
}})();"#
    )
}

fn dialog_close_script(id: &str) -> String {
    format!(
        r#"(function(){{var m=window.__kitDlgPrev; if(!m) return; var p=m["{id}"]; delete m["{id}"];
if(p&&typeof p.focus==="function"&&document.contains(p)){{try{{p.focus({{preventScroll:true}});}}catch(e){{}}}}}})();"#
    )
}

#[component]
fn DialogPanel(
    title: String,
    on_close: EventHandler<()>,
    wide: bool,
    flush: bool,
    foot: Element,
    children: Element,
) -> Element {
    let uid = use_hook(next_uid);
    let panel_id = format!("kit-dlg-{uid}");
    let title_id = format!("kit-dlg-title-{uid}");
    let mut down_on_backdrop = use_signal(|| false);

    {
        let panel_id = panel_id.clone();
        use_effect(move || run_js(dialog_open_script(&panel_id)));
    }
    {
        let panel_id = panel_id.clone();
        use_drop(move || run_js(dialog_close_script(&panel_id)));
    }

    let class = if wide {
        "dialog dialog--wide"
    } else {
        "dialog"
    };
    rsx! {
        div {
            class: "dialog-backdrop",
            onmousedown: move |_| down_on_backdrop.set(true),
            onclick: move |_| {
                // a text selection that starts inside the dialog and ends on the backdrop must not close it
                if down_on_backdrop() {
                    on_close.call(());
                }
                down_on_backdrop.set(false);
            },
            div {
                id: "{panel_id}",
                class,
                role: "dialog",
                "aria-modal": "true",
                "aria-labelledby": "{title_id}",
                tabindex: "-1",
                onmousedown: move |e| {
                    e.stop_propagation();
                    down_on_backdrop.set(false);
                },
                onclick: move |e| e.stop_propagation(),
                onkeydown: move |e| {
                    if e.key() == Key::Escape {
                        e.stop_propagation();
                        on_close.call(());
                    }
                },
                div { class: "dialog__head",
                    h2 { id: "{title_id}", class: "dialog__title", "{title}" }
                    button {
                        class: "btn btn--ghost btn--icon btn--sm dialog__close",
                        r#type: "button",
                        "aria-label": "Bezárás",
                        title: "Bezárás (Esc)",
                        onclick: move |_| on_close.call(()),
                        Icon { name: IconName::Close, size: 16 }
                    }
                }
                div { class: if flush { "dialog__body dialog__body--flush" } else { "dialog__body" }, {children} }
                div { class: "dialog__foot", {foot} }
            }
        }
    }
}

// ───────────────────────────── Segmented ─────────────────────────────

/// Segmented control: `options` = `(value, label)`. Arrow keys / Home / End move the selection
/// (roving tabindex, `role="radiogroup"`). With `tabs` it renders as an underlined tab strip
/// (`role="tablist"`, items `role="tab"`); [`Tabs`] is the shorthand for that.
#[component]
pub fn Segmented(
    options: Vec<(String, String)>,
    value: String,
    on_change: EventHandler<String>,
    #[props(default = String::new())] aria_label: String,
    #[props(default = false)] small: bool,
    #[props(default = false)] block: bool,
    #[props(default = false)] tabs: bool,
) -> Element {
    let mut mounted = use_signal(Vec::<Option<Rc<MountedData>>>::new);
    let count = options.len();
    let current = options.iter().position(|(v, _)| *v == value);
    let class = if tabs {
        "tabs".to_string()
    } else {
        format!(
            "seg{}{}",
            if small { " seg--sm" } else { "" },
            if block { " seg--block" } else { "" }
        )
    };
    let (group_role, item_role, item_base) = if tabs {
        ("tablist", "tab", "tab")
    } else {
        ("radiogroup", "radio", "seg__item")
    };
    let values: Vec<String> = options.iter().map(|(v, _)| v.clone()).collect();

    rsx! {
        div { class, role: group_role, "aria-label": "{aria_label}",
            for (i , (v , label)) in options.into_iter().enumerate() {
                button {
                    key: "{v}",
                    class: if v == value { format!("{item_base} is-active") } else { item_base.to_string() },
                    r#type: "button",
                    role: item_role,
                    "aria-checked": if !tabs { Some(if v == value { "true" } else { "false" }) } else { None },
                    "aria-selected": if tabs { Some(if v == value { "true" } else { "false" }) } else { None },
                    tabindex: if v == value || (current.is_none() && i == 0) { "0" } else { "-1" },
                    onmounted: move |e| {
                        let mut list = mounted.write();
                        if list.len() <= i {
                            list.resize(i + 1, None);
                        }
                        list[i] = Some(e.data());
                    },
                    onclick: {
                        let v = v.clone();
                        move |_| on_change.call(v.clone())
                    },
                    onkeydown: {
                        let values = values.clone();
                        move |e| {
                            if count == 0 {
                                return;
                            }
                            let at = current.unwrap_or(0);
                            let target = match e.key() {
                                Key::ArrowRight | Key::ArrowDown => Some((at + 1) % count),
                                Key::ArrowLeft | Key::ArrowUp => Some((at + count - 1) % count),
                                Key::Home => Some(0),
                                Key::End => Some(count - 1),
                                _ => None,
                            };
                            if let Some(t) = target {
                                e.prevent_default();
                                if let Some(next) = values.get(t) {
                                    on_change.call(next.clone());
                                }
                                let handle = mounted.peek().get(t).cloned().flatten();
                                if let Some(h) = handle {
                                    spawn(async move {
                                        let _ = h.set_focus(true).await;
                                    });
                                }
                            }
                        }
                    },
                    "{label}"
                }
            }
        }
    }
}

/// Underlined tab strip (`role="tablist"`): the same keyboard behaviour as [`Segmented`].
#[component]
pub fn Tabs(
    options: Vec<(String, String)>,
    value: String,
    on_change: EventHandler<String>,
    #[props(default = String::new())] aria_label: String,
) -> Element {
    rsx! {
        Segmented { options, value, on_change, aria_label, tabs: true }
    }
}

// ───────────────────────────── Switch ─────────────────────────────

/// On/off switch (`role="switch"`, space toggles). `row` puts the label left and the switch at
/// the far right, the layout of a settings row; `hint` adds a dim second line.
#[component]
pub fn Switch(
    checked: bool,
    on_change: EventHandler<bool>,
    label: String,
    #[props(default = String::new())] hint: String,
    #[props(default = false)] disabled: bool,
    #[props(default = false)] row: bool,
) -> Element {
    rsx! {
        label { class: if row { "switch switch--row" } else { "switch" },
            input {
                r#type: "checkbox",
                role: "switch",
                checked,
                disabled,
                onchange: move |e| on_change.call(e.checked()),
            }
            span { class: "switch__track", span { class: "switch__thumb" } }
            span { class: "switch__text",
                span { class: "switch__label", "{label}" }
                if !hint.is_empty() {
                    span { class: "switch__hint", "{hint}" }
                }
            }
        }
    }
}

// ───────────────────────────── Slider ─────────────────────────────

/// Range slider with a label and the live value (`unit` appended). Emits values snapped to the
/// step grid and clamped to `[min, max]`.
#[component]
pub fn Slider(
    label: String,
    value: f64,
    min: f64,
    max: f64,
    step: f64,
    #[props(default = String::new())] unit: String,
    on_change: EventHandler<f64>,
    #[props(default = String::new())] hint: String,
    #[props(default = false)] disabled: bool,
) -> Element {
    let shown = format::fmt_step_value(value, step);
    let fill = format::fraction(value, min, max) * 100.0;
    rsx! {
        label { class: "slider",
            span { class: "slider__label", "{label}" }
            output { class: "slider__value",
                "{shown}"
                if !unit.is_empty() {
                    span { class: "slider__unit", "{unit}" }
                }
            }
            input {
                r#type: "range",
                min: "{min}",
                max: "{max}",
                step: "{step}",
                value: "{value}",
                disabled,
                style: "--fill: {fill:.2}%",
                "aria-label": "{label}",
                oninput: move |e| {
                    if let Some(v) = format::parse_number(&e.value()) {
                        on_change.call(format::snap_to_step(v, min, max, step));
                    }
                },
            }
            if !hint.is_empty() {
                span { class: "slider__hint", "{hint}" }
            }
        }
    }
}

// ───────────────────────────── NumberField ─────────────────────────────

/// Labelled numeric input with unit suffix and −/+ buttons; emits only valid numbers inside
/// [min, max] (on Enter / blur / step). While the typed text is out of range the field shows an
/// error state and the valid range instead of silently changing the model.
#[component]
pub fn NumberField(
    label: String,
    value: f64,
    min: f64,
    max: f64,
    step: f64,
    #[props(default = String::new())] unit: String,
    on_change: EventHandler<f64>,
    #[props(default = String::new())] hint: String,
    #[props(default = false)] disabled: bool,
    #[props(default = false)] row: bool,
) -> Element {
    let mut draft = use_signal(|| None::<String>);
    let shown = draft
        .read()
        .clone()
        .unwrap_or_else(|| format::fmt_step_value(value, step));
    let invalid = draft
        .read()
        .as_ref()
        .is_some_and(|t| format::parse_in_range(t, min, max).is_none());
    let message = if invalid {
        format!(
            "Érvényes tartomány: {} – {}{}",
            format::fmt_step_value(min, step),
            format::fmt_step_value(max, step),
            if unit.is_empty() {
                String::new()
            } else {
                format!(" {unit}")
            }
        )
    } else {
        hint.clone()
    };
    let class = format!(
        "field{}{}{}",
        if invalid { " is-invalid" } else { "" },
        if row { " field--row" } else { "" },
        if disabled { " field--disabled" } else { "" }
    );
    // value the −/+ buttons start from: what is typed (if valid) or the model value
    let base = draft
        .read()
        .as_ref()
        .and_then(|t| format::parse_in_range(t, min, max))
        .unwrap_or(value);

    rsx! {
        div { class,
            label { class: "field__label", "{label}" }
            div { class: "field__control",
                input {
                    r#type: "number",
                    min: "{min}",
                    max: "{max}",
                    step: "{step}",
                    value: "{shown}",
                    disabled,
                    "aria-label": "{label}",
                    "aria-invalid": if invalid { "true" } else { "false" },
                    oninput: move |e| draft.set(Some(e.value())),
                    onchange: move |e| {
                        if let Some(v) = format::parse_in_range(&e.value(), min, max) {
                            on_change.call(v);
                        }
                        draft.set(None);
                    },
                    onblur: move |_| draft.set(None),
                }
                if !unit.is_empty() {
                    span { class: "field__unit", "{unit}" }
                }
                span { class: "field__steps",
                    button {
                        class: "field__step",
                        r#type: "button",
                        tabindex: "-1",
                        "aria-label": "Csökkentés",
                        disabled,
                        onclick: move |_| {
                            on_change.call(format::snap_to_step(base - step, min, max, step));
                            draft.set(None);
                        },
                        Icon { name: IconName::Minus, size: 14 }
                    }
                    button {
                        class: "field__step",
                        r#type: "button",
                        tabindex: "-1",
                        "aria-label": "Növelés",
                        disabled,
                        onclick: move |_| {
                            on_change.call(format::snap_to_step(base + step, min, max, step));
                            draft.set(None);
                        },
                        Icon { name: IconName::Plus, size: 14 }
                    }
                }
            }
            if !message.is_empty() {
                span { class: "field__hint", role: if invalid { "alert" } else { "note" }, "{message}" }
            }
        }
    }
}

// ───────────────────────────── TextField ─────────────────────────────

/// Labelled single-line text input (`on_input` fires on every keystroke). `error` switches to the
/// error state and replaces the hint.
#[component]
pub fn TextField(
    label: String,
    value: String,
    on_input: EventHandler<String>,
    #[props(default = String::new())] placeholder: String,
    #[props(default = String::new())] hint: String,
    #[props(default = None)] error: Option<String>,
    #[props(default = false)] disabled: bool,
) -> Element {
    let invalid = error.is_some();
    let message = error.clone().unwrap_or(hint);
    rsx! {
        label { class: if invalid { "field is-invalid" } else { "field" },
            span { class: "field__label", "{label}" }
            input {
                r#type: "text",
                value: "{value}",
                placeholder: "{placeholder}",
                disabled,
                spellcheck: "false",
                autocomplete: "off",
                "aria-invalid": if invalid { "true" } else { "false" },
                oninput: move |e| on_input.call(e.value()),
            }
            if !message.is_empty() {
                span { class: "field__hint", role: if invalid { "alert" } else { "note" }, "{message}" }
            }
        }
    }
}

// ───────────────────────────── Toasts ─────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Warning,
    Error,
}

impl ToastKind {
    fn class(self) -> &'static str {
        match self {
            ToastKind::Info => "toast toast--info",
            ToastKind::Success => "toast toast--success",
            ToastKind::Warning => "toast toast--warning",
            ToastKind::Error => "toast toast--error",
        }
    }

    fn icon(self) -> IconName {
        match self {
            ToastKind::Info => IconName::Info,
            ToastKind::Success => IconName::Check,
            ToastKind::Warning | ToastKind::Error => IconName::Warning,
        }
    }

    /// Multiplier on the base auto-dismiss time: problems stay longer than confirmations.
    fn linger(self) -> u64 {
        match self {
            ToastKind::Info | ToastKind::Success => 1,
            ToastKind::Warning => 2,
            ToastKind::Error => 3,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Toast {
    pub id: u64,
    pub kind: ToastKind,
    pub text: String,
}

impl Toast {
    pub fn new(id: u64, kind: ToastKind, text: impl Into<String>) -> Self {
        Self {
            id,
            kind,
            text: text.into(),
        }
    }
}

/// Stack of toasts (bottom-left). The app owns the list; entries also dismiss themselves after
/// `auto_dismiss_ms` (×2 for warnings, ×3 for errors; paused while hovered; `0` = never) by
/// calling `on_dismiss`, which must remove the id from the list. Click or the × dismisses at once.
#[component]
pub fn ToastHost(
    toasts: ReadSignal<Vec<Toast>>,
    on_dismiss: EventHandler<u64>,
    #[props(default = 5000)] auto_dismiss_ms: u64,
) -> Element {
    rsx! {
        div { class: "toasts", role: "region", "aria-label": "Értesítések",
            for t in toasts.read().iter().cloned() {
                ToastItem { key: "{t.id}", toast: t, on_dismiss, auto_dismiss_ms }
            }
        }
    }
}

#[component]
fn ToastItem(toast: Toast, on_dismiss: EventHandler<u64>, auto_dismiss_ms: u64) -> Element {
    let mut hovered = use_signal(|| false);
    let id = toast.id;
    let total = auto_dismiss_ms.saturating_mul(toast.kind.linger());
    use_future(move || async move {
        // no timer without a tokio runtime (SSR previews, tests): the toast then stays until dismissed
        if total == 0 || tokio::runtime::Handle::try_current().is_err() {
            return;
        }
        let tick = 100u64;
        let mut left = total;
        while left > 0 {
            tokio::time::sleep(Duration::from_millis(tick)).await;
            if !*hovered.peek() {
                left = left.saturating_sub(tick);
            }
        }
        on_dismiss.call(id);
    });
    let live = if toast.kind == ToastKind::Error {
        "alert"
    } else {
        "status"
    };
    rsx! {
        div {
            class: toast.kind.class(),
            role: live,
            onmouseenter: move |_| hovered.set(true),
            onmouseleave: move |_| hovered.set(false),
            span { class: "toast__icon", Icon { name: toast.kind.icon(), size: 18 } }
            span { class: "toast__text", "{toast.text}" }
            button {
                class: "btn btn--ghost btn--icon btn--sm toast__close",
                r#type: "button",
                "aria-label": "Értesítés bezárása",
                onclick: move |_| on_dismiss.call(id),
                Icon { name: IconName::Close, size: 14 }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_document_wraps_css_and_theme() {
        let doc = html_document(Theme::Light, &[".x{color:red}"], "<p>hi</p>");
        assert!(doc.starts_with("<!doctype html>"));
        assert!(doc.contains("data-theme=\"light\""));
        let kit_at = doc.find("--bg-1").expect("kit css inlined");
        let extra_at = doc.find(".x{color:red}").expect("extra css inlined");
        assert!(kit_at < extra_at, "kit CSS must come first");
        assert!(doc.contains("<body><p>hi</p></body>"));
        assert_eq!(theme_attr(Theme::Dark), "dark");
    }

    #[test]
    fn dialog_scripts_only_embed_generated_ids() {
        let s = dialog_open_script("kit-dlg-7");
        assert!(s.contains("getElementById(\"kit-dlg-7\")"));
        assert!(s.contains("e.key!=='Tab'"));
        assert!(dialog_close_script("kit-dlg-7").contains("__kitDlgPrev"));
    }

    #[test]
    fn uids_are_unique() {
        let a = next_uid();
        let b = next_uid();
        assert_ne!(a, b);
    }

    // ───────────── embedded fonts ─────────────

    const FONTS_CSS: &str = include_str!("../assets/fonts.css");
    const KIT_CSS: &str = include_str!("../assets/kit.css");

    fn base64(data: &[u8]) -> String {
        const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in data.chunks(3) {
            let n = (u32::from(chunk[0]) << 16)
                | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
                | u32::from(*chunk.get(2).unwrap_or(&0));
            for i in 0..4 {
                if i <= chunk.len() {
                    out.push(char::from(T[((n >> (18 - 6 * i)) & 63) as usize]));
                } else {
                    out.push('=');
                }
            }
        }
        out
    }

    #[test]
    fn base64_helper_matches_known_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn fonts_css_embeds_exactly_the_woff2_files() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/fonts");
        let mut seen = 0;
        for entry in std::fs::read_dir(&dir).expect("assets/fonts exists") {
            let path = entry.expect("dir entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("woff2") {
                continue;
            }
            let bytes = std::fs::read(&path).expect("read font");
            assert_eq!(&bytes[..4], b"wOF2", "{path:?} is not WOFF2");
            assert!(
                bytes.len() < 40_000,
                "{path:?} is {} bytes: not subsetted?",
                bytes.len()
            );
            assert!(
                FONTS_CSS.contains(&base64(&bytes)),
                "fonts.css is stale for {path:?}: run node crates/ride-ui-kit/tools/gen_fonts_css.mjs"
            );
            seen += 1;
        }
        assert_eq!(seen, 6, "3 Barlow + 3 Barlow Condensed weights");
        assert_eq!(FONTS_CSS.matches("@font-face").count(), seen);
        assert!(FONTS_CSS.contains("font-family:\"Barlow Condensed\""));
        assert!(FONTS_CSS.contains("font-family:\"Barlow\""));
        assert!(
            !FONTS_CSS.contains("http://") && !FONTS_CSS.contains("https://url"),
            "fonts must be embedded, not fetched"
        );
        assert!(CSS.starts_with("/* Barlow"), "fonts come first in CSS");
        assert!(CSS.contains("--font-num"));
    }

    #[test]
    fn the_font_licence_ships_with_the_fonts() {
        let ofl = include_str!("../assets/fonts/OFL.txt");
        assert!(ofl.contains("SIL OPEN FONT LICENSE Version 1.1"));
        assert!(ofl.contains("The Barlow Project Authors"));
        let notice = include_str!("../../../NOTICE.md");
        assert!(
            notice.contains("Barlow"),
            "NOTICE.md must list the bundled fonts"
        );
    }

    // ───────────── the house rules, enforced on kit.css ─────────────

    /// `(selector, declarations)` of every rule (comments stripped; @media / @keyframes bodies are
    /// walked too, so keyframe steps appear as rules of their own).
    fn css_rules(css: &str) -> Vec<(String, Vec<(String, String)>)> {
        let mut clean = String::new();
        let mut rest = css;
        while let Some(i) = rest.find("/*") {
            clean.push_str(&rest[..i]);
            rest = match rest[i..].find("*/") {
                Some(j) => &rest[i + j + 2..],
                None => "",
            };
        }
        clean.push_str(rest);

        let mut rules = Vec::new();
        let mut selector = String::new();
        let mut body = String::new();
        let mut depth = 0usize;
        for c in clean.chars() {
            match c {
                '{' => {
                    if depth > 0 {
                        body.push(c);
                    }
                    depth += 1;
                }
                '}' => {
                    depth = depth.saturating_sub(1);
                    if depth > 0 {
                        body.push(c);
                    } else {
                        if body.contains('{') {
                            rules.extend(css_rules(&body));
                        } else {
                            let decls = body
                                .split(';')
                                .filter_map(|d| {
                                    let (k, v) = d.split_once(':')?;
                                    let k = k.trim();
                                    (!k.is_empty()).then(|| (k.to_string(), v.trim().to_string()))
                                })
                                .collect();
                            rules.push((selector.trim().to_string(), decls));
                        }
                        selector.clear();
                        body.clear();
                    }
                }
                _ if depth == 0 => selector.push(c),
                _ => body.push(c),
            }
        }
        rules
    }

    fn px_values(v: &str) -> Vec<f64> {
        let mut out = Vec::new();
        for tok in v.split(|c: char| c.is_whitespace() || c == '(' || c == ')' || c == ',') {
            if let Some(n) = tok.strip_suffix("px") {
                if let Ok(x) = n.parse::<f64>() {
                    out.push(x);
                }
            }
        }
        out
    }

    #[test]
    fn css_rule_parser_walks_nested_blocks() {
        let rules = css_rules("/* c */ a { x: 1px; y: 2 } @media (m) { b { z: 3px } } @keyframes k { from { o: 0 } to { o: 1 } }");
        let sels: Vec<&str> = rules.iter().map(|(s, _)| s.as_str()).collect();
        assert!(sels.contains(&"a") && sels.contains(&"b"), "{sels:?}");
        assert!(rules.iter().any(|(s, d)| s == "a" && d.len() == 2));
    }

    #[test]
    fn no_blur_glow_or_soft_shadow_anywhere() {
        assert!(!KIT_CSS.contains("blur("), "blur is banned");
        assert!(
            !KIT_CSS.contains("backdrop-filter"),
            "backdrop-filter is banned"
        );
        assert!(!KIT_CSS.contains("text-shadow"), "text-shadow is banned");
        assert!(!KIT_CSS.contains("drop-shadow"), "drop-shadow is banned");
        // every box-shadow layer is a hard offset (or a 0-blur spread ring): the 3rd length is 0
        for (sel, decls) in css_rules(KIT_CSS) {
            for (k, v) in decls {
                if k != "box-shadow" && k != "--shadow-2" && k != "--shadow-1" {
                    continue;
                }
                if v == "none" {
                    continue;
                }
                for layer in v.split(',') {
                    // drop colour functions, keep the lengths
                    let mut lengths = Vec::new();
                    let mut skip = 0i32;
                    for tok in layer.split_whitespace() {
                        if tok.contains('(') {
                            skip +=
                                tok.matches('(').count() as i32 - tok.matches(')').count() as i32;
                            continue;
                        }
                        if skip > 0 {
                            skip +=
                                tok.matches('(').count() as i32 - tok.matches(')').count() as i32;
                            continue;
                        }
                        if tok == "inset" || tok.starts_with('#') {
                            continue;
                        }
                        lengths.push(tok.to_string());
                    }
                    if lengths.len() >= 3 {
                        let blur = lengths[2].trim_end_matches("px");
                        assert!(
                            blur == "0",
                            "{sel}: soft shadow `{layer}` (blur {})",
                            lengths[2]
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn gradients_are_only_hard_stop_value_fills() {
        for line in KIT_CSS.lines() {
            if line.contains("gradient(") {
                assert!(
                    line.contains("/*hard*/"),
                    "decorative gradient not allowed: {line}"
                );
            }
        }
    }

    #[test]
    fn corners_are_square_except_state_badges() {
        let rules = css_rules(KIT_CSS);
        let root = rules
            .iter()
            .find(|(s, _)| s == ":root")
            .expect(":root block");
        for (k, v) in &root.1 {
            if k.starts_with("--radius-") && k != "--radius-pill" {
                for px in px_values(v) {
                    assert!(px <= 4.0, "{k}: {v} is rounder than 4px");
                }
            }
        }
        for (sel, decls) in &rules {
            for (k, v) in decls {
                if k == "border-radius" {
                    for px in px_values(v) {
                        assert!(px <= 4.0, "{sel}: border-radius {v}");
                    }
                    let ok = v.split_whitespace().all(|t| {
                        t.starts_with("var(--radius-")
                            || t.ends_with("px")
                            || t == "0"
                            || t == "50%"
                            || t == "inherit"
                    });
                    assert!(ok, "{sel}: unexpected border-radius `{v}`");
                    if v.contains("--radius-pill") {
                        assert!(
                            sel.contains(".badge") || sel.contains(".chip--"),
                            "{sel}: a pill outside the state badges"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn type_comes_from_the_scale_and_labels_are_sentence_case() {
        assert!(!KIT_CSS.contains("text-transform"), "no capitals-by-CSS");
        for (sel, decls) in css_rules(KIT_CSS) {
            for (k, v) in decls {
                if k == "letter-spacing" {
                    for em in v.split_whitespace() {
                        let n: f64 = em.trim_end_matches("em").parse().unwrap_or(0.0);
                        assert!(n.abs() <= 0.02, "{sel}: tracking {v}");
                    }
                }
                if k == "font" || k == "font-size" {
                    assert!(
                        px_values(&v).is_empty(),
                        "{sel}: literal px in `{k}: {v}`, use the --t-* scale"
                    );
                }
            }
        }
        let scale = [
            "--t-11: 11px",
            "--t-13: 13px",
            "--t-15: 15px",
            "--t-20: 20px",
            "--t-28: 28px",
            "--t-56: 56px",
            "--t-96: 96px",
        ];
        for s in scale {
            assert!(KIT_CSS.contains(s), "scale step {s}");
        }
        // numerals are tabular + lining
        let num = css_rules(KIT_CSS)
            .into_iter()
            .find(|(s, _)| s == ".num")
            .expect(".num");
        let feat = num
            .1
            .iter()
            .find(|(k, _)| k == "font-feature-settings")
            .map(|(_, v)| v.clone())
            .unwrap_or_default();
        assert!(feat.contains("\"tnum\" 1") && feat.contains("\"lnum\" 1"));
    }

    #[test]
    fn spacing_sits_on_the_four_pixel_grid() {
        // hairline adjustments (0 to 2 px) are allowed; everything else is a multiple of 4
        let props = [
            "padding",
            "padding-top",
            "padding-right",
            "padding-bottom",
            "padding-left",
            "margin",
            "margin-top",
            "margin-right",
            "margin-bottom",
            "margin-left",
            "gap",
            "row-gap",
            "column-gap",
            "height",
            "min-height",
            "width",
            "min-width",
        ];
        for (sel, decls) in css_rules(KIT_CSS) {
            // the native range thumb is centred on its 2 px track by an odd margin: documented exception
            if sel.contains("::-webkit-slider") {
                continue;
            }
            for (k, v) in decls {
                if !props.contains(&k.as_str()) {
                    continue;
                }
                for px in px_values(&v) {
                    let a = px.abs();
                    assert!(
                        a <= 2.0 || (a % 4.0).abs() < 1e-9,
                        "{sel}: `{k}: {v}` leaves the 4 px grid"
                    );
                }
            }
        }
    }

    #[test]
    fn dialog_and_toast_motion_is_150ms() {
        let root = css_rules(KIT_CSS)
            .into_iter()
            .find(|(s, _)| s == ":root")
            .expect(":root");
        let dur = root
            .1
            .iter()
            .find(|(k, _)| k == "--dur")
            .map(|(_, v)| v.clone());
        assert_eq!(dur.as_deref(), Some("150ms"));
        for (sel, decls) in css_rules(KIT_CSS) {
            if sel == ".dialog" || sel == ".dialog-backdrop" || sel == ".toast" {
                let a = decls
                    .iter()
                    .find(|(k, _)| k == "animation")
                    .expect("animation");
                assert!(a.1.contains("var(--dur)"), "{sel}: {}", a.1);
            }
        }
    }

    #[test]
    fn every_primitive_of_the_brief_is_styled() {
        for class in [
            ".btn",
            ".btn--primary",
            ".btn--ghost",
            ".btn--icon",
            ".btn-group",
            ".input",
            ".field",
            ".tag",
            ".chip",
            ".badge",
            ".cells",
            ".cell",
            ".cell__label",
            ".cell__value",
            ".cell__unit",
            ".tabs",
            ".tab",
            ".seg",
            ".seg__item",
            ".switch",
            ".slider",
            ".dialog",
            ".dialog-backdrop",
            ".toast",
            ".toasts",
            ".kbd",
            ".zonebar",
            ".zonebar__seg",
            ".zonebar__ftp",
            ".panel",
            ".num",
            ".unit",
            ".progress",
        ] {
            assert!(
                css_rules(KIT_CSS)
                    .iter()
                    .any(|(s, _)| s.split(',').any(|p| p.trim().starts_with(class))),
                "{class} is not styled in kit.css"
            );
        }
    }

    #[test]
    fn segmented_renders_as_tabs_when_asked() {
        fn app() -> Element {
            rsx! {
                Tabs {
                    options: vec![("a".to_string(), "Egy".to_string()), ("b".to_string(), "Kettő".to_string())],
                    value: "b".to_string(),
                    on_change: move |_| {},
                    aria_label: "Nézet".to_string(),
                }
                Segmented {
                    options: vec![("a".to_string(), "Egy".to_string()), ("b".to_string(), "Kettő".to_string())],
                    value: "a".to_string(),
                    on_change: move |_| {},
                }
            }
        }
        let mut vdom = VirtualDom::new(app);
        vdom.rebuild_in_place();
        let html = dioxus_ssr::render(&vdom);
        assert!(
            html.contains("role=\"tablist\"") && html.contains("class=\"tab is-active\""),
            "{html}"
        );
        assert!(html.contains("aria-selected=\"true\""), "{html}");
        assert!(
            html.contains("role=\"radiogroup\"") && html.contains("seg__item is-active"),
            "{html}"
        );
    }

    #[test]
    fn toast_linger_orders_by_severity() {
        assert!(ToastKind::Error.linger() > ToastKind::Warning.linger());
        assert!(ToastKind::Warning.linger() > ToastKind::Info.linger());
        let t = Toast::new(3, ToastKind::Success, "Kész");
        assert_eq!(t.id, 3);
        assert_eq!(t.text, "Kész");
    }
}
