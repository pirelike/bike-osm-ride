//! Inline SVG icon set, drawn for the "bike head unit" language: a 24x24 grid with a 20x20 live
//! area, a 2 px stroke with square caps and mitred corners, straight segments wherever the object
//! has a straight edge (round shapes stay circles and arcs), `currentColor`. Transport glyphs
//! (play, pause, stop) are solid shapes without an outline. Coordinates sit on whole pixels so that the 2 px strokes
//! land on the pixel grid at 24 px. All artwork is original (simple geometric primitives), no
//! third-party icon font or set is used.

/// Stroke width of every icon, in the 24x24 viewBox.
pub const ICON_STROKE_WIDTH: &str = "2";
/// Line cap shared by every icon (square ends read as printed rules, not as a rounded app icon).
pub const ICON_LINECAP: &str = "square";
/// Line join shared by every icon.
pub const ICON_LINEJOIN: &str = "miter";

/// Every icon of the kit. `IconName::ALL` lists them in declaration order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IconName {
    Play,
    Pause,
    Stop,
    Reset,
    Flag,
    Folder,
    Link,
    Bluetooth,
    Heart,
    Bolt,
    Cadence,
    Gauge,
    Mountain,
    Settings,
    Sun,
    Moon,
    Close,
    Check,
    Plus,
    Minus,
    Download,
    Upload,
    Layers,
    Locate,
    Clock,
    Ruler,
    Route,
    Trophy,
    Warning,
    Search,
    Trash,
    Info,
    Fullscreen,
    ChevronLeft,
    ChevronRight,
    ChevronUp,
    ChevronDown,
    Bike,
    Flame,
    Compass,
    Refresh,
    Sliders,
}

impl IconName {
    pub const ALL: [IconName; 42] = [
        IconName::Play,
        IconName::Pause,
        IconName::Stop,
        IconName::Reset,
        IconName::Flag,
        IconName::Folder,
        IconName::Link,
        IconName::Bluetooth,
        IconName::Heart,
        IconName::Bolt,
        IconName::Cadence,
        IconName::Gauge,
        IconName::Mountain,
        IconName::Settings,
        IconName::Sun,
        IconName::Moon,
        IconName::Close,
        IconName::Check,
        IconName::Plus,
        IconName::Minus,
        IconName::Download,
        IconName::Upload,
        IconName::Layers,
        IconName::Locate,
        IconName::Clock,
        IconName::Ruler,
        IconName::Route,
        IconName::Trophy,
        IconName::Warning,
        IconName::Search,
        IconName::Trash,
        IconName::Info,
        IconName::Fullscreen,
        IconName::ChevronLeft,
        IconName::ChevronRight,
        IconName::ChevronUp,
        IconName::ChevronDown,
        IconName::Bike,
        IconName::Flame,
        IconName::Compass,
        IconName::Refresh,
        IconName::Sliders,
    ];

    /// Identifier as written in the source (`"ChevronLeft"`), used for the showcase captions.
    pub fn name(self) -> &'static str {
        match self {
            IconName::Play => "Play",
            IconName::Pause => "Pause",
            IconName::Stop => "Stop",
            IconName::Reset => "Reset",
            IconName::Flag => "Flag",
            IconName::Folder => "Folder",
            IconName::Link => "Link",
            IconName::Bluetooth => "Bluetooth",
            IconName::Heart => "Heart",
            IconName::Bolt => "Bolt",
            IconName::Cadence => "Cadence",
            IconName::Gauge => "Gauge",
            IconName::Mountain => "Mountain",
            IconName::Settings => "Settings",
            IconName::Sun => "Sun",
            IconName::Moon => "Moon",
            IconName::Close => "Close",
            IconName::Check => "Check",
            IconName::Plus => "Plus",
            IconName::Minus => "Minus",
            IconName::Download => "Download",
            IconName::Upload => "Upload",
            IconName::Layers => "Layers",
            IconName::Locate => "Locate",
            IconName::Clock => "Clock",
            IconName::Ruler => "Ruler",
            IconName::Route => "Route",
            IconName::Trophy => "Trophy",
            IconName::Warning => "Warning",
            IconName::Search => "Search",
            IconName::Trash => "Trash",
            IconName::Info => "Info",
            IconName::Fullscreen => "Fullscreen",
            IconName::ChevronLeft => "ChevronLeft",
            IconName::ChevronRight => "ChevronRight",
            IconName::ChevronUp => "ChevronUp",
            IconName::ChevronDown => "ChevronDown",
            IconName::Bike => "Bike",
            IconName::Flame => "Flame",
            IconName::Compass => "Compass",
            IconName::Refresh => "Refresh",
            IconName::Sliders => "Sliders",
        }
    }

    /// `true` for solid glyphs (transport controls): drawn with `fill="currentColor"`.
    pub fn is_filled(self) -> bool {
        matches!(self, IconName::Play | IconName::Pause | IconName::Stop)
    }
}

