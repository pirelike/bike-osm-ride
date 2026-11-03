//! The tiled ride screen: which panels exist, how the workspace is divided between them, and the
//! arithmetic of dragging dividers and moving panels. Pure data and maths, no UI; `shell.rs`
//! draws it.
//!
//! The layout is a binary tree, like a tiling window manager: a leaf is a panel, a split divides
//! its area between two subtrees at a `ratio`. Panels never overlap and never leave a gap: closing
//! one gives its space to its neighbour, a panel dropped on the edge of another splits that one in
//! two, and dragging the divider between two panels resizes both. The workspace is a grid of
//! [`COLS`] x [`ROWS`] cells that scales with the window; dividers snap to whole cells.
//!
//! Panels that cannot be shown right now (the workout card without a chosen workout, the
//! simulated-rider panel without a simulated rider) stay in the tree but take no space.

use ride_model::TileNode;

/// Cells per step of the original 24 x 16 grid. Dividers snap to whole cells, so this decides how
/// finely the panels can be sized (a quarter of the old step: about 20 px on a 1920 px screen).
pub const FINE: f64 = 4.0;

/// Grid size of the workspace, in cells.
pub const COLS: f64 = 24.0 * FINE;
pub const ROWS: f64 = 16.0 * FINE;

/// Pointer travel (px) below which a press on a panel or chip counts as a click, not a drag.
pub const CLICK_SLOP: f64 = 4.0;

/// Deepest split tree accepted from a settings file.
const MAX_DEPTH: usize = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PanelId {
    Map,
    Profile,
    Controls,
    Metrics,
    Climb,
    Stats,
    Charts,
    Workout,
    Rider,
    Gears,
    Browser,
}

impl PanelId {
    pub const ALL: [PanelId; 11] = [
        PanelId::Map,
        PanelId::Profile,
        PanelId::Controls,
        PanelId::Metrics,
        PanelId::Climb,
        PanelId::Stats,
        PanelId::Charts,
        PanelId::Workout,
        PanelId::Rider,
        PanelId::Gears,
        PanelId::Browser,
    ];

    /// Stable key, used in the settings file and in `data-panel`.
    pub fn key(self) -> &'static str {
        match self {
            PanelId::Map => "map",
            PanelId::Profile => "profile",
            PanelId::Controls => "controls",
            PanelId::Metrics => "metrics",
            PanelId::Climb => "climb",
            PanelId::Stats => "stats",
            PanelId::Charts => "charts",
            PanelId::Workout => "workout",
            PanelId::Rider => "rider",
            PanelId::Gears => "gears",
            PanelId::Browser => "browser",
        }
    }

    pub fn from_key(key: &str) -> Option<PanelId> {
        PanelId::ALL.into_iter().find(|id| id.key() == key)
    }

    /// Title bar text and the label of the chip in the top bar.
    pub fn title(self) -> &'static str {
        match self {
            PanelId::Map => "Térkép",
            PanelId::Profile => "Magassági profil",
            PanelId::Controls => "Vezérlés",
            PanelId::Metrics => "Teljesítmény",
            PanelId::Climb => "Következő emelkedő",
            PanelId::Stats => "Statisztika",
            PanelId::Charts => "Grafikonok",
            PanelId::Workout => "Edzés",
            PanelId::Rider => "Szimulált kerékpáros",
            PanelId::Gears => "Váltó",
            PanelId::Browser => "Böngésző",
        }
    }

    /// Smallest (width, height) in cells at which the panel's content is still usable; dividers
    /// stop there.
    pub fn min_size(self) -> (f64, f64) {
        // in steps of the old 24 x 16 grid, i.e. fractions of the workspace
        let (w, h) = match self {
            PanelId::Map => (5.0, 3.0),
            PanelId::Profile => (5.0, 2.5),
            PanelId::Controls => (2.5, 1.75),
            PanelId::Metrics => (4.5, 4.5),
            PanelId::Climb => (3.5, 2.5),
            PanelId::Stats => (4.5, 2.5),
            PanelId::Charts => (4.5, 3.5),
            PanelId::Workout => (4.5, 3.5),
            PanelId::Rider => (4.5, 3.5),
            PanelId::Gears => (4.0, 1.75),
            PanelId::Browser => (12.0, 8.0),
        };
        (w * FINE, h * FINE)
    }

    /// The content fills the panel body and scrolls (or not) by itself.
    pub fn fills(self) -> bool {
        matches!(self, PanelId::Map | PanelId::Profile | PanelId::Browser)
    }
}

/// Which of the conditional panels can be shown right now.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Availability {
    /// A workout is chosen.
    pub workout: bool,
    /// The simulated rider is connected.
    pub rider: bool,
    /// The virtual gearbox is switched on.
    pub gears: bool,
}

impl Availability {
    pub fn has(self, id: PanelId) -> bool {
        match id {
            PanelId::Workout => self.workout,
            PanelId::Rider => self.rider,
            PanelId::Gears => self.gears,
            // The browser panel embeds the installed Firefox as a Win32 child window, so it only
            // exists on Windows; elsewhere it is not offered (no tile, no chip, no compact tab).
            PanelId::Browser => cfg!(windows),
            _ => true,
        }
    }
}

/// A rectangle in (fractional) grid cells.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub const GRID: Rect = Rect {
        x: 0.0,
        y: 0.0,
        w: COLS,
        h: ROWS,
    };

    pub fn right(self) -> f64 {
        self.x + self.w
    }

    pub fn bottom(self) -> f64 {
        self.y + self.h
    }

    pub fn contains(self, px: f64, py: f64) -> bool {
        px >= self.x && px < self.right() && py >= self.y && py < self.bottom()
    }

    /// Inline CSS geometry in percent of the workspace. Width and height carry one extra pixel,
    /// so the one pixel borders of neighbours lie on top of each other instead of doubling.
    pub fn style(self) -> String {
        format!(
            "left:{:.4}%;top:{:.4}%;width:calc({:.4}% + 1px);height:calc({:.4}% + 1px)",
            self.x / COLS * 100.0,
            self.y / ROWS * 100.0,
            self.w / COLS * 100.0,
            self.h / ROWS * 100.0,
        )
    }
}

/// How a split lays out its two subtrees.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    /// Side by side: the first subtree is on the left, the divider is a vertical line.
    Cols,
    /// Stacked: the first subtree is on top, the divider is a horizontal line.
    Rows,
}

