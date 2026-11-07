//! Procedurally drawn window icon (dark rounded square with an accent-coloured ridge
//! line), so the exe needs no image asset and no resource compiler on the GNU toolchain.

use dioxus::desktop::tao::window::Icon;

const BG_TOP: [f32; 3] = [27.0, 33.0, 46.0];
const BG_BOTTOM: [f32; 3] = [11.0, 14.0, 20.0];
const ACCENT: [f32; 3] = [77.0, 224.0, 192.0];
/// Ridge silhouette as (u, v) in the unit square; the filled area lies below the line.
const RIDGE: [(f32, f32); 5] = [
    (0.14, 0.74),
    (0.38, 0.34),
    (0.52, 0.54),
    (0.64, 0.40),
    (0.86, 0.74),
];
const BASE_V: f32 = 0.74;
/// Corner radius of the rounded square, as a share of the side.
const RADIUS: f32 = 0.22;

pub fn window_icon() -> Option<Icon> {
    const SIZE: u32 = 64;
    Icon::from_rgba(render_rgba(SIZE), SIZE, SIZE).ok()
}

fn ridge_height(u: f32) -> Option<f32> {
    RIDGE.windows(2).find_map(|w| {
        let ((u0, v0), (u1, v1)) = (w[0], w[1]);
        (u >= u0 && u <= u1).then(|| v0 + (v1 - v0) * (u - u0) / (u1 - u0))
    })
}

/// `size`×`size` straight-alpha RGBA bytes.
pub fn render_rgba(size: u32) -> Vec<u8> {
    let s = size as f32;
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let (u, v) = ((x as f32 + 0.5) / s, (y as f32 + 0.5) / s);

            // Rounded-square coverage (signed distance, 1 px anti-aliasing).
            let radius = RADIUS;
            let (dx, dy) = (
                (u - 0.5).abs() - (0.5 - radius),
                (v - 0.5).abs() - (0.5 - radius),
            );
            let sdf =
                (dx.max(0.0).powi(2) + dy.max(0.0).powi(2)).sqrt() + dx.max(dy).min(0.0) - radius;
            let alpha = (0.5 - sdf * s).clamp(0.0, 1.0);

            let bg: Vec<f32> = (0..3)
                .map(|c| BG_TOP[c] + (BG_BOTTOM[c] - BG_TOP[c]) * v)
                .collect();
            let cover = ridge_height(u)
                .map(|top| {
                    ((v - top) * s + 0.5).clamp(0.0, 1.0) * ((BASE_V - v) * s + 0.5).clamp(0.0, 1.0)
                })
                .unwrap_or(0.0);
            for c in 0..3 {
                out.push(
                    (bg[c] * (1.0 - cover) + ACCENT[c] * cover)
                        .round()
                        .clamp(0.0, 255.0) as u8,
                );
            }
            out.push((alpha * 255.0).round() as u8);
        }
    }
    out
}

/// The same icon as an inline SVG in a 100 x 100 view box, for the logo of the top bar: drawn
/// from the constants above, so the bar, the window, the packages and the APK show one picture.
pub fn svg_markup() -> String {
    let rgb = |c: [f32; 3]| format!("rgb({},{},{})", c[0], c[1], c[2]);
    // a tenth of a unit is plenty, and it keeps f32 noise (14.000001) out of the markup
    let unit = |share: f32| (share * 1000.0).round() / 10.0;
    let ridge: Vec<String> = RIDGE
        .iter()
        .map(|(u, v)| format!("{},{}", unit(*u), unit(*v)))
        .collect();
    format!(
        "<svg viewBox=\"0 0 100 100\" width=\"100%\" height=\"100%\" aria-hidden=\"true\" focusable=\"false\">\
         <defs><linearGradient id=\"app-mark-bg\" x1=\"0\" y1=\"0\" x2=\"0\" y2=\"1\">\
         <stop offset=\"0\" stop-color=\"{top}\"/><stop offset=\"1\" stop-color=\"{bottom}\"/>\
         </linearGradient></defs>\
         <rect width=\"100\" height=\"100\" rx=\"{r}\" fill=\"url(#app-mark-bg)\"/>\
         <polygon points=\"{ridge}\" fill=\"{accent}\"/></svg>",
        top = rgb(BG_TOP),
        bottom = rgb(BG_BOTTOM),
        r = unit(RADIUS),
        ridge = ridge.join(" "),
        accent = rgb(ACCENT),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_the_right_size_and_shape() {
        let px = render_rgba(64);
        assert_eq!(px.len(), 64 * 64 * 4);
        let alpha = |x: usize, y: usize| px[(y * 64 + x) * 4 + 3];
        assert_eq!(alpha(0, 0), 0, "corner is outside the rounded square");
        assert_eq!(alpha(32, 32), 255, "centre is opaque");
    }

    #[test]
    fn ridge_is_drawn_in_accent_colour() {
        let px = render_rgba(64);
        let i = (42 * 64 + 32) * 4; // below the ridge line (v ~ 0.52), above the base line (v = 0.74)
        assert!(
            px[i + 1] > 150 && px[i] < 120,
            "expected a teal pixel, got {:?}",
            &px[i..i + 3]
        );
    }

    #[test]
    fn svg_mark_draws_the_window_icon_art() {
        let svg = svg_markup();
        assert!(svg.starts_with("<svg") && svg.ends_with("</svg>"));
        assert!(svg.contains("rx=\"22\""), "same corner radius: {svg}");
        assert!(
            svg.contains("fill=\"rgb(77,224,192)\""),
            "accent ridge: {svg}"
        );
        assert!(
            svg.contains("points=\"14,74 38,34 52,54 64,40 86,74\""),
            "same ridge: {svg}"
        );
    }

    #[test]
    fn builds_a_window_icon() {
        assert!(window_icon().is_some());
    }
}