/// Inner SVG markup (paths/circles) of an icon, 24x24 coordinates, no `<svg>` wrapper.
pub fn icon_inner(name: IconName) -> &'static str {
    match name {
        IconName::Play => r#"<path d="M6 3v18l15-9z"/>"#,
        IconName::Pause => r#"<path d="M5 4h5v16H5zM14 4h5v16h-5z"/>"#,
        IconName::Stop => r#"<path d="M5 5h14v14H5z"/>"#,
        IconName::Reset => r#"<path d="M4 12a8 8 0 1 0 2.5-5.8L4 9"/><path d="M4 3.5V9h5.5"/>"#,
        IconName::Flag => r#"<path d="M5 21V3"/><path d="M5 4h14l-3.5 4.5L19 13H5"/>"#,
        IconName::Folder => r#"<path d="M3 5h7l2 3h9v11H3z"/>"#,
        IconName::Link => {
            r#"<path d="M9 7H6a5 5 0 0 0 0 10h3"/><path d="M15 7h3a5 5 0 0 1 0 10h-3"/><path d="M8 12h8"/>"#
        }
        IconName::Bluetooth => r#"<path d="M6 7.5l11 9-5.5 4.5V3L17 7.5l-11 9"/>"#,
        IconName::Heart => {
            r#"<path d="M12 20L4.6 12.4A4.6 4.6 0 0 1 12 6.6a4.6 4.6 0 0 1 7.4 5.8z"/>"#
        }
        IconName::Bolt => r#"<path d="M13 2L5 14h6l-1 8 9-12h-6z"/>"#,
        IconName::Cadence => {
            r#"<circle cx="12" cy="12" r="9"/><circle cx="12" cy="12" r="1.5"/><path d="M13 11l5-5"/><path d="M15.5 5l3.5 3.5"/>"#
        }
        IconName::Gauge => {
            r#"<path d="M3 18a9 9 0 1 1 18 0"/><path d="M12 18l4.5-6.5"/><path d="M3 18h3M18 18h3"/>"#
        }
        IconName::Mountain => r#"<path d="M2 20L9 7l4 7 3-4 6 10z"/>"#,
        IconName::Settings => {
            r#"<path d="M10.1 4.9L10.4 2.1h3.2l.3 2.8 1.8.7 2.2-1.7 2.2 2.2-1.7 2.2.7 1.8 2.8.3v3.2l-2.8.3-.7 1.8 1.7 2.2-2.2 2.2-2.2-1.7-1.8.7-.3 2.8h-3.2l-.3-2.8-1.8-.7-2.2 1.7-2.2-2.2 1.7-2.2-.7-1.8-2.8-.3v-3.2l2.8-.3.7-1.8-1.7-2.2 2.2-2.2 2.2 1.7z"/><circle cx="12" cy="12" r="3"/>"#
        }
        IconName::Sun => {
            r#"<circle cx="12" cy="12" r="4"/><path d="M12 2v3M12 19v3M2 12h3M19 12h3M5 5l2 2M17 17l2 2M5 19l2-2M17 7l2-2"/>"#
        }
        IconName::Moon => r#"<path d="M20 14.5A8.5 8.5 0 0 1 9.5 4a8.5 8.5 0 1 0 10.5 10.5z"/>"#,
        IconName::Close => r#"<path d="M5 5l14 14M19 5L5 19"/>"#,
        IconName::Check => r#"<path d="M4 12.5l5 5L20 6.5"/>"#,
        IconName::Plus => r#"<path d="M12 4v16M4 12h16"/>"#,
        IconName::Minus => r#"<path d="M4 12h16"/>"#,
        IconName::Download => r#"<path d="M12 3v13M6 11l6 6 6-6M4 21h16"/>"#,
        IconName::Upload => r#"<path d="M12 17V4M6 10l6-6 6 6M4 21h16"/>"#,
        IconName::Layers => {
            r#"<path d="M12 3l9 4.5-9 4.5-9-4.5z"/><path d="M3 12l9 4.5 9-4.5"/><path d="M3 16.5l9 4.5 9-4.5"/>"#
        }
        IconName::Locate => {
            r#"<circle cx="12" cy="12" r="6"/><circle cx="12" cy="12" r="1.5"/><path d="M12 2v5M12 17v5M2 12h5M17 12h5"/>"#
        }
        IconName::Clock => r#"<circle cx="12" cy="12" r="9"/><path d="M12 6v6.5l4 2.5"/>"#,
        IconName::Ruler => {
            r#"<path d="M3 17L17 3l4 4L7 21z"/><path d="M7 13l3 3M10 10l2 2M13 7l3 3"/>"#
        }
        IconName::Route => {
            r#"<circle cx="6" cy="19" r="2.5"/><circle cx="18" cy="5" r="2.5"/><path d="M8.5 19H19v-7H5V5h10.5"/>"#
        }
        IconName::Trophy => {
            r#"<path d="M7 3h10v7a5 5 0 0 1-10 0z"/><path d="M7 5H3v2a4 4 0 0 0 4 4M17 5h4v2a4 4 0 0 1-4 4"/><path d="M12 15v4M8 21h8"/>"#
        }
        IconName::Warning => {
            r#"<path d="M12 3l10 17H2z"/><path d="M12 9.5v5"/><path d="M12 17.5h.01"/>"#
        }
        IconName::Search => r#"<circle cx="10" cy="10" r="6.5"/><path d="M15 15l6 6"/>"#,
        IconName::Trash => {
            r#"<path d="M4 6h16"/><path d="M9 6V3h6v3"/><path d="M6 6l1 15h10l1-15"/><path d="M10 10v7M14 10v7"/>"#
        }
        IconName::Info => {
            r#"<circle cx="12" cy="12" r="9"/><path d="M12 11v6"/><path d="M12 7.5h.01"/>"#
        }
        IconName::Fullscreen => r#"<path d="M4 9V4h5M15 4h5v5M20 15v5h-5M9 20H4v-5"/>"#,
        IconName::ChevronLeft => r#"<path d="M14 6l-6 6 6 6"/>"#,
        IconName::ChevronRight => r#"<path d="M10 6l6 6-6 6"/>"#,
        IconName::ChevronUp => r#"<path d="M6 15l6-6 6 6"/>"#,
        IconName::ChevronDown => r#"<path d="M6 9l6 6 6-6"/>"#,
        IconName::Bike => {
            r#"<circle cx="5.5" cy="16.5" r="3.5"/><circle cx="18.5" cy="16.5" r="3.5"/><path d="M5.5 16.5L9 9h6.5l3 7.5"/><path d="M9 9l3 7.5L15.5 9"/><path d="M7 6.5h4M14.5 6h3"/>"#
        }
        IconName::Flame => {
            r#"<path d="M12 2c0 4 6 6 6 12a6 6 0 0 1-12 0c0-2.5 1-4 2.5-5.5.5 1.5 1.5 2.5 2.5 3C10.5 8.5 11 5 12 2z"/>"#
        }
        IconName::Compass => {
            r#"<circle cx="12" cy="12" r="9"/><path d="M16 8l-2.5 5.5L8 16l2.5-5.5z"/>"#
        }
        IconName::Refresh => {
            r#"<path d="M20 11a8 8 0 0 0-14-4L4 9"/><path d="M4 4v5h5"/><path d="M4 13a8 8 0 0 0 14 4l2-2"/><path d="M20 20v-5h-5"/>"#
        }
        IconName::Sliders => {
            r#"<path d="M4 7h7M17 7h3M4 12h3M13 12h7M4 17h9M19 17h1"/><path d="M14 4v6M10 9v6M16 14v6"/>"#
        }
    }
}