/// Address of a split in the tree: the turns taken from the root (0 = first subtree).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Path {
    bits: u32,
    len: u8,
}

impl Path {
    fn child(self, second: bool) -> Path {
        Path {
            bits: self.bits | (u32::from(second) << self.len),
            len: self.len + 1,
        }
    }

    fn steps(self) -> impl Iterator<Item = bool> {
        (0..self.len).map(move |i| self.bits >> i & 1 == 1)
    }

    /// The split at `self` lies inside the subtrees of the split at `ancestor`.
    fn is_below(self, ancestor: Path) -> bool {
        let mask = (1u32 << ancestor.len).wrapping_sub(1);
        self.len > ancestor.len && self.bits & mask == ancestor.bits
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Node {
    Leaf(PanelId),
    Split {
        axis: Axis,
        ratio: f64,
        a: Box<Node>,
        b: Box<Node>,
    },
}

fn leaf(id: PanelId) -> Box<Node> {
    Box::new(Node::Leaf(id))
}

fn split(axis: Axis, ratio: f64, a: Box<Node>, b: Box<Node>) -> Box<Node> {
    Box::new(Node::Split { axis, ratio, a, b })
}

impl Node {
    fn present(&self, av: Availability) -> bool {
        match self {
            Node::Leaf(id) => av.has(*id),
            Node::Split { a, b, .. } => a.present(av) || b.present(av),
        }
    }

    fn contains(&self, wanted: PanelId) -> bool {
        match self {
            Node::Leaf(id) => *id == wanted,
            Node::Split { a, b, .. } => a.contains(wanted) || b.contains(wanted),
        }
    }

    fn first_leaf(&self) -> PanelId {
        match self {
            Node::Leaf(id) => *id,
            Node::Split { a, .. } => a.first_leaf(),
        }
    }

    /// Smallest (width, height) the shown part of the subtree can be squeezed to.
    fn min_size(&self, av: Availability) -> (f64, f64) {
        match self {
            Node::Leaf(id) if av.has(*id) => id.min_size(),
            Node::Leaf(_) => (0.0, 0.0),
            Node::Split { axis, a, b, .. } => match (a.present(av), b.present(av)) {
                (true, true) => {
                    let (aw, ah) = a.min_size(av);
                    let (bw, bh) = b.min_size(av);
                    match axis {
                        Axis::Cols => (aw + bw, ah.max(bh)),
                        Axis::Rows => (aw.max(bw), ah + bh),
                    }
                }
                (true, false) => a.min_size(av),
                (false, true) => b.min_size(av),
                (false, false) => (0.0, 0.0),
            },
        }
    }

    fn walk(&self, rect: Rect, path: Path, av: Availability, out: &mut Layout) {
        match self {
            Node::Leaf(id) => {
                if av.has(*id) {
                    out.tiles.push(Tile { id: *id, rect });
                }
            }
            Node::Split { axis, ratio, a, b } => match (a.present(av), b.present(av)) {
                (true, true) => {
                    let (first, second) = divide(rect, *axis, *ratio);
                    let (amin, bmin) = (a.min_size(av), b.min_size(av));
                    let (min_a, min_b) = match axis {
                        Axis::Cols => (amin.0, bmin.0),
                        Axis::Rows => (amin.1, bmin.1),
                    };
                    out.dividers.push(Divider {
                        path,
                        axis: *axis,
                        rect,
                        ratio: *ratio,
                        min_a,
                        min_b,
                    });
                    a.walk(first, path.child(false), av, out);
                    b.walk(second, path.child(true), av, out);
                }
                (true, false) => a.walk(rect, path.child(false), av, out),
                (false, true) => b.walk(rect, path.child(true), av, out),
                (false, false) => {}
            },
        }
    }

    fn at_mut(&mut self, path: Path) -> Option<&mut Node> {
        let mut node = self;
        for second in path.steps() {
            node = match node {
                Node::Split { a, b, .. } => {
                    if second {
                        b
                    } else {
                        a
                    }
                }
                Node::Leaf(_) => return None,
            };
        }
        Some(node)
    }

    fn swap_leaves(&mut self, x: PanelId, y: PanelId) {
        match self {
            Node::Leaf(id) if *id == x => *id = y,
            Node::Leaf(id) if *id == y => *id = x,
            Node::Leaf(_) => {}
            Node::Split { a, b, .. } => {
                a.swap_leaves(x, y);
                b.swap_leaves(x, y);
            }
        }
    }

    /// The tree without `id`, plus where `id` was.
    fn without(self, id: PanelId) -> (Option<Node>, Option<Home>) {
        match self {
            Node::Leaf(x) if x == id => (None, None),
            leaf @ Node::Leaf(_) => (Some(leaf), None),
            Node::Split { axis, ratio, a, b } => {
                if matches!(*a, Node::Leaf(x) if x == id) {
                    let home = Home {
                        target: DropTarget {
                            onto: b.first_leaf(),
                            zone: if axis == Axis::Cols {
                                Zone::Left
                            } else {
                                Zone::Top
                            },
                        },
                        share: ratio,
                    };
                    return (Some(*b), Some(home));
                }
                if matches!(*b, Node::Leaf(x) if x == id) {
                    let home = Home {
                        target: DropTarget {
                            onto: a.first_leaf(),
                            zone: if axis == Axis::Cols {
                                Zone::Right
                            } else {
                                Zone::Bottom
                            },
                        },
                        share: 1.0 - ratio,
                    };
                    return (Some(*a), Some(home));
                }
                let (na, home_a) = a.without(id);
                let (nb, home_b) = b.without(id);
                let home = home_a.or(home_b);
                match (na, nb) {
                    (Some(a), Some(b)) => (
                        Some(Node::Split {
                            axis,
                            ratio,
                            a: Box::new(a),
                            b: Box::new(b),
                        }),
                        home,
                    ),
                    (Some(only), None) | (None, Some(only)) => (Some(only), home),
                    (None, None) => (None, home),
                }
            }
        }
    }

    /// Replaces the leaf `onto` by a split holding it and `new`, with `new` taking `share`.
    fn insert(&mut self, new: PanelId, target: DropTarget, share: f64) -> bool {
        match self {
            Node::Leaf(id) if *id == target.onto => {
                let (axis, new_first) = match target.zone {
                    Zone::Left => (Axis::Cols, true),
                    Zone::Right | Zone::Center => (Axis::Cols, false),
                    Zone::Top => (Axis::Rows, true),
                    Zone::Bottom => (Axis::Rows, false),
                };
                let share = share.clamp(0.1, 0.9);
                let old = Box::new(Node::Leaf(*id));
                *self = if new_first {
                    Node::Split {
                        axis,
                        ratio: share,
                        a: leaf(new),
                        b: old,
                    }
                } else {
                    Node::Split {
                        axis,
                        ratio: 1.0 - share,
                        a: old,
                        b: leaf(new),
                    }
                };
                true
            }
            Node::Leaf(_) => false,
            Node::Split { a, b, .. } => {
                a.insert(new, target, share) || b.insert(new, target, share)
            }
        }
    }

    fn to_tile(&self) -> TileNode {
        match self {
            Node::Leaf(id) => TileNode::Leaf {
                panel: id.key().to_string(),
            },
            Node::Split { axis, ratio, a, b } => TileNode::Split {
                rows: *axis == Axis::Rows,
                ratio: *ratio,
                a: Box::new(a.to_tile()),
                b: Box::new(b.to_tile()),
            },
        }
    }

    /// Reads a saved tree, dropping what is not valid: unknown or repeated panels, absurd
    /// ratios, trees that are too deep.
    fn from_tile(tile: &TileNode, seen: &mut Vec<PanelId>, depth: usize) -> Option<Node> {
        match tile {
            TileNode::Empty => None,
            TileNode::Leaf { panel } => {
                let id = PanelId::from_key(panel)?;
                if seen.contains(&id) {
                    return None;
                }
                seen.push(id);
                Some(Node::Leaf(id))
            }
            TileNode::Split { rows, ratio, a, b } => {
                if depth >= MAX_DEPTH {
                    return None;
                }
                let first = Node::from_tile(a, seen, depth + 1);
                let second = Node::from_tile(b, seen, depth + 1);
                match (first, second) {
                    (Some(a), Some(b)) => Some(Node::Split {
                        axis: if *rows { Axis::Rows } else { Axis::Cols },
                        ratio: if ratio.is_finite() {
                            ratio.clamp(0.05, 0.95)
                        } else {
                            0.5
                        },
                        a: Box::new(a),
                        b: Box::new(b),
                    }),
                    (Some(only), None) | (None, Some(only)) => Some(only),
                    (None, None) => None,
                }
            }
        }
    }
}

/// The two halves of `rect` for a split at `ratio`.
fn divide(rect: Rect, axis: Axis, ratio: f64) -> (Rect, Rect) {
    match axis {
        Axis::Cols => {
            let w = rect.w * ratio;
            (
                Rect { w, ..rect },
                Rect {
                    x: rect.x + w,
                    w: rect.w - w,
                    ..rect
                },
            )
        }
        Axis::Rows => {
            let h = rect.h * ratio;
            (
                Rect { h, ..rect },
                Rect {
                    y: rect.y + h,
                    h: rect.h - h,
                    ..rect
                },
            )
        }
    }
}

/// Where a panel was before it was closed, so that it comes back to the same place.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Home {
    target: DropTarget,
    /// The panel's share of the split it was part of.
    share: f64,
}

/// Which part of a panel a drop lands on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zone {
    Left,
    Right,
    Top,
    Bottom,
    /// The middle: the two panels swap places.
    Center,
}

/// A drop: next to (or onto) the panel `onto`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DropTarget {
    pub onto: PanelId,
    pub zone: Zone,
}

/// A panel and the cells it covers right now.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tile {
    pub id: PanelId,
    pub rect: Rect,
}

/// The line between two panels, with everything a drag needs to move it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Divider {
    pub path: Path,
    pub axis: Axis,
    /// The whole area the split divides.
    pub rect: Rect,
    pub ratio: f64,
    /// Smallest extent (cells along the axis) of the first / second side.
    pub min_a: f64,
    pub min_b: f64,
}

impl Divider {
    /// Where the line is, as a degenerate rectangle (zero wide for a vertical line).
    pub fn line(self) -> Rect {
        match self.axis {
            Axis::Cols => Rect {
                x: self.rect.x + self.rect.w * self.ratio,
                y: self.rect.y,
                w: 0.0,
                h: self.rect.h,
            },
            Axis::Rows => Rect {
                x: self.rect.x,
                y: self.rect.y + self.rect.h * self.ratio,
                w: self.rect.w,
                h: 0.0,
            },
        }
    }

    /// Where the line is along its axis, cells from the workspace edge.
    fn position(self) -> f64 {
        match self.axis {
            Axis::Cols => self.rect.x + self.rect.w * self.ratio,
            Axis::Rows => self.rect.y + self.rect.h * self.ratio,
        }
    }

    /// The ratio that puts the line exactly at `boundary` (no snapping), kept inside what both
    /// sides can be squeezed to.
    fn ratio_at(self, boundary: f64) -> f64 {
        let (start, len) = match self.axis {
            Axis::Cols => (self.rect.x, self.rect.w),
            Axis::Rows => (self.rect.y, self.rect.h),
        };
        let (low, high) = (self.min_a, len - self.min_b);
        if len <= 0.0 || !boundary.is_finite() || low > high {
            return self.ratio;
        }
        ((boundary - start).clamp(low, high) / len).clamp(0.05, 0.95)
    }

    /// The ratio that puts the line on the grid line `boundary` (cells from the workspace edge),
    /// kept inside what both sides can be squeezed to. Unchanged when there is no room at all.
    pub fn ratio_for(self, boundary: f64) -> f64 {
        let (start, len) = match self.axis {
            Axis::Cols => (self.rect.x, self.rect.w),
            Axis::Rows => (self.rect.y, self.rect.h),
        };
        if len <= 0.0 || !boundary.is_finite() {
            return self.ratio;
        }
        let low = self.min_a;
        let high = len - self.min_b;
        if low > high {
            return self.ratio;
        }
        ((boundary.round() - start).clamp(low, high) / len).clamp(0.05, 0.95)
    }
}

/// Everything that is on screen: the panels and the dividers between them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
    pub tiles: Vec<Tile>,
    pub dividers: Vec<Divider>,
}