/// Complete `<svg>` markup for an icon at `size` px (for places that need a string, e.g. map
/// markers built as HTML). `aria-hidden` because icons are decorative; buttons carry the label.
pub fn icon_svg(name: IconName, size: u32) -> String {
    // solid glyphs carry no outline, line icons carry no fill
    let (fill, stroke) = if name.is_filled() {
        ("currentColor", "none")
    } else {
        ("none", "currentColor")
    };
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\" width=\"{size}\" height=\"{size}\" fill=\"{fill}\" stroke=\"{stroke}\" stroke-width=\"{ICON_STROKE_WIDTH}\" stroke-linecap=\"{ICON_LINECAP}\" stroke-linejoin=\"{ICON_LINEJOIN}\" aria-hidden=\"true\">{}</svg>",
        icon_inner(name)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_lists_every_variant_once_with_unique_names() {
        let mut names: Vec<&str> = IconName::ALL.iter().map(|n| n.name()).collect();
        let total = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), total, "duplicate icon in ALL");
        assert_eq!(total, 42);
    }

    #[test]
    fn every_icon_has_well_formed_markup() {
        for n in IconName::ALL {
            let inner = icon_inner(n);
            assert!(!inner.is_empty(), "{n:?} empty");
            // balanced tags: every element is self-closing in our set
            let opens = inner.matches('<').count();
            let closes = inner.matches("/>").count();
            assert_eq!(opens, closes, "{n:?}: unbalanced markup {inner}");
            assert!(
                !inner.contains("script"),
                "{n:?}: suspicious content {inner}"
            );
            let svg = icon_svg(n, 20);
            assert!(svg.starts_with("<svg") && svg.ends_with("</svg>"));
            assert!(svg.contains("viewBox=\"0 0 24 24\"") && svg.contains("stroke-width=\"2\""));
        }
    }

    #[test]
    fn icons_use_the_square_head_unit_language() {
        // square caps + mitred joins everywhere, no rounded rectangles, nothing styled per icon
        let svg = icon_svg(IconName::Bike, 24);
        assert!(
            svg.contains("stroke-linecap=\"square\"") && svg.contains("stroke-linejoin=\"miter\"")
        );
        assert!(!svg.contains("round"));
        for n in IconName::ALL {
            let inner = icon_inner(n);
            assert!(!inner.contains("rx="), "{n:?}: rounded rect");
            assert!(!inner.contains("stroke"), "{n:?}: per-icon stroke styling");
            assert!(!inner.contains("fill="), "{n:?}: per-icon fill styling");
        }
    }

    /// Every number in every path / shape must lie inside the 24x24 viewBox (so an icon can never
    /// be clipped by its own svg element), checking absolute and relative commands alike.
    #[test]
    fn icon_geometry_stays_inside_the_view_box() {
        for n in IconName::ALL {
            let inner = icon_inner(n);
            for attr in ["cx=\"", "cy=\"", "x=\"", "y=\""] {
                for part in inner.split(attr).skip(1) {
                    let v: f64 = part
                        .split('"')
                        .next()
                        .and_then(|t| t.parse().ok())
                        .unwrap_or(f64::NAN);
                    assert!((0.0..=24.0).contains(&v), "{n:?}: {attr}{v}");
                }
            }
            // absolute move/line targets in paths: walk the d attribute with a tiny interpreter
            for d in inner.split("d=\"").skip(1) {
                let d = d.split('"').next().unwrap_or("");
                for (x, y) in absolute_points(d) {
                    assert!(
                        (-0.5..=24.5).contains(&x) && (-0.5..=24.5).contains(&y),
                        "{n:?}: point ({x}, {y}) outside the grid in {d}"
                    );
                }
            }
        }
    }

    /// Absolute end points of the path commands (relative commands are accumulated).
    fn absolute_points(d: &str) -> Vec<(f64, f64)> {
        let mut out = Vec::new();
        let (mut x, mut y) = (0.0f64, 0.0f64);
        let (mut sx, mut sy) = (0.0f64, 0.0f64);
        let chars = d.chars();
        let mut cmd = 'M';
        let mut nums: Vec<f64> = Vec::new();
        let flush = |cmd: char,
                     nums: &mut Vec<f64>,
                     x: &mut f64,
                     y: &mut f64,
                     sx: &mut f64,
                     sy: &mut f64,
                     out: &mut Vec<(f64, f64)>| {
            let rel = cmd.is_ascii_lowercase();
            let arity = match cmd.to_ascii_uppercase() {
                'M' | 'L' | 'T' => 2,
                'H' | 'V' => 1,
                'C' => 6,
                'S' | 'Q' => 4,
                'A' => 7,
                _ => 0,
            };
            if arity == 0 {
                if cmd.eq_ignore_ascii_case(&'z') {
                    *x = *sx;
                    *y = *sy;
                }
                nums.clear();
                return;
            }
            for chunk in nums.chunks(arity).filter(|c| c.len() == arity) {
                let (nx, ny) = match cmd.to_ascii_uppercase() {
                    'H' => (if rel { *x + chunk[0] } else { chunk[0] }, *y),
                    'V' => (*x, if rel { *y + chunk[0] } else { chunk[0] }),
                    _ => {
                        let (ex, ey) = (chunk[arity - 2], chunk[arity - 1]);
                        (
                            if rel { *x + ex } else { ex },
                            if rel { *y + ey } else { ey },
                        )
                    }
                };
                *x = nx;
                *y = ny;
                if cmd.eq_ignore_ascii_case(&'m') {
                    *sx = nx;
                    *sy = ny;
                }
                out.push((nx, ny));
            }
            nums.clear();
        };
        let mut token = String::new();
        let push_token = |token: &mut String, nums: &mut Vec<f64>| {
            if !token.is_empty() {
                if let Ok(v) = token.parse::<f64>() {
                    nums.push(v);
                }
                token.clear();
            }
        };
        for c in chars {
            if c.is_ascii_alphabetic() {
                push_token(&mut token, &mut nums);
                flush(cmd, &mut nums, &mut x, &mut y, &mut sx, &mut sy, &mut out);
                cmd = c;
                if c.eq_ignore_ascii_case(&'z') {
                    flush(cmd, &mut nums, &mut x, &mut y, &mut sx, &mut sy, &mut out);
                }
            } else if c == '-' {
                push_token(&mut token, &mut nums);
                token.push(c);
            } else if c == '.' {
                // a second '.' starts a new number ("1.5.5" = 1.5, .5)
                if token.contains('.') {
                    push_token(&mut token, &mut nums);
                }
                token.push(c);
            } else if c.is_ascii_digit() {
                token.push(c);
            } else {
                push_token(&mut token, &mut nums);
            }
        }
        push_token(&mut token, &mut nums);
        flush(cmd, &mut nums, &mut x, &mut y, &mut sx, &mut sy, &mut out);
        out
    }

    #[test]
    fn path_walker_handles_relative_and_implicit_commands() {
        let pts = absolute_points("M2 3h4v5l-1 1zM10 10");
        assert_eq!(
            pts,
            vec![(2.0, 3.0), (6.0, 3.0), (6.0, 8.0), (5.0, 9.0), (10.0, 10.0)]
        );
    }

    #[test]
    fn only_transport_glyphs_are_filled() {
        let filled: Vec<_> = IconName::ALL.iter().filter(|n| n.is_filled()).collect();
        assert_eq!(filled.len(), 3);
        assert!(icon_svg(IconName::Play, 24).contains("fill=\"currentColor\""));
        assert!(icon_svg(IconName::Bolt, 24).contains("fill=\"none\""));
    }
}