impl Layout {
    pub fn tile(&self, id: PanelId) -> Option<Tile> {
        self.tiles.iter().copied().find(|t| t.id == id)
    }
}

/// The tiling of the screen.
#[derive(Clone, Debug)]
pub struct Dock {
    root: Option<Node>,
    /// Where each closed panel used to be (not saved; the default rules apply after a restart).
    homes: Vec<(PanelId, Home)>,
}

/// Two docks are equal when they tile the same way; the memory of closed panels does not count.
impl PartialEq for Dock {
    fn eq(&self, other: &Dock) -> bool {
        self.root == other.root
    }
}

impl Default for Dock {
    /// The map with the profile and the transport under it, the live numbers, the next climb
    /// and the statistics in a column on the right, and the two conditional panels in a column
    /// left of the map, the gear panel next to the transport (they take no room while they are not
    /// available).
    fn default() -> Self {
        use PanelId::*;
        let side = split(Axis::Rows, 0.5, leaf(Workout), leaf(Rider));
        let top = split(Axis::Cols, 0.375, side, leaf(Map));
        let transport = split(Axis::Cols, 0.5625, leaf(Controls), leaf(Gears));
        let bottom = split(Axis::Rows, 1.0 / 3.0, transport, leaf(Profile));
        let left = split(Axis::Rows, 0.625, top, bottom);
        let right = split(
            Axis::Rows,
            0.5,
            leaf(Metrics),
            split(Axis::Rows, 0.5, leaf(Climb), leaf(Stats)),
        );
        Dock {
            root: Some(*split(Axis::Cols, 2.0 / 3.0, left, right)),
            homes: Vec::new(),
        }
    }
}

impl Dock {
    /// Rebuilds the dock from the saved tiling (`None` = the default one).
    pub fn from_settings(saved: Option<&TileNode>) -> Dock {
        match saved {
            None => Dock::default(),
            Some(tile) => Dock {
                root: Node::from_tile(tile, &mut Vec::new(), 0),
                homes: Vec::new(),
            },
        }
    }

    pub fn to_settings(&self) -> Option<TileNode> {
        Some(match &self.root {
            Some(root) => root.to_tile(),
            None => TileNode::Empty,
        })
    }

    /// The panel is part of the tiling (it may still be unavailable right now).
    pub fn contains(&self, id: PanelId) -> bool {
        self.root.as_ref().is_some_and(|root| root.contains(id))
    }

    pub fn layout(&self, av: Availability) -> Layout {
        let mut out = Layout::default();
        if let Some(root) = &self.root {
            root.walk(Rect::GRID, Path::default(), av, &mut out);
        }
        out
    }

    /// Moves the divider at `path` to `ratio`.
    pub fn set_ratio(&mut self, path: Path, ratio: f64) {
        if let Some(Node::Split { ratio: r, .. }) =
            self.root.as_mut().and_then(|root| root.at_mut(path))
        {
            *r = ratio.clamp(0.05, 0.95);
        }
    }

    /// This dock with the divider at `path` moved to `ratio`, the way a tiling window manager
    /// resizes: only the two panels touching the line change. The ratios of the splits are
    /// relative to their parent, so on their own the lines nested behind the moved one (in the
    /// same direction) would slide along with it and resize panels that do not touch it; here
    /// they keep where they were, unless a panel next to the moved line is at its minimum and
    /// pushes them. Call it on the dock as it was when the drag started, so a line pushed on the
    /// way out comes back on the way back.
    pub fn resized(&self, path: Path, ratio: f64, av: Availability) -> Dock {
        let before = self.layout(av);
        let mut dock = self.clone();
        dock.set_ratio(path, ratio);
        let Some(moved) = before.dividers.iter().find(|d| d.path == path).copied() else {
            return dock;
        };
        let mut nested: Vec<Divider> = before
            .dividers
            .iter()
            .copied()
            .filter(|d| d.axis == moved.axis && d.path.is_below(path))
            .collect();
        // outer lines first: an inner line's rect depends on them
        nested.sort_by_key(|d| d.path.len);
        for old in nested {
            let now = dock.layout(av);
            if let Some(d) = now.dividers.iter().find(|d| d.path == old.path) {
                dock.set_ratio(old.path, d.ratio_at(old.position()));
            }
        }
        dock
    }

    /// Closes a panel: its neighbour takes the space, and its place is remembered.
    pub fn hide(&mut self, id: PanelId) {
        let Some(root) = self.root.take() else {
            return;
        };
        let (rest, home) = root.without(id);
        self.root = rest;
        if let Some(home) = home {
            self.homes.retain(|(other, _)| *other != id);
            self.homes.push((id, home));
        }
    }

    /// Brings a closed panel back: at `at` when it was dropped somewhere, otherwise where it used
    /// to be, otherwise next to the biggest panel. Does nothing for a panel that is already shown.
    pub fn show(&mut self, id: PanelId, at: Option<DropTarget>, av: Availability) {
        if self.contains(id) {
            return;
        }
        let home = self
            .homes
            .iter()
            .find(|(other, home)| *other == id && self.contains(home.target.onto))
            .map(|(_, home)| *home);
        let (target, share) = match (at, home) {
            (Some(target), _) => (Some(target), self.share_for(id, target, 0.5, av)),
            (None, Some(home)) => (Some(home.target), home.share),
            (None, None) => match self.next_to_the_biggest(av) {
                Some(target) => (Some(target), self.share_for(id, target, 0.5, av)),
                None => (None, 0.5),
            },
        };
        let placed = match (&mut self.root, target) {
            (Some(root), Some(target)) => root.insert(id, target, share),
            _ => false,
        };
        if !placed {
            match &mut self.root {
                Some(root) => {
                    // nothing to put it next to (everything left is unavailable): beside the first leaf
                    let onto = root.first_leaf();
                    root.insert(
                        id,
                        DropTarget {
                            onto,
                            zone: Zone::Right,
                        },
                        0.5,
                    );
                }
                None => self.root = Some(Node::Leaf(id)),
            }
        }
        self.homes.retain(|(other, _)| *other != id);
    }

    /// Moves a shown panel next to another one, or swaps the two when dropped in the middle. A
    /// closed panel is shown there instead.
    pub fn relocate(&mut self, id: PanelId, target: DropTarget, av: Availability) {
        if target.onto == id || !self.contains(target.onto) {
            return;
        }
        if !self.contains(id) {
            self.show(id, Some(target), av);
            return;
        }
        if target.zone == Zone::Center {
            if let Some(root) = &mut self.root {
                root.swap_leaves(id, target.onto);
            }
            return;
        }
        self.hide(id);
        self.homes.retain(|(other, _)| *other != id);
        self.show(id, Some(target), av);
    }

    /// The share a new panel needs of the split it is dropped into so that it is not smaller than
    /// its minimum (a browser is useless at 300 px): `share` or more, at most 0.8.
    fn share_for(&self, id: PanelId, target: DropTarget, share: f64, av: Availability) -> f64 {
        let Some(rect) = self.layout(av).tile(target.onto).map(|t| t.rect) else {
            return share;
        };
        let (min_w, min_h) = id.min_size();
        let need = match target.zone {
            Zone::Left | Zone::Right => min_w / rect.w,
            Zone::Top | Zone::Bottom => min_h / rect.h,
            Zone::Center => 0.0,
        };
        share.max(need).min(0.8)
    }

    /// Splits the biggest shown panel along its longer side.
    fn next_to_the_biggest(&self, av: Availability) -> Option<DropTarget> {
        let layout = self.layout(av);
        let biggest = layout
            .tiles
            .iter()
            .max_by(|x, y| (x.rect.w * x.rect.h).total_cmp(&(y.rect.w * y.rect.h)))?;
        Some(DropTarget {
            onto: biggest.id,
            zone: if biggest.rect.w >= biggest.rect.h {
                Zone::Right
            } else {
                Zone::Bottom
            },
        })
    }
}

/// What a drop at the grid position (`px`, `py`) cells would do to a panel being carried: the
/// edge of the panel underneath splits it, its middle swaps (`allow_swap`, for panels that are
/// already shown). Nothing happens over the carried panel itself or outside every panel.
pub fn drop_target(
    layout: &Layout,
    px: f64,
    py: f64,
    carried: PanelId,
    allow_swap: bool,
) -> Option<DropTarget> {
    let tile = layout.tiles.iter().find(|t| t.rect.contains(px, py))?;
    if tile.id == carried {
        return None;
    }
    let (rx, ry) = (
        (px - tile.rect.x) / tile.rect.w,
        (py - tile.rect.y) / tile.rect.h,
    );
    let zone = if allow_swap && (0.3..0.7).contains(&rx) && (0.3..0.7).contains(&ry) {
        Zone::Center
    } else {
        let edges = [
            (rx, Zone::Left),
            (1.0 - rx, Zone::Right),
            (ry, Zone::Top),
            (1.0 - ry, Zone::Bottom),
        ];
        edges
            .into_iter()
            .min_by(|x, y| x.0.total_cmp(&y.0))
            .map(|(_, zone)| zone)?
    };
    Some(DropTarget {
        onto: tile.id,
        zone,
    })
}

/// The area a drop would take: half of the panel for an edge, all of it for a swap.
pub fn drop_ghost(layout: &Layout, target: DropTarget) -> Option<Rect> {
    let rect = layout.tile(target.onto)?.rect;
    Some(match target.zone {
        Zone::Center => rect,
        Zone::Left => Rect {
            w: rect.w / 2.0,
            ..rect
        },
        Zone::Right => Rect {
            x: rect.x + rect.w / 2.0,
            w: rect.w / 2.0,
            ..rect
        },
        Zone::Top => Rect {
            h: rect.h / 2.0,
            ..rect
        },
        Zone::Bottom => Rect {
            y: rect.y + rect.h / 2.0,
            h: rect.h / 2.0,
            ..rect
        },
    })
}

/// Where the workspace is on the page, in CSS pixels (measured by a ResizeObserver).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Workspace {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}

impl Workspace {
    pub fn is_usable(self) -> bool {
        [self.left, self.top, self.width, self.height]
            .iter()
            .all(|v| v.is_finite())
            && self.width > 1.0
            && self.height > 1.0
    }

    /// A client point in grid cells (fractional); `None` outside the workspace.
    pub fn cell_pos(self, x: f64, y: f64) -> Option<(f64, f64)> {
        if !self.is_usable() || !x.is_finite() || !y.is_finite() {
            return None;
        }
        let (rx, ry) = (x - self.left, y - self.top);
        if rx < 0.0 || ry < 0.0 || rx >= self.width || ry >= self.height {
            return None;
        }
        Some((rx / self.width * COLS, ry / self.height * ROWS))
    }

    /// Like [`Workspace::cell_pos`] but also outside the workspace (the pointer may leave it
    /// while a divider is dragged).
    pub fn cell_pos_unbounded(self, x: f64, y: f64) -> Option<(f64, f64)> {
        if !self.is_usable() || !x.is_finite() || !y.is_finite() {
            return None;
        }
        Some((
            (x - self.left) / self.width * COLS,
            (y - self.top) / self.height * ROWS,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_THERE: Availability = Availability {
        workout: true,
        rider: true,
        gears: true,
    };
    const NONE_OF_THEM: Availability = Availability {
        workout: false,
        rider: false,
        gears: false,
    };

    fn area(layout: &Layout) -> f64 {
        layout.tiles.iter().map(|t| t.rect.w * t.rect.h).sum()
    }

    /// The panels cover the whole grid and no two overlap.
    fn assert_tiles_the_grid(layout: &Layout) {
        assert!(
            (area(layout) - COLS * ROWS).abs() < 1e-6,
            "area {}",
            area(layout)
        );
        for (i, a) in layout.tiles.iter().enumerate() {
            let r = a.rect;
            assert!(
                r.x >= -1e-9
                    && r.y >= -1e-9
                    && r.right() <= COLS + 1e-9
                    && r.bottom() <= ROWS + 1e-9
            );
            for b in &layout.tiles[i + 1..] {
                let o = b.rect;
                let overlap = r.x < o.right() - 1e-9
                    && o.x < r.right() - 1e-9
                    && r.y < o.bottom() - 1e-9
                    && o.y < r.bottom() - 1e-9;
                assert!(!overlap, "{:?} overlaps {:?}", a.id, b.id);
            }
        }
    }

    fn ids(layout: &Layout) -> Vec<PanelId> {
        let mut ids: Vec<_> = layout.tiles.iter().map(|t| t.id).collect();
        ids.sort_by_key(|id| PanelId::ALL.iter().position(|x| x == id));
        ids
    }

    #[test]
    fn keys_round_trip_and_are_unique() {
        for id in PanelId::ALL {
            assert_eq!(PanelId::from_key(id.key()), Some(id));
        }
        assert_eq!(PanelId::from_key("nope"), None);
        let mut keys: Vec<_> = PanelId::ALL.iter().map(|id| id.key()).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), PanelId::ALL.len());
    }

    #[test]
    fn the_default_layout_tiles_the_screen_with_and_without_the_conditional_panels() {
        let dock = Dock::default();
        for av in [
            ALL_THERE,
            NONE_OF_THEM,
            Availability {
                workout: true,
                rider: false,
                gears: false,
            },
        ] {
            let layout = dock.layout(av);
            assert_tiles_the_grid(&layout);
            for tile in &layout.tiles {
                let (mw, mh) = tile.id.min_size();
                assert!(
                    tile.rect.w >= mw - 1e-9 && tile.rect.h >= mh - 1e-9,
                    "{:?} is below its minimum: {:?}",
                    tile.id,
                    tile.rect
                );
            }
        }
        let layout = dock.layout(NONE_OF_THEM);
        assert!(layout.tile(PanelId::Workout).is_none() && layout.tile(PanelId::Rider).is_none());
        assert!(layout.tile(PanelId::Map).is_some());
        // charts and street view start parked
        assert!(!dock.contains(PanelId::Charts) && !dock.contains(PanelId::Browser));
        // the map takes the room of the conditional panels while they are absent
        assert!(
            layout.tile(PanelId::Map).unwrap().rect.w
                > dock.layout(ALL_THERE).tile(PanelId::Map).unwrap().rect.w
        );
    }

    #[test]
    fn there_is_one_divider_between_every_two_neighbours() {
        let dock = Dock::default();
        let layout = dock.layout(NONE_OF_THEM);
        assert_eq!(layout.dividers.len(), layout.tiles.len() - 1);
    }

    #[test]
    fn closing_a_panel_gives_its_space_to_the_neighbour_and_it_returns_to_the_same_place() {
        let mut dock = Dock::default();
        let before = dock.layout(NONE_OF_THEM);
        dock.hide(PanelId::Climb);
        let hidden = dock.layout(NONE_OF_THEM);
        assert_tiles_the_grid(&hidden);
        assert!(hidden.tile(PanelId::Climb).is_none());
        let stats_before = before.tile(PanelId::Stats).unwrap().rect;
        let stats_after = hidden.tile(PanelId::Stats).unwrap().rect;
        assert!(stats_after.h > stats_before.h, "the neighbour grew");
        dock.show(PanelId::Climb, None, NONE_OF_THEM);
        assert_eq!(
            dock.layout(NONE_OF_THEM),
            before,
            "back exactly where it was"
        );
    }

    #[test]
    fn a_closed_panel_without_a_home_splits_the_biggest_one() {
        let mut dock = Dock::default();
        dock.show(PanelId::Charts, None, NONE_OF_THEM);
        let layout = dock.layout(NONE_OF_THEM);
        assert_tiles_the_grid(&layout);
        assert!(layout.tile(PanelId::Charts).is_some());
        dock.show(PanelId::Browser, None, NONE_OF_THEM);
        assert_tiles_the_grid(&dock.layout(NONE_OF_THEM));
    }

    #[test]
    fn closing_everything_and_opening_one_again() {
        let mut dock = Dock::default();
        for id in PanelId::ALL {
            dock.hide(id);
        }
        assert!(dock.layout(ALL_THERE).tiles.is_empty());
        assert_eq!(dock.to_settings(), Some(TileNode::Empty));
        dock.show(PanelId::Map, None, NONE_OF_THEM);
        let layout = dock.layout(NONE_OF_THEM);
        assert_eq!(ids(&layout), [PanelId::Map]);
        assert_tiles_the_grid(&layout);
        assert!(layout.dividers.is_empty());
    }

    #[test]
    fn dropping_on_an_edge_splits_that_panel() {
        let mut dock = Dock::default();
        dock.show(
            PanelId::Charts,
            Some(DropTarget {
                onto: PanelId::Map,
                zone: Zone::Bottom,
            }),
            NONE_OF_THEM,
        );
        let layout = dock.layout(NONE_OF_THEM);
        assert_tiles_the_grid(&layout);
        let map = layout.tile(PanelId::Map).unwrap().rect;
        let charts = layout.tile(PanelId::Charts).unwrap().rect;
        assert!(
            (charts.y - map.bottom()).abs() < 1e-9,
            "charts sit right under the map"
        );
        assert!((charts.h - map.h).abs() < 1e-9, "half and half");
        assert!((charts.w - map.w).abs() < 1e-9);
    }

    #[test]
    fn a_panel_that_needs_room_gets_it_when_it_is_dropped_in() {
        // The browser panel is the only panel with a very large minimum; off Windows it does not
        // exist at all (`Availability::has`), so the rule cannot be checked with it there.
        if !NONE_OF_THEM.has(PanelId::Browser) {
            return;
        }
        let mut dock = Dock::default();
        dock.show(PanelId::Browser, None, NONE_OF_THEM);
        let layout = dock.layout(NONE_OF_THEM);
        assert_tiles_the_grid(&layout);
        let browser = layout.tile(PanelId::Browser).unwrap().rect;
        let (min_w, min_h) = PanelId::Browser.min_size();
        assert!(
            browser.w >= min_w - 1e-9 && browser.h >= min_h - 1e-9,
            "{browser:?}"
        );
    }

    #[test]
    fn moving_a_panel_next_to_another_keeps_the_screen_tiled() {
        let mut dock = Dock::default();
        dock.relocate(
            PanelId::Stats,
            DropTarget {
                onto: PanelId::Map,
                zone: Zone::Left,
            },
            NONE_OF_THEM,
        );
        let layout = dock.layout(NONE_OF_THEM);
        assert_tiles_the_grid(&layout);
        let stats = layout.tile(PanelId::Stats).unwrap().rect;
        let map = layout.tile(PanelId::Map).unwrap().rect;
        assert!(
            (stats.right() - map.x).abs() < 1e-9,
            "stats left of the map"
        );
        assert_eq!(layout.tiles.len(), 6);
    }

    #[test]
    fn dropping_in_the_middle_swaps_two_panels() {
        let mut dock = Dock::default();
        let before = dock.layout(NONE_OF_THEM);
        dock.relocate(
            PanelId::Map,
            DropTarget {
                onto: PanelId::Metrics,
                zone: Zone::Center,
            },
            NONE_OF_THEM,
        );
        let after = dock.layout(NONE_OF_THEM);
        assert_eq!(
            after.tile(PanelId::Map).unwrap().rect,
            before.tile(PanelId::Metrics).unwrap().rect
        );
        assert_eq!(
            after.tile(PanelId::Metrics).unwrap().rect,
            before.tile(PanelId::Map).unwrap().rect
        );
        assert_tiles_the_grid(&after);
    }

    #[test]
    fn relocating_onto_itself_or_a_missing_panel_does_nothing() {
        let mut dock = Dock::default();
        let before = dock.clone();
        dock.relocate(
            PanelId::Map,
            DropTarget {
                onto: PanelId::Map,
                zone: Zone::Left,
            },
            NONE_OF_THEM,
        );
        dock.relocate(
            PanelId::Map,
            DropTarget {
                onto: PanelId::Charts,
                zone: Zone::Left,
            },
            NONE_OF_THEM,
        );
        assert_eq!(dock, before);
    }

    #[test]
    fn dragging_a_divider_resizes_both_sides_and_respects_the_minimums() {
        let mut dock = Dock::default();
        let layout = dock.layout(NONE_OF_THEM);
        // the divider between the map column and the right column
        let divider = layout
            .dividers
            .iter()
            .copied()
            .find(|d| d.axis == Axis::Cols && d.rect == Rect::GRID)
            .expect("root divider");
        assert!((divider.line().x - 16.0 * FINE).abs() < 1e-9);
        // to cell 14: the map shrinks, the right column grows by exactly two old steps
        dock.set_ratio(divider.path, divider.ratio_for(14.0 * FINE));
        let moved = dock.layout(NONE_OF_THEM);
        assert_tiles_the_grid(&moved);
        let metrics = moved.tile(PanelId::Metrics).unwrap().rect;
        assert!((metrics.x - 14.0 * FINE).abs() < 1e-9 && (metrics.w - 10.0 * FINE).abs() < 1e-9);
        // dragging way past the edge stops at the smallest sizes of what is on the other side
        let ratio = divider.ratio_for(-50.0);
        assert!(ratio * COLS >= divider.min_a - 1e-9);
        let ratio = divider.ratio_for(500.0);
        assert!((1.0 - ratio) * COLS >= divider.min_b - 1e-9);
        // the dragged line sits on a whole cell
        let snapped = divider.ratio_for(53.6) * COLS;
        assert!((snapped - 54.0).abs() < 1e-9, "{snapped}");
        // a divider that has no room at all stays where it is
        let cramped = Divider {
            min_a: 20.0 * FINE,
            min_b: 20.0 * FINE,
            ..divider
        };
        assert_eq!(cramped.ratio_for(3.0), cramped.ratio);
        assert_eq!(divider.ratio_for(f64::NAN), divider.ratio);
    }

    /// The bug report: dragging the bottom edge of the map also resized the profile, two panels
    /// away. Only the controls, which touch the line, may change.
    #[test]
    fn dragging_a_line_resizes_only_the_panels_that_touch_it() {
        let dock = Dock::default();
        let before = dock.layout(NONE_OF_THEM);
        let map = before.tile(PanelId::Map).unwrap().rect;
        let line = before
            .dividers
            .iter()
            .copied()
            .find(|d| d.axis == Axis::Rows && (d.line().y - map.bottom()).abs() < 1e-9)
            .expect("the line under the map");
        // the map gives two old steps to the controls
        let moved = dock.resized(
            line.path,
            line.ratio_for(map.bottom() - 2.0 * FINE),
            NONE_OF_THEM,
        );
        let after = moved.layout(NONE_OF_THEM);
        assert_tiles_the_grid(&after);
        let (p0, p1) = (
            before.tile(PanelId::Profile).unwrap().rect,
            after.tile(PanelId::Profile).unwrap().rect,
        );
        assert!(
            (p0.y - p1.y).abs() < 1e-9 && (p0.h - p1.h).abs() < 1e-9,
            "{p0:?} -> {p1:?}"
        );
        let (c0, c1) = (
            before.tile(PanelId::Controls).unwrap().rect,
            after.tile(PanelId::Controls).unwrap().rect,
        );
        assert!(
            (c1.h - (c0.h + 2.0 * FINE)).abs() < 1e-9,
            "the controls take the room"
        );
        assert!((after.tile(PanelId::Map).unwrap().rect.h - (map.h - 2.0 * FINE)).abs() < 1e-9);

        // pushed further than the controls can shrink: they stop at their minimum and the
        // profile gives the rest; dragged back from the same start, the profile is whole again
        let far = dock.resized(line.path, line.ratio_for(ROWS), NONE_OF_THEM);
        let far = far.layout(NONE_OF_THEM);
        assert_tiles_the_grid(&far);
        let controls = far.tile(PanelId::Controls).unwrap().rect;
        assert!(
            (controls.h - PanelId::Controls.min_size().1).abs() < 1e-9,
            "{controls:?}"
        );
        assert!(far.tile(PanelId::Profile).unwrap().rect.h < p0.h);
        let back = dock.resized(line.path, line.ratio, NONE_OF_THEM);
        assert_eq!(back.layout(NONE_OF_THEM), before);
    }

    #[test]
    fn paths_know_their_ancestors() {
        let root = Path::default();
        let a = root.child(false);
        let ab = a.child(true);
        let b = root.child(true);
        assert!(a.is_below(root) && ab.is_below(root) && ab.is_below(a));
        assert!(!ab.is_below(b) && !a.is_below(a) && !root.is_below(a));
    }

    #[test]
    fn the_divider_between_two_stacked_panels_moves_vertically() {
        let dock = Dock::default();
        let layout = dock.layout(NONE_OF_THEM);
        let divider = layout
            .dividers
            .iter()
            .copied()
            .find(|d| d.axis == Axis::Rows && d.rect.x == 16.0 * FINE && d.rect.y == 0.0)
            .expect("metrics | rest");
        assert!((divider.line().y - 8.0 * FINE).abs() < 1e-9);
        let ratio = divider.ratio_for(41.0);
        assert!((ratio * divider.rect.h - 41.0).abs() < 1e-9);
    }

    #[test]
    fn drop_targets_follow_the_pointer() {
        let dock = Dock::default();
        let layout = dock.layout(NONE_OF_THEM);
        let map = layout.tile(PanelId::Map).unwrap().rect;
        let (cx, cy) = (map.x + map.w / 2.0, map.y + map.h / 2.0);
        let t = |x, y, swap| drop_target(&layout, x, y, PanelId::Stats, swap);
        assert_eq!(t(cx, cy, true).unwrap().zone, Zone::Center);
        assert_ne!(
            t(cx, cy, false).unwrap().zone,
            Zone::Center,
            "a closed panel cannot swap"
        );
        assert_eq!(t(map.x + 0.3, cy, true).unwrap().zone, Zone::Left);
        assert_eq!(t(map.right() - 0.3, cy, true).unwrap().zone, Zone::Right);
        assert_eq!(t(cx, map.y + 0.3, true).unwrap().zone, Zone::Top);
        assert_eq!(t(cx, map.bottom() - 0.3, true).unwrap().zone, Zone::Bottom);
        // over the carried panel itself nothing happens
        let stats = layout.tile(PanelId::Stats).unwrap().rect;
        assert_eq!(t(stats.x + 1.0, stats.y + 1.0, true), None);
        // outside the grid there is no panel
        assert_eq!(t(-1.0, 3.0, true), None);
    }

    #[test]
    fn the_ghost_covers_what_the_drop_would_take() {
        let dock = Dock::default();
        let layout = dock.layout(NONE_OF_THEM);
        let map = layout.tile(PanelId::Map).unwrap().rect;
        let ghost = |zone| {
            drop_ghost(
                &layout,
                DropTarget {
                    onto: PanelId::Map,
                    zone,
                },
            )
            .unwrap()
        };
        assert_eq!(ghost(Zone::Center), map);
        assert!((ghost(Zone::Left).w - map.w / 2.0).abs() < 1e-9);
        assert!((ghost(Zone::Right).x - (map.x + map.w / 2.0)).abs() < 1e-9);
        assert!((ghost(Zone::Top).h - map.h / 2.0).abs() < 1e-9);
        assert!((ghost(Zone::Bottom).y - (map.y + map.h / 2.0)).abs() < 1e-9);
        assert_eq!(
            drop_ghost(
                &layout,
                DropTarget {
                    onto: PanelId::Charts,
                    zone: Zone::Left
                }
            ),
            None
        );
    }

    #[test]
    fn settings_round_trip() {
        let mut dock = Dock::default();
        dock.hide(PanelId::Climb);
        dock.show(
            PanelId::Browser,
            Some(DropTarget {
                onto: PanelId::Map,
                zone: Zone::Right,
            }),
            NONE_OF_THEM,
        );
        let back = Dock::from_settings(dock.to_settings().as_ref());
        assert_eq!(back, dock);
        assert_eq!(Dock::from_settings(None), Dock::default());
    }

    #[test]
    fn a_damaged_settings_tree_is_repaired() {
        let leaf = |key: &str| TileNode::Leaf {
            panel: key.to_string(),
        };
        let tree = TileNode::Split {
            rows: false,
            ratio: f64::NAN,
            a: Box::new(TileNode::Split {
                rows: true,
                ratio: 7.0,
                a: Box::new(leaf("map")),
                b: Box::new(leaf("from-the-future")),
            }),
            b: Box::new(TileNode::Split {
                rows: true,
                ratio: -3.0,
                a: Box::new(leaf("map")),
                b: Box::new(leaf("stats")),
            }),
        };
        let dock = Dock::from_settings(Some(&tree));
        let layout = dock.layout(NONE_OF_THEM);
        // the unknown panel and the repeated map are gone, the rest still tiles the screen
        assert_eq!(ids(&layout), [PanelId::Map, PanelId::Stats]);
        assert_tiles_the_grid(&layout);
        // a tree that is far too deep is cut off instead of recursing forever
        let mut deep = leaf("map");
        for _ in 0..200 {
            deep = TileNode::Split {
                rows: false,
                ratio: 0.5,
                a: Box::new(deep),
                b: Box::new(leaf("stats")),
            };
        }
        let _ = Dock::from_settings(Some(&deep));
        assert!(Dock::from_settings(Some(&TileNode::Empty))
            .layout(ALL_THERE)
            .tiles
            .is_empty());
    }

    #[test]
    fn a_closed_conditional_panel_comes_back_when_asked() {
        let mut dock = Dock::default();
        dock.hide(PanelId::Workout);
        assert!(!dock.contains(PanelId::Workout));
        dock.show(PanelId::Workout, None, ALL_THERE);
        let layout = dock.layout(ALL_THERE);
        assert!(layout.tile(PanelId::Workout).is_some());
        assert_tiles_the_grid(&layout);
    }

    #[test]
    fn css_geometry_is_in_percent_of_the_grid() {
        let style = Rect {
            x: COLS / 2.0,
            y: ROWS / 2.0,
            w: COLS / 4.0,
            h: ROWS / 4.0,
        }
        .style();
        assert!(style.contains("left:50.0000%"), "{style}");
        assert!(style.contains("top:50.0000%"), "{style}");
        assert!(style.contains("width:calc(25.0000% + 1px)"), "{style}");
        assert!(style.contains("height:calc(25.0000% + 1px)"), "{style}");
    }

    #[test]
    fn pointer_positions_become_cells() {
        let w = Workspace {
            left: 0.0,
            top: 40.0,
            width: 1200.0,
            height: 640.0,
        };
        assert_eq!(w.cell_pos(600.0, 360.0), Some((COLS / 2.0, ROWS / 2.0)));
        assert_eq!(
            w.cell_pos(10.0, 20.0),
            None,
            "the top bar is not the workspace"
        );
        assert_eq!(w.cell_pos(1200.0, 100.0), None);
        assert_eq!(w.cell_pos(f64::NAN, 100.0), None);
        let (ux, uy) = w.cell_pos_unbounded(-60.0, 40.0).unwrap();
        assert!((ux + 0.05 * COLS).abs() < 1e-9 && uy.abs() < 1e-9);
        assert_eq!(Workspace::default().cell_pos(1.0, 1.0), None);
    }
}
