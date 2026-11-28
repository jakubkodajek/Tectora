//! Towers and turrets (věž, věžička): a shaft that stands through the roof
//! and carries its own roof of stacked tiers, e.g. a pyramid, a cone, a
//! bell-shaped helm, an onion dome, a lantern and a spire.
//!
//! DACAD has no such object: its help models a cone or a tower roof as a
//! polygon of walls with the same pitch (DACAD.HLP via the 2026-11-08 gap
//! analysis) and has no curved profile at all. Tectora calculates a tower
//! in closed form instead (ADR 0008, Jakub 2026-11-14: keep DACAD's
//! behaviour where it is good, improve it where it is impractical):
//!
//! - The tower's plan is a convex **shape** around its centre: a rectangle
//!   or a regular polygon, both with optional rounded corners, or a circle.
//! - Its roof is a **profile** of tiers from `base_height` up. A tier rises
//!   by `rise` and ends at `top_width`; the plan at every height is the base
//!   shape scaled about the centre (a homothety), so straight tiers are
//!   frusta whose side faces are planar trapezoids. A tier with a `bulge`
//!   follows a circular arc in the profile (positive outward like an onion,
//!   negative inward like a bell); the arc is integrated in fine steps.
//!
//! Area of the band between two levels with scale `k_a`, `k_b` and heights
//! `z_a`, `z_b`, for an outline edge of length `l` whose line lies `d` from
//! the centre: a trapezoid with parallel sides `k_a·l`, `k_b·l` and height
//! `sqrt(Δz² + (Δk·d)²)`. This is exact for straight tiers of polygonal
//! shapes; circles and rounded corners are taken as fine polygons
//! ([`FINE_ARC_STEP`], 512 steps a turn) and curved tiers as
//! [`FINE_ARC_SUBSTEPS`] chords; against Archimedes' sphere zones and the
//! cone the error stays under 1e-4 (tests below).
//!
//! The shaft is cut out of every roof it stands in like a chimney (DACAD's
//! chimney rules: plan area / cos(pitch) deducted, abutment lines WV/WH/WS),
//! see [`Tower::footprint`] and `openings::cleaned`.

// `!(x > 0.0)` and the like are deliberate: they also refuse NaN.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use std::f64::consts::PI;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::geom::Point2;

/// Plan shape of a tower.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TowerShape {
    /// `width` along the tower's x axis, `depth` across; corners rounded by
    /// `corner_radius`.
    #[default]
    Rectangle,
    /// Regular polygon with `sides` sides, `width` across the flats, one
    /// side parallel to the tower's x axis; corners rounded by
    /// `corner_radius`.
    Polygon,
    /// Circle of diameter `width`.
    Circle,
}

/// One tier of a tower's roof profile, from the top of the tier below (or
/// the tower's base) up.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TowerTier {
    /// Height the tier rises, m (0 for a flat ledge).
    pub rise: f64,
    /// Width of the plan at the top of the tier, m, measured like the
    /// tower's `width` (0 = the tier ends in a point).
    pub top_width: f64,
    /// Curvature of the tier's profile: the arc's height over its chord,
    /// m; positive bulges outward (onion), negative inward (bell). At most
    /// half the chord (a semicircle).
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub bulge: f64,
    /// Roof material, numbered as on walls (1 tiles, 2 sheet metal, …).
    #[serde(default = "default_tier_material")]
    pub material: u8,
}

/// A tower or turret standing through the roof (see the module docs).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Tower {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    /// Plan point of the tower's axis.
    pub center: Point2,
    /// Turn of the tower's x axis from the plan's x axis, degrees
    /// counter-clockwise.
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub rotation_deg: f64,
    #[serde(default)]
    pub shape: TowerShape,
    /// Width of the shaft, m (rectangle: along the tower's x axis; polygon:
    /// across the flats; circle: diameter).
    pub width: f64,
    /// Depth of a rectangular shaft, m (across the tower's x axis); `None`
    /// for a square.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth: Option<f64>,
    /// Sides of a polygonal shaft.
    #[serde(default = "default_sides", skip_serializing_if = "is_default_sides")]
    pub sides: u32,
    /// Radius of the rounded corners of a rectangle or polygon at the base,
    /// m (scales with the plan up the roof).
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub corner_radius: f64,
    /// Height where the tower's roof starts (top of the shaft), m above
    /// ground: the roof over the shaft's wall line.
    pub base_height: f64,
    /// Eave overhang of the tower's roof beyond the shaft, m (0: the roof
    /// ends at the wall). The first tier runs on at its own pitch to the
    /// overhang line, where the eave is, `overhang · tan(pitch)` below
    /// `base_height`, as a wall's roof meets its eave at the overhang line
    /// (audit r2 UX-001). The shaft and its cut stay the wall's.
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub overhang: f64,
    /// The roof profile, bottom tier first.
    #[serde(default)]
    pub tiers: Vec<TowerTier>,
}

fn is_zero_f64(v: &f64) -> bool {
    *v == 0.0
}

fn default_sides() -> u32 {
    8
}

fn is_default_sides(v: &u32) -> bool {
    *v == 8
}

fn default_tier_material() -> u8 {
    DEFAULT_TOWER_MATERIAL
}

/// A tower tier's default material: sheet metal (material 2), what turret
/// roofs are usually covered with. [no source — common convention]
pub const DEFAULT_TOWER_MATERIAL: u8 = 2;

/// Most tiers of one tower.
pub const MAX_TIERS: usize = 24;
/// The most tiers of all towers of a job together: with the points a ring
/// is drawn with, what bounds the size of a calculation's answer (audit
/// ARCH-001: 50 towers of 24 curved tiers answered 119 MB).
pub const MAX_TIERS_TOTAL: usize = 120;
/// The most points of a ring drawn for the editor.
pub const MAX_DRAW_POINTS: usize = 64;
/// Most towers of one job.
pub const MAX_TOWERS: usize = 50;
/// Fewest and most sides of a polygonal tower.
pub const MIN_SIDES: u32 = 3;
pub const MAX_SIDES: u32 = 64;
/// Smallest and largest shaft width or depth, m.
pub const MIN_WIDTH: f64 = 0.1;
pub const MAX_WIDTH: f64 = 100.0;
/// Largest rise of one tier, m.
pub const MAX_RISE: f64 = 100.0;
/// Largest eave overhang of a tower's roof, m.
pub const MAX_OVERHANG: f64 = 10.0;

/// Angle step of arcs (circle, rounded corners) for areas and lengths:
/// 1/512 of a turn; the chord polygon is short of the arc by step²/24,
/// about 1.6e-6 of its length.
pub const FINE_ARC_STEP: f64 = PI / 256.0;
/// Chords of a curved tier's profile for areas and lengths.
pub const FINE_ARC_SUBSTEPS: usize = 128;
/// Angle step of arcs and chords of a curved tier for drawing and for the
/// shaft's cut in the roof (1/32 of a turn).
pub const DRAW_ARC_STEP: f64 = PI / 16.0;
pub const DRAW_ARC_SUBSTEPS: usize = 12;

/// The shortest edge of the shaft's cut in the roof, m (well above the
/// core's 2 cm point merge).
pub const MIN_SHAFT_EDGE: f64 = 0.1;
/// The most corners of the shaft's cut in the roof.
pub const MAX_SHAFT_CORNERS: usize = 32;

/// Heights and widths closer than this are the same, m.
const EPS: f64 = 1e-9;

/// The tower's outline at the base, in the tower's own axes around its
/// centre, counter-clockwise; with the indices of its sharp corners (where
/// the roof has hips).
#[derive(Debug, Clone, PartialEq)]
pub struct Outline {
    pub points: Vec<Point2>,
    pub sharp: Vec<usize>,
}

impl Outline {
    pub fn perimeter(&self) -> f64 {
        crate::geom::perimeter(&self.points)
    }

    pub fn area(&self) -> f64 {
        crate::geom::signed_area(&self.points).abs()
    }

    /// For each edge `i` (points `i` to `i + 1`): its length and the
    /// distance of its line from the centre.
    fn edges(&self) -> Vec<(f64, f64)> {
        let n = self.points.len();
        (0..n)
            .map(|i| {
                let a = self.points[i];
                let b = self.points[(i + 1) % n];
                let l = a.distance(b);
                let d = if l > 0.0 {
                    (a.x * b.y - a.y * b.x).abs() / l
                } else {
                    0.0
                };
                (l, d)
            })
            .collect()
    }
}

/// What a tier is: a roof (sloped or flat) or a vertical wall.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TierKind {
    Roof,
    Wall,
}

/// The profile of one tier as `(half width, height)` points from its
/// bottom to its top.
type Profile = Vec<(f64, f64)>;

impl Tower {
    /// A square tower with a pyramid roof: the editor's starting point.
    pub fn pyramid(center: Point2, width: f64, base_height: f64, rise: f64) -> Tower {
        Tower {
            label: String::new(),
            center,
            rotation_deg: 0.0,
            shape: TowerShape::Rectangle,
            width,
            depth: None,
            sides: default_sides(),
            corner_radius: 0.0,
            base_height,
            overhang: 0.0,
            tiers: vec![TowerTier {
                rise,
                top_width: 0.0,
                bulge: 0.0,
                material: DEFAULT_TOWER_MATERIAL,
            }],
        }
    }

    fn depth_or_width(&self) -> f64 {
        match self.shape {
            TowerShape::Rectangle => self.depth.unwrap_or(self.width),
            _ => self.width,
        }
    }

    /// The largest corner radius the shape allows, m.
    pub fn max_corner_radius(&self) -> f64 {
        match self.shape {
            TowerShape::Rectangle => self.width.min(self.depth_or_width()) / 2.0,
            TowerShape::Polygon => {
                let n = f64::from(self.sides.clamp(MIN_SIDES, MAX_SIDES));
                // Side length of the polygon with apothem width / 2; the
                // tangent length r / tan(θ/2) may take half of it, θ the
                // inner angle (n - 2)π / n.
                let side = self.width * (PI / n).tan();
                let half_inner = (n - 2.0) * PI / (2.0 * n);
                side / 2.0 * half_inner.tan()
            }
            TowerShape::Circle => 0.0,
        }
    }

    /// Codes and English messages of [`Tower::problems`].
    pub const PROBLEMS: [(&'static str, &'static str); 11] = [
        ("tower_nan", "a value is not a number or out of range"),
        (
            "tower_size",
            "width and depth must be between 0.1 m and 100 m",
        ),
        ("tower_sides", "a polygon needs 3 to 64 sides"),
        (
            "tower_corner_radius",
            "the corner radius must be 0 or more and fit the sides",
        ),
        ("tower_base", "the base height must be 0 m or more"),
        ("tower_no_tiers", "needs at least one roof tier"),
        ("tower_tiers", "at most 24 tiers"),
        (
            "tower_tier_size",
            "a tier rises 0 to 100 m and ends 0 to 200 m wide",
        ),
        ("tower_flat_bulge", "a flat tier cannot be curved"),
        (
            "tower_bulge",
            "a curve may be at most half its chord and must stay within its tier's heights and not cross the tower's axis",
        ),
        (
            "tower_overhang",
            "the overhang must be 0 to 10 m, its first tier must slope down outward from the shaft and its eave stay above the ground",
        ),
    ];

    /// What is wrong with the tower, as `(code, English message)`; the
    /// messages are those of [`Tower::PROBLEMS`], `tower_bulge`'s with the
    /// tier and the largest curve it takes appended.
    pub fn problems(&self) -> Vec<(&'static str, String)> {
        self.problem_list()
            .into_iter()
            .map(|(code, msg)| {
                let msg = match (code, self.bulge_limit()) {
                    ("tower_bulge", Some((k, max))) => {
                        format!("{msg}: tier {} takes a curve of at most {max:.3} m", k + 1)
                    }
                    _ => msg.to_string(),
                };
                (code, msg)
            })
            .collect()
    }

    fn problem_list(&self) -> Vec<(&'static str, &'static str)> {
        let mut out = Vec::new();
        let finite = [
            self.center.x,
            self.center.y,
            self.rotation_deg,
            self.width,
            self.depth.unwrap_or(1.0),
            self.corner_radius,
            self.base_height,
            self.overhang,
        ]
        .iter()
        .all(|v| v.is_finite() && v.abs() <= crate::model::MAX_COORD)
            && self.tiers.iter().all(|t| {
                [t.rise, t.top_width, t.bulge]
                    .iter()
                    .all(|v| v.is_finite() && v.abs() <= crate::model::MAX_COORD)
            });
        if !finite {
            out.push(("tower_nan", "a value is not a number or out of range"));
            return out;
        }
        let size_ok = |v: f64| (MIN_WIDTH..=MAX_WIDTH).contains(&v);
        if !size_ok(self.width) || !size_ok(self.depth_or_width()) {
            out.push((
                "tower_size",
                "width and depth must be between 0.1 m and 100 m",
            ));
        }
        if self.shape == TowerShape::Polygon && !(MIN_SIDES..=MAX_SIDES).contains(&self.sides) {
            out.push(("tower_sides", "a polygon needs 3 to 64 sides"));
        }
        if self.corner_radius < 0.0
            || (self.shape != TowerShape::Circle
                && self.corner_radius > self.max_corner_radius() + 1e-9)
        {
            out.push((
                "tower_corner_radius",
                "the corner radius must be 0 or more and fit the sides",
            ));
        }
        if self.base_height < 0.0 {
            out.push(("tower_base", "the base height must be 0 m or more"));
        }
        if self.tiers.is_empty() {
            out.push(("tower_no_tiers", "needs at least one roof tier"));
        }
        if self.tiers.len() > MAX_TIERS {
            out.push(("tower_tiers", "at most 24 tiers"));
        }
        for t in &self.tiers {
            if !(0.0..=MAX_RISE).contains(&t.rise)
                || !(0.0..=2.0 * MAX_WIDTH).contains(&t.top_width)
            {
                out.push((
                    "tower_tier_size",
                    "a tier rises 0 to 100 m and ends 0 to 200 m wide",
                ));
                break;
            }
        }
        if self.tiers.iter().any(|t| t.rise <= EPS && t.bulge != 0.0) {
            out.push(("tower_flat_bulge", "a flat tier cannot be curved"));
        }
        if out.is_empty() && self.bare_profiles(FINE_ARC_SUBSTEPS).is_none() {
            out.push((
                "tower_bulge",
                "a curve may be at most half its chord and must stay within its tier's heights and not cross the tower's axis",
            ));
        }
        if out.is_empty()
            && (!(0.0..=MAX_OVERHANG).contains(&self.overhang)
                || (self.overhang > 0.0 && self.eave_point().is_none()))
        {
            out.push((
                "tower_overhang",
                "the overhang must be 0 to 10 m, its first tier must slope down outward from the shaft and its eave stay above the ground",
            ));
        }
        out
    }

    /// The base outline in the tower's own axes; `step` is the angle step
    /// of arcs. `None` for a shape the numbers do not make.
    pub fn outline(&self, step: f64) -> Option<Outline> {
        let w = self.width;
        if !(w > 0.0) {
            return None;
        }
        let Some(corners) = self.corners() else {
            if self.shape != TowerShape::Circle {
                return None;
            }
            let n = ((2.0 * PI / step).ceil() as usize).max(8);
            let r = w / 2.0;
            let points = (0..n)
                .map(|i| {
                    let a = 2.0 * PI * i as f64 / n as f64 - PI / 2.0;
                    Point2::new(r * a.cos(), r * a.sin())
                })
                .collect();
            return Some(Outline {
                points,
                sharp: Vec::new(),
            });
        };
        Some(fillet(&corners, self.corner_radius.max(0.0), step))
    }

    /// The roof profile of every tier as `(half width, height)` points,
    /// curved tiers in `substeps` chords; `None` when a tier's curve is
    /// more than a semicircle or the profile crosses the axis.
    fn profiles(&self, substeps: usize) -> Option<Vec<Profile>> {
        let mut out = self.bare_profiles(substeps)?;
        if self.overhang > 0.0 {
            let eave = self.eave_point()?;
            out.first_mut()?.insert(0, eave);
        }
        Some(out)
    }

    /// The eave of a tower with an overhang, `(half width, height)`: the
    /// first tier's line (its first chord at the fine steps) carried on
    /// outward by the overhang; `None` when the first tier does not slope
    /// down outward or the eave would be below the ground.
    fn eave_point(&self) -> Option<(f64, f64)> {
        let first = self.bare_profiles(FINE_ARC_SUBSTEPS)?.into_iter().next()?;
        let ((ha, za), (hb, zb)) = (*first.first()?, *first.get(1)?);
        let (run, drop) = (ha - hb, zb - za);
        if !(run > EPS && drop >= 0.0) {
            return None;
        }
        let z = za - self.overhang * drop / run;
        (z >= 0.0 && z.is_finite()).then_some((ha + self.overhang, z))
    }

    /// [`Tower::profiles`] without the overhang.
    fn bare_profiles(&self, substeps: usize) -> Option<Vec<Profile>> {
        let mut out = Vec::with_capacity(self.tiers.len());
        let (mut h, mut z) = (self.width / 2.0, self.base_height);
        for t in &self.tiers {
            let (h1, z1) = (t.top_width / 2.0, z + t.rise);
            let pts = tier_points((h, z), (h1, z1), t.bulge, substeps)?;
            out.push(pts);
            (h, z) = (h1, z1);
        }
        Some(out)
    }

    /// The first tier whose curve [`Tower::bare_profiles`] refuses, and the
    /// largest curve of the same direction it takes, m, rounded down to
    /// the millimetre (audit r2: `tower_bulge` names the allowed value).
    fn bulge_limit(&self) -> Option<(usize, f64)> {
        let (mut h, mut z) = (self.width / 2.0, self.base_height);
        for (k, t) in self.tiers.iter().enumerate() {
            let (a, b) = ((h, z), (t.top_width / 2.0, z + t.rise));
            if tier_points(a, b, t.bulge, FINE_ARC_SUBSTEPS).is_none() {
                // A straight tier always fits: halve the gap to the curve.
                let (mut lo, mut hi) = (0.0, t.bulge.abs());
                for _ in 0..40 {
                    let mid = (lo + hi) / 2.0;
                    if tier_points(a, b, mid * t.bulge.signum(), FINE_ARC_SUBSTEPS).is_some() {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                return Some((k, (lo * 1000.0).floor() / 1000.0));
            }
            (h, z) = b;
        }
        None
    }

    /// The plan point of a point of the tower's own axes at scale `k`.
    fn place(&self, p: Point2, k: f64) -> Point2 {
        let (s, c) = self.rotation_deg.to_radians().sin_cos();
        let (x, y) = (p.x * k, p.y * k);
        Point2::new(self.center.x + x * c - y * s, self.center.y + x * s + y * c)
    }

    /// The shaft's plan outline, counter-clockwise, as it is cut out of the
    /// roof like a chimney: a convex polygon the core takes (corners at
    /// least [`MIN_SHAFT_EDGE`] apart, at most [`MAX_SHAFT_CORNERS`]) with
    /// the shaft's true plan area (scaled about the axis, so a round
    /// shaft's hole is not the inscribed polygon's, audit MATH-002/004).
    ///
    /// The polygon keeps the shaft's own symmetry (audit r2 MATH-001): a
    /// rounded corner is the same few chords between its two tangent
    /// points on every corner, so the straight sides keep their direction
    /// and the core lists them as front and back abutments (WV, WH). A
    /// corner whose whole arc is shorter than the minimum is the sharp
    /// corner; a straight side shorter than the minimum shrinks to its
    /// middle point. A circle, and a shape of many short sides, is a
    /// regular polygon with an even number of corners (at least four,
    /// audit r2 MATH-011: never a sliver of what a walk along the outline
    /// leaves).
    pub fn footprint(&self) -> Vec<Point2> {
        let Some(fine) = self.outline(FINE_ARC_STEP) else {
            return Vec::new();
        };
        let pts = self.shaft_outline();
        if pts.len() < 3 {
            return Vec::new();
        }
        let a = crate::geom::signed_area(&pts).abs();
        let k = if a > 0.0 {
            (fine.area() / a).sqrt()
        } else {
            1.0
        };
        pts.iter().map(|&p| self.place(p, k)).collect()
    }

    /// [`Tower::footprint`] in the tower's own axes, before it is scaled
    /// to the shaft's area.
    fn shaft_outline(&self) -> Vec<Point2> {
        let w = self.width;
        if !(w > 0.0) {
            return Vec::new();
        }
        // A regular polygon of at most `most` (even) corners, a flat side
        // facing -y, with corners at least the minimum apart where four
        // allow it.
        let regular = |most: usize, radius: f64| -> Vec<Point2> {
            let mut n = (most.min(MAX_SHAFT_CORNERS) / 2 * 2).max(4);
            while n > 4 && 2.0 * radius * (PI / n as f64).sin() < MIN_SHAFT_EDGE - 1e-9 {
                n -= 2;
            }
            (0..n)
                .map(|i| {
                    let a = -PI / 2.0 + PI / n as f64 * (2.0 * i as f64 - 1.0);
                    Point2::new(radius * a.cos(), radius * a.sin())
                })
                .collect()
        };
        let Some(corners) = self.corners() else {
            return regular(MAX_SHAFT_CORNERS, w / 2.0);
        };
        let n = corners.len();
        let side = |i: usize| corners[i].distance(corners[(i + 1) % n]);
        let sides_ok = (0..n).all(|i| side(i) >= MIN_SHAFT_EDGE - 1e-9);
        let many = n > MAX_SHAFT_CORNERS || (!sides_ok && n > 4);
        if many {
            let radius = corners.iter().map(|p| p.x.hypot(p.y)).fold(0.0, f64::max);
            return regular(n, radius);
        }
        let r = self.corner_radius.max(0.0);
        if r <= 0.0 {
            return corners;
        }
        let arcs: Vec<CornerArc> = (0..n)
            .map(|i| {
                corner_arc(
                    corners[(i + n - 1) % n],
                    corners[i],
                    corners[(i + 1) % n],
                    r,
                )
            })
            .collect();
        // Straight sides too short to keep: from corner i's arc to the next.
        let short: Vec<bool> = (0..n)
            .map(|i| side(i) - arcs[i].t - arcs[(i + 1) % n].t < MIN_SHAFT_EDGE - 1e-9)
            .collect();
        let merged = short.iter().filter(|&&s| s).count();
        // All corners of a regular shape (and a rectangle's) are alike:
        // one chord count for all.
        let a0 = &arcs[0];
        let chord = |m: usize| 2.0 * a0.radius * (a0.sweep / (2.0 * m as f64)).sin();
        let mut m = ((a0.sweep / DRAW_ARC_STEP).ceil() as usize).max(1);
        while m > 0
            && (chord(m) < MIN_SHAFT_EDGE - 1e-9 || n * (m + 1) - merged > MAX_SHAFT_CORNERS)
        {
            m -= 1;
        }
        if m == 0 {
            return corners;
        }
        let mut out: Vec<Point2> = Vec::with_capacity(n * (m + 1));
        for i in 0..n {
            let arc = &arcs[i];
            let from = usize::from(short[(i + n - 1) % n]);
            let to = if short[i] { m - 1 } else { m };
            for j in from..=to {
                out.push(arc.point(j as f64 / m as f64));
            }
            if short[i] {
                let (a, b) = (arc.point(1.0), arcs[(i + 1) % n].point(0.0));
                out.push(Point2::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0));
            }
        }
        out
    }

    /// The sharp corners of a rectangle or polygon in the tower's own axes,
    /// counter-clockwise; `None` for a circle or a shape the numbers do
    /// not make.
    fn corners(&self) -> Option<Vec<Point2>> {
        let w = self.width;
        match self.shape {
            TowerShape::Circle => None,
            TowerShape::Rectangle => {
                let (hx, hy) = (w / 2.0, self.depth_or_width() / 2.0);
                Some(vec![
                    Point2::new(-hx, -hy),
                    Point2::new(hx, -hy),
                    Point2::new(hx, hy),
                    Point2::new(-hx, hy),
                ])
            }
            TowerShape::Polygon => {
                if !(MIN_SIDES..=MAX_SIDES).contains(&self.sides) {
                    return None;
                }
                let n = self.sides as usize;
                let r = w / 2.0 / (PI / n as f64).cos();
                Some(
                    (0..n)
                        .map(|i| {
                            let a = -PI / 2.0 + PI / n as f64 * (2.0 * i as f64 - 1.0);
                            Point2::new(r * a.cos(), r * a.sin())
                        })
                        .collect(),
                )
            }
        }
    }

    /// The tower's highest point, m above ground.
    pub fn top_height(&self) -> f64 {
        self.base_height + self.tiers.iter().map(|t| t.rise.max(0.0)).sum::<f64>()
    }

    /// The tower's roof: every tier's area and lines, the flat top if the
    /// profile does not end in a point, and the rings to draw. `None` for a
    /// tower with problems ([`Tower::problems`]).
    pub fn calculate(&self) -> Option<TowerResult> {
        self.calculate_drawn(Detail::FULL)
    }

    /// [`Tower::calculate`] with the points to draw at `detail`; every
    /// figure is the same at any detail.
    pub fn calculate_drawn(&self, detail: Detail) -> Option<TowerResult> {
        if !self.problems().is_empty() {
            return None;
        }
        let fine = self.outline(FINE_ARC_STEP)?;
        let mut draw = self.outline(DRAW_ARC_STEP)?;
        // The hips are drawn up every sharp corner, whatever the rings.
        let hip_corners: Vec<Point2> = draw.sharp.iter().map(|&i| draw.points[i]).collect();
        // At most `detail.ring_points` points a ring: what the editor draws
        // is sent with every calculation (audit ARCH-001, r2 ARCH-003).
        let most = detail.ring_points.clamp(4, MAX_DRAW_POINTS);
        if draw.points.len() > most {
            let n = draw.points.len();
            draw.points = (0..most).map(|i| draw.points[i * n / most]).collect();
            draw.sharp.clear();
        }
        let fine_profiles = self.profiles(FINE_ARC_SUBSTEPS)?;
        let draw_profiles = self.profiles(detail.substeps.clamp(1, DRAW_ARC_SUBSTEPS))?;
        let half = self.width / 2.0;
        let edges = fine.edges();
        // A rectangle that is not square has a second pitch on the sides
        // at distance depth/2.
        let depth_half = (self.shape == TowerShape::Rectangle
            && (self.depth_or_width() - self.width).abs() > 1e-9)
            .then(|| self.depth_or_width() / 2.0);
        let perimeter = fine.perimeter();
        let plan_area = fine.area();
        let sharp_radii: Vec<f64> = fine
            .sharp
            .iter()
            .map(|&i| fine.points[i].x.hypot(fine.points[i].y))
            .collect();

        let mut tiers = Vec::with_capacity(self.tiers.len());
        for (ti, (t, prof)) in self.tiers.iter().zip(&fine_profiles).enumerate() {
            let (h0, z0) = prof[0];
            let (h1, z1) = prof[prof.len() - 1];
            let kind = if (h1 - h0).abs() <= EPS && t.bulge == 0.0 && t.rise > EPS {
                TierKind::Wall
            } else {
                TierKind::Roof
            };
            let mut area = 0.0;
            let mut depth_area = 0.0;
            let mut hip = 0.0;
            for w in prof.windows(2) {
                let (ka, za) = (w[0].0 / half, w[0].1);
                let (kb, zb) = (w[1].0 / half, w[1].1);
                let dz = zb - za;
                let dk = kb - ka;
                for &(l, d) in &edges {
                    let a = (ka + kb) / 2.0 * l * dz.hypot(dk * d);
                    area += a;
                    if depth_half.is_some_and(|hy| (d - hy).abs() < (d - half).abs()) {
                        depth_area += a;
                    }
                }
                hip += dz.hypot(dk * sharp_radii.first().copied().unwrap_or(0.0));
            }
            // Pitch of the sides at distance width/2 (all sides of a square,
            // polygon or circle; the x-facing sides of a rectangle), along
            // the tier's chord: 90° and more lean outward.
            // From the profile's ends: with an overhang the first tier
            // starts lower and wider on the same line.
            let pitch_deg = if t.rise <= EPS {
                0.0
            } else {
                ((z1 - z0).atan2(h0 - h1)).to_degrees()
            };
            let depth_sides = depth_half
                .filter(|_| kind == TierKind::Roof)
                .map(|hy| SidePitch {
                    pitch_deg: if t.rise <= EPS {
                        0.0
                    } else {
                        ((z1 - z0).atan2((h0 - h1) * hy / half)).to_degrees()
                    },
                    area: depth_area,
                });
            let hips = if kind == TierKind::Roof && t.rise > EPS {
                // All sharp corners of a regular shape are alike; a
                // rectangle's are too (they lie on its diagonals).
                fine.sharp.len()
            } else {
                0
            };
            tiers.push(TierResult {
                index: ti,
                kind,
                material: t.material,
                pitch_deg,
                area,
                depth_sides,
                bottom_height: z0,
                top_height: z1,
                bottom_perimeter: perimeter * h0 / half,
                top_perimeter: perimeter * h1 / half,
                hip_count: hips,
                hip_length: if hips > 0 { hip } else { 0.0 },
                rings: Vec::new(),
            });
        }

        // Rings to draw, from the coarse profile.
        let ring = |h: f64, z: f64| -> Vec<[f64; 3]> {
            draw.points
                .iter()
                .map(|&p| {
                    let q = self.place(p, h / half);
                    mm([q.x, q.y, z])
                })
                .collect()
        };
        for (tr, prof) in tiers.iter_mut().zip(&draw_profiles) {
            tr.rings = prof.iter().map(|&(h, z)| ring(h, z)).collect();
        }

        // Lines: eaves where a roof tier starts above a wall or the shaft,
        // pitch breaks between roof tiers of different pitch, Pult where a
        // roof tier ends against a wall above, hips at sharp corners.
        // A tier of no size (no rise, the width of the tier below) is
        // passed over: it adds no line (audit r2 MATH-010).
        let live: Vec<usize> = (0..tiers.len())
            .filter(|&i| {
                let p = &fine_profiles[i];
                let ((h0, z0), (h1, z1)) = (p[0], p[p.len() - 1]);
                !((h1 - h0).abs() <= EPS && (z1 - z0).abs() <= EPS)
            })
            .collect();
        let mut lines = Vec::new();
        for (li, &i) in live.iter().enumerate() {
            let tr = &tiers[i];
            if tr.kind != TierKind::Roof {
                continue;
            }
            let below = li.checked_sub(1).map(|j| &tiers[live[j]]);
            let (bottom_h, bottom_z) = fine_profiles[i][0];
            match below {
                None => {
                    lines.push(self.ring_line("T", i, bottom_h, bottom_z, &draw, half, perimeter))
                }
                Some(b) if b.kind == TierKind::Wall => {
                    lines.push(self.ring_line("T", i, bottom_h, bottom_z, &draw, half, perimeter))
                }
                Some(b)
                    if ((b.pitch_deg - tr.pitch_deg).abs() > 0.5 || b.area == 0.0)
                        && bottom_h > EPS =>
                {
                    lines.push(self.ring_line("c", i, bottom_h, bottom_z, &draw, half, perimeter))
                }
                _ => {}
            }
            let (top_h, top_z) = fine_profiles[i][fine_profiles[i].len() - 1];
            let above = live.get(li + 1).map(|&j| &tiers[j]);
            if top_h > EPS {
                match above {
                    Some(a) if a.kind == TierKind::Wall => {
                        lines.push(self.ring_line("P", i, top_h, top_z, &draw, half, perimeter))
                    }
                    // The flat top.
                    None if tr.rise_positive() => {
                        lines.push(self.ring_line("c", i, top_h, top_z, &draw, half, perimeter))
                    }
                    _ => {}
                }
            }
            if tr.hip_count > 0 {
                for &corner in &hip_corners {
                    let points: Vec<[f64; 3]> = draw_profiles[i]
                        .iter()
                        .map(|&(h, z)| {
                            let q = self.place(corner, h / half);
                            mm([q.x, q.y, z])
                        })
                        .collect();
                    lines.push(TowerLine {
                        code: "G",
                        tier: i,
                        length: tr.hip_length,
                        points,
                        closed: false,
                    });
                }
            }
        }

        let (last_h, last_z) = fine_profiles
            .last()
            .and_then(|p| p.last().copied())
            .unwrap_or((half, self.base_height));
        // A flat top straight on the shaft or on a vertical tier starts
        // with an eave, as a roof tier above a wall does (audit r2
        // MATH-007).
        let last_live = live.last().map(|&i| &tiers[i]);
        if last_h > EPS && last_live.is_none_or(|t| t.kind == TierKind::Wall) {
            let tier = live.last().copied().unwrap_or(0);
            lines.push(self.ring_line("T", tier, last_h, last_z, &draw, half, perimeter));
        }
        let cap = (last_h > EPS).then(|| TowerCap {
            area: plan_area * (last_h / half).powi(2),
            height: last_z,
            material: self
                .tiers
                .last()
                .map_or(DEFAULT_TOWER_MATERIAL, |t| t.material),
            ring: ring(last_h, last_z),
        });
        Some(TowerResult {
            top_height: self.top_height(),
            base_perimeter: perimeter,
            base_area: plan_area,
            tiers,
            cap,
            lines,
        })
    }

    /// A horizontal ring line at half width `h` and height `z`; its length
    /// from the fine outline.
    #[allow(clippy::too_many_arguments)]
    fn ring_line(
        &self,
        code: &'static str,
        tier: usize,
        h: f64,
        z: f64,
        draw: &Outline,
        half: f64,
        perimeter: f64,
    ) -> TowerLine {
        let points = draw
            .points
            .iter()
            .map(|&p| {
                let q = self.place(p, h / half);
                mm([q.x, q.y, z])
            })
            .collect();
        TowerLine {
            code,
            tier,
            length: perimeter * h / half,
            points,
            closed: true,
        }
    }
}

impl TierResult {
    fn rise_positive(&self) -> bool {
        self.top_height - self.bottom_height > EPS
    }
}

/// The calculated roof of a tower.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TowerResult {
    pub top_height: f64,
    /// Perimeter and plan area of the shaft, m and m².
    pub base_perimeter: f64,
    pub base_area: f64,
    pub tiers: Vec<TierResult>,
    /// The flat top where the profile does not end in a point.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cap: Option<TowerCap>,
    pub lines: Vec<TowerLine>,
}

impl TowerResult {
    /// Area of the roof tiers and the flat top, m² (no vertical walls).
    pub fn roof_area(&self) -> f64 {
        self.tiers
            .iter()
            .filter(|t| t.kind == TierKind::Roof)
            .map(|t| t.area)
            .sum::<f64>()
            + self.cap.as_ref().map_or(0.0, |c| c.area)
    }

    /// Area of the vertical tiers (a lantern's walls), m².
    pub fn wall_area(&self) -> f64 {
        self.tiers
            .iter()
            .filter(|t| t.kind == TierKind::Wall)
            .map(|t| t.area)
            .sum()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TierResult {
    pub index: usize,
    pub kind: TierKind,
    pub material: u8,
    /// Pitch of the tier's chord, degrees (over 90 leans outward).
    pub pitch_deg: f64,
    /// True area, m².
    pub area: f64,
    /// A rectangle that is not square: the pitch and area of the sides
    /// facing along its depth (at distance depth/2), which have their own
    /// pitch; `pitch_deg` is then that of the other two sides and of the
    /// rest of `area` (audit MATH-006).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depth_sides: Option<SidePitch>,
    pub bottom_height: f64,
    pub top_height: f64,
    pub bottom_perimeter: f64,
    pub top_perimeter: f64,
    /// Hips at the sharp corners: how many and the length of one.
    pub hip_count: usize,
    pub hip_length: f64,
    /// Plan rings from the tier's bottom to its top, to draw.
    pub rings: Vec<Vec<[f64; 3]>>,
}

/// The pitch and area of some sides of a tier.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct SidePitch {
    pub pitch_deg: f64,
    pub area: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TowerCap {
    pub area: f64,
    pub height: f64,
    pub material: u8,
    pub ring: Vec<[f64; 3]>,
}

/// A roof line of a tower: a closed ring (eave T, pitch break c, Pult P) or
/// a hip G up a sharp corner.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TowerLine {
    pub code: &'static str,
    /// The tier the line belongs to.
    pub tier: usize,
    /// Length, m, from the fine outline (the points are for drawing).
    pub length: f64,
    pub points: Vec<[f64; 3]>,
    pub closed: bool,
}

/// How finely a tower's rings, lines and hips are given to draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Detail {
    /// Most points of a ring.
    pub ring_points: usize,
    /// Chords of a curved tier.
    pub substeps: usize,
}

impl Detail {
    /// What the tower dialog's preview and a job of few towers get.
    pub const FULL: Detail = Detail {
        ring_points: MAX_DRAW_POINTS,
        substeps: DRAW_ARC_SUBSTEPS,
    };
    /// The coarser steps for a job whose towers would give more than
    /// [`MAX_JOB_DRAW_POINTS`].
    const STEPS: [Detail; 5] = [
        Detail::FULL,
        Detail {
            ring_points: 32,
            substeps: 6,
        },
        Detail {
            ring_points: 16,
            substeps: 4,
        },
        Detail {
            ring_points: 12,
            substeps: 2,
        },
        Detail {
            ring_points: 12,
            substeps: 1,
        },
    ];
}

/// The most points all towers of a job are given to draw with, about 25
/// bytes each in a calculation's answer (audit r2 ARCH-003: a job at the
/// tower limits answered 12 MB and froze the editor for seconds).
pub const MAX_JOB_DRAW_POINTS: usize = 20_000;

/// The points tower `t` is drawn with at `d` (rings, ring lines, hips).
fn draw_points(t: &Tower, d: Detail) -> usize {
    let outline = match t.shape {
        TowerShape::Circle => 32,
        _ if t.corner_radius > 0.0 => 64,
        _ => t.sides.min(MAX_SIDES) as usize,
    }
    .min(d.ring_points);
    let sharp = if t.corner_radius > 0.0 || t.shape == TowerShape::Circle {
        0
    } else if t.shape == TowerShape::Rectangle {
        4
    } else {
        t.sides.min(MAX_SIDES) as usize
    };
    let per_tier = |x: &TowerTier| if x.bulge != 0.0 { d.substeps + 1 } else { 2 };
    let rings: usize = t.tiers.iter().map(per_tier).sum::<usize>() + t.tiers.len() + 2;
    outline * rings + sharp * t.tiers.iter().map(per_tier).sum::<usize>()
}

/// The finest [`Detail`] at which all towers of `project` stay within
/// [`MAX_JOB_DRAW_POINTS`] (the coarsest when none does).
pub fn job_detail(project: &crate::Project) -> Detail {
    Detail::STEPS
        .iter()
        .copied()
        .find(|&d| {
            project
                .towers
                .iter()
                .map(|t| draw_points(t, d))
                .sum::<usize>()
                <= MAX_JOB_DRAW_POINTS
        })
        .unwrap_or(Detail::STEPS[Detail::STEPS.len() - 1])
}

/// Every tower's roof, in project order (`None` for a tower with
/// problems), drawn at the job's [`job_detail`].
pub fn results(project: &crate::Project) -> Vec<Option<TowerResult>> {
    let d = job_detail(project);
    project
        .towers
        .iter()
        .map(|t| t.calculate_drawn(d))
        .collect()
}

/// The towers' roof quantities, to add to the roofs': area per material,
/// eaves, pitch breaks, Pult lines and hips. Vertical tiers (a lantern's
/// walls) and the shafts are no roof area.
pub fn quantities(project: &crate::Project) -> crate::roof::RoofQuantities {
    quantities_of(&results(project))
}

/// [`quantities`] of towers already calculated.
pub fn quantities_of(results: &[Option<TowerResult>]) -> crate::roof::RoofQuantities {
    let mut q = crate::roof::RoofQuantities::default();
    for r in results.iter().flatten() {
        for t in r.tiers.iter().filter(|t| t.kind == TierKind::Roof) {
            q.roof_area += t.area;
            crate::roof::add_material_area(&mut q.area_by_material, t.material, t.area);
        }
        if let Some(c) = &r.cap {
            q.roof_area += c.area;
            crate::roof::add_material_area(&mut q.area_by_material, c.material, c.area);
        }
        for l in &r.lines {
            match l.code {
                "T" => q.eave_length += l.length,
                "c" => q.knick_length += l.length,
                "P" => q.pult_length += l.length,
                "G" => {
                    q.hip_length += l.length;
                    q.hip_count += 1;
                }
                _ => {}
            }
        }
    }
    q
}

/// The towers' roof area per pitch (chord pitch of each tier, 0° for a
/// flat top), unrounded.
pub fn area_by_pitch(project: &crate::Project) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    for r in results(project).iter().flatten() {
        for t in r.tiers.iter().filter(|t| t.kind == TierKind::Roof) {
            match t.depth_sides {
                Some(d) => {
                    out.push((t.pitch_deg, t.area - d.area));
                    out.push((d.pitch_deg, d.area));
                }
                None => out.push((t.pitch_deg, t.area)),
            }
        }
        if let Some(c) = &r.cap {
            out.push((0.0, c.area));
        }
    }
    out
}

/// The towers' line lengths by DACAD code.
pub fn line_totals(project: &crate::Project) -> Vec<(&'static str, f64)> {
    results(project)
        .iter()
        .flatten()
        .flat_map(|r| r.lines.iter().map(|l| (l.code, l.length)))
        .collect()
}

/// The towers' line lengths per material: a line counts for the material
/// of the tier it belongs to and, for a pitch break, also for the one of
/// the tier below.
pub fn lines_by_material(project: &crate::Project) -> Vec<(u8, &'static str, f64)> {
    let mut out = Vec::new();
    for (t, r) in project.towers.iter().zip(results(project)) {
        let Some(r) = r else { continue };
        let m = |i: usize| {
            crate::roof::counted_material(
                t.tiers
                    .get(i)
                    .map_or(DEFAULT_TOWER_MATERIAL, |x| x.material),
            )
        };
        for l in &r.lines {
            let own = m(l.tier);
            out.push((own, l.code, l.length));
            if l.code == "c"
                && l.tier > 0
                && r.tiers.get(l.tier).is_some_and(|x| {
                    (x.bottom_height - l.points.first().map_or(0.0, |p| p[2])).abs() < 1e-9
                })
            {
                let below = m(l.tier - 1);
                if below != own {
                    out.push((below, l.code, l.length));
                }
            }
        }
    }
    out
}

/// The height of the roof at a plan point: the highest face that holds
/// it, `None` beside the roofs.
pub fn roof_height_at(geos: &[crate::roof::RoofGeometry], p: Point2) -> Option<f64> {
    roof_height_near(geos, p, p)
}

/// The height at `at` of the roof face that holds `probe` (the highest of
/// them), `None` when no face holds it: the plane of the face just outside
/// a shaft, read on the shaft's edge (audit r2 MATH-009).
fn roof_height_near(geos: &[crate::roof::RoofGeometry], probe: Point2, at: Point2) -> Option<f64> {
    let mut best: Option<f64> = None;
    for f in geos.iter().flat_map(|g| &g.faces) {
        if f.polygon.len() < 3 || f.pitch_deg >= 89.9 {
            continue;
        }
        let plan: Vec<Point2> = f.polygon.iter().map(|v| Point2::new(v[0], v[1])).collect();
        if !point_in(&plan, probe) {
            continue;
        }
        // The face's plane from Newell's normal.
        let (mut nx, mut ny, mut nz) = (0.0, 0.0, 0.0);
        let n = f.polygon.len();
        for i in 0..n {
            let (a, b) = (f.polygon[i], f.polygon[(i + 1) % n]);
            nx += (a[1] - b[1]) * (a[2] + b[2]);
            ny += (a[2] - b[2]) * (a[0] + b[0]);
            nz += (a[0] - b[0]) * (a[1] + b[1]);
        }
        if nz.abs() < 1e-12 {
            continue;
        }
        let o = f.polygon[0];
        let z = o[2] - (nx * (at.x - o[0]) + ny * (at.y - o[1])) / nz;
        if z.is_finite() {
            best = Some(best.map_or(z, |b: f64| b.max(z)));
        }
    }
    best
}

fn point_in(poly: &[Point2], p: Point2) -> bool {
    let mut inside = false;
    let n = poly.len();
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (poly[i], poly[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Where a tower's shaft meets the roofs: the roof height at each corner of
/// its footprint (just outside it; the ground, 0 m, beside the roofs) and
/// the area of the shaft's walls from there up to the tower's base.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Shaft {
    /// The footprint's corners where the shaft leaves the roof or the
    /// ground.
    pub bottom: Vec<[f64; 3]>,
    /// Area of the shaft's walls above the roof, m².
    pub wall_area: f64,
    /// Whether the shaft stands in a roof at all.
    pub on_roof: bool,
    /// Whether some of the roof around it is higher than the tower's base.
    pub base_below_roof: bool,
    /// The shaft stands in a building whose roof is not calculated (an
    /// error, or input the calculation left out): it is not beside the
    /// roofs, and its walls have no figure (`wall_area` is 0) until that
    /// roof is (audit ERR-005).
    pub roof_failed: bool,
}

/// Whether the shaft `fp` (or the axis `centre`) reaches a part of
/// `project` with roofed walls that has no roof face in `geos` (same order
/// as `project.parts`): its roof failed or was left out. Reaching is a
/// point within the walls or within the part's largest overhang of them
/// (audit r2 ERR-003: a turret at the wall line with its axis outside).
fn in_failed_building(
    project: &crate::Project,
    geos: &[crate::roof::RoofGeometry],
    centre: Point2,
    fp: &[Point2],
) -> bool {
    project.parts.iter().enumerate().any(|(i, part)| {
        let roofed = part.edges.iter().any(|e| !e.is_gable());
        let failed = geos
            .get(i)
            .is_none_or(|g| g.error.is_some() || g.faces.is_empty());
        if !(roofed && failed && part.edges.len() >= 3) {
            return false;
        }
        let outline = part.outline();
        let reach = part
            .edges
            .iter()
            .map(|e| e.overhang)
            .filter(|o| o.is_finite())
            .fold(0.0, f64::max)
            + 1e-6;
        let shaft: Vec<Point2> = std::iter::once(centre).chain(fp.iter().copied()).collect();
        shaft
            .iter()
            .any(|&p| point_in(&outline, p) || distance_to_outline(&outline, p) <= reach)
            // A shaft over a whole corner of the building.
            || (fp.len() >= 3 && outline.iter().any(|&c| point_in(fp, c)))
    })
}

/// The plan distance of `p` from the closed polygon `poly`'s edges.
fn distance_to_outline(poly: &[Point2], p: Point2) -> f64 {
    let n = poly.len();
    (0..n)
        .map(|i| {
            let (a, b) = (poly[i], poly[(i + 1) % n]);
            let (dx, dy) = (b.x - a.x, b.y - a.y);
            let l2 = dx * dx + dy * dy;
            let s = if l2 > 0.0 {
                (((p.x - a.x) * dx + (p.y - a.y) * dy) / l2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            p.distance(Point2::new(a.x + dx * s, a.y + dy * s))
        })
        .fold(f64::MAX, f64::min)
}

/// The shaft of tower `t` on the calculated roofs `geos` of `project`.
pub fn shaft(t: &Tower, project: &crate::Project, geos: &[crate::roof::RoofGeometry]) -> Shaft {
    let fp = t.footprint();
    let n = fp.len();
    let mut on_roof = false;
    let mut below = false;
    let mut bottom = Vec::with_capacity(n);
    for &p in &fp {
        // 5 cm outside, away from the axis: the face around the hole.
        let (dx, dy) = (p.x - t.center.x, p.y - t.center.y);
        let l = dx.hypot(dy).max(1e-9);
        let q = Point2::new(p.x + dx / l * 0.05, p.y + dy / l * 0.05);
        let z = match roof_height_near(geos, q, p) {
            Some(z) => {
                on_roof = true;
                z
            }
            None => 0.0,
        };
        if z > t.base_height + 1e-6 {
            below = true;
        }
        bottom.push([p.x, p.y, z.min(t.base_height)]);
    }
    // The walls from the roof up to the base, the roof height sampled
    // every 5 cm along each side (a ridge or hip under the shaft runs
    // between its corners: audit MATH-008): the plane of the face found
    // just outside the shaft, read on the shaft's edge.
    let height = |p: Point2| -> f64 {
        let (dx, dy) = (p.x - t.center.x, p.y - t.center.y);
        let l = dx.hypot(dy).max(1e-9);
        let q = Point2::new(p.x + dx / l * 0.05, p.y + dy / l * 0.05);
        roof_height_near(geos, q, p)
            .unwrap_or(0.0)
            .min(t.base_height)
    };
    let mut wall_area = 0.0;
    for i in 0..n {
        let (a, b) = (fp[i], fp[(i + 1) % n]);
        let len = a.distance(b);
        let k = ((len / 0.05).ceil() as usize).clamp(1, 400);
        let mut sum = 0.0;
        let mut prev = t.base_height - height(a);
        for j in 1..=k {
            let s = j as f64 / k as f64;
            let h =
                t.base_height - height(Point2::new(a.x + (b.x - a.x) * s, a.y + (b.y - a.y) * s));
            sum += (prev + h) / 2.0;
            prev = h;
        }
        wall_area += len * sum / k as f64;
    }
    let roof_failed = !on_roof && in_failed_building(project, geos, t.center, &fp);
    Shaft {
        bottom,
        wall_area: if roof_failed { 0.0 } else { wall_area },
        on_roof,
        base_below_roof: below,
        roof_failed,
    }
}

/// The rounding of corner `v` (between `prev` and `next`) by `r`, as
/// [`fillet`] makes it: the tangent length `t`, the arc's centre, radius,
/// start angle and sweep.
struct CornerArc {
    t: f64,
    centre: Point2,
    radius: f64,
    start: f64,
    sweep: f64,
}

impl CornerArc {
    /// The arc's point at `s` (0 the first tangent point, 1 the second).
    fn point(&self, s: f64) -> Point2 {
        let a = self.start + self.sweep * s;
        Point2::new(
            self.centre.x + self.radius * a.cos(),
            self.centre.y + self.radius * a.sin(),
        )
    }
}

fn corner_arc(prev: Point2, v: Point2, next: Point2, r: f64) -> CornerArc {
    let unit = |a: Point2, b: Point2| {
        let l = a.distance(b);
        Point2::new((b.x - a.x) / l, (b.y - a.y) / l)
    };
    let e1 = unit(v, prev);
    let e2 = unit(v, next);
    let cos_t = (e1.x * e2.x + e1.y * e2.y).clamp(-1.0, 1.0);
    let theta = cos_t.acos();
    let half = theta / 2.0;
    let max_t = (v.distance(prev).min(v.distance(next))) / 2.0;
    let t = (r / half.tan()).min(max_t);
    let rr = t * half.tan();
    let t1 = Point2::new(v.x + e1.x * t, v.y + e1.y * t);
    let bis = Point2::new(e1.x + e2.x, e1.y + e2.y);
    let bl = bis.x.hypot(bis.y);
    let c = Point2::new(
        v.x + bis.x / bl * rr / half.sin(),
        v.y + bis.y / bl * rr / half.sin(),
    );
    CornerArc {
        t,
        centre: c,
        radius: rr,
        start: (t1.y - c.y).atan2(t1.x - c.x),
        sweep: PI - theta,
    }
}

/// A point to draw, rounded to the millimetre: what the editor draws is
/// sent with every calculation, and 17 digits a coordinate made a job at
/// the limits answer 12 MB (audit r2 ARCH-003). Lengths and areas are
/// worked out from the exact outline, never from these points.
fn mm(p: [f64; 3]) -> [f64; 3] {
    p.map(|v| (v * 1000.0).round() / 1000.0)
}

/// Rounds the corners of a convex counter-clockwise polygon by `r`
/// (clamped to what each corner allows), arcs at `step`.
fn fillet(corners: &[Point2], r: f64, step: f64) -> Outline {
    let n = corners.len();
    if r <= 0.0 || n < 3 {
        return Outline {
            points: corners.to_vec(),
            sharp: (0..n).collect(),
        };
    }
    let mut points = Vec::new();
    for i in 0..n {
        let arc = corner_arc(
            corners[(i + n - 1) % n],
            corners[i],
            corners[(i + 1) % n],
            r,
        );
        let k = ((arc.sweep / step).ceil() as usize).max(1);
        for j in 0..=k {
            let p = arc.point(j as f64 / k as f64);
            if points.last().is_none_or(|q: &Point2| q.distance(p) > 1e-9) {
                points.push(p);
            }
        }
    }
    if points.len() > 1 && points[0].distance(points[points.len() - 1]) <= 1e-9 {
        points.pop();
    }
    Outline {
        points,
        sharp: Vec::new(),
    }
}

/// Points of a circular arc from `a` to `b` in the profile plane
/// (half width, height) whose middle lies `bulge` from the chord, positive
/// away from the axis; `None` for more than a semicircle.
/// The profile of one tier from `a` to `b` (half width, height) with
/// `bulge`; `None` when the curve does not fit. Never across the axis, and
/// never above or below the tier's own heights: a dome would end in a
/// dimple over its top, a bell dip into the shaft (audit MATH-007).
fn tier_points(a: (f64, f64), b: (f64, f64), bulge: f64, substeps: usize) -> Option<Profile> {
    let pts = if bulge == 0.0 {
        vec![a, b]
    } else {
        arc_points(a, b, bulge, substeps)?
    };
    let (lo, hi) = (a.1.min(b.1) - 1e-6, a.1.max(b.1) + 1e-6);
    if pts.iter().any(|&(ph, pz)| ph < -1e-9 || pz < lo || pz > hi) {
        return None;
    }
    Some(pts.into_iter().map(|(ph, pz)| (ph.max(0.0), pz)).collect())
}

fn arc_points(a: (f64, f64), b: (f64, f64), bulge: f64, substeps: usize) -> Option<Profile> {
    let (dh, dz) = (b.0 - a.0, b.1 - a.1);
    let l = dh.hypot(dz);
    if !(l > EPS) || bulge.abs() > l / 2.0 + 1e-9 {
        return None;
    }
    // Normal of the chord pointing away from the axis (positive half
    // width): the chord rises, so (dz, -dh) has a positive first part.
    let (nh, nz) = (dz / l, -dh / l);
    let mid = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
    let apex = (mid.0 + nh * bulge, mid.1 + nz * bulge);
    let s = bulge.abs().min(l / 2.0);
    let r = (l * l / 4.0 + s * s) / (2.0 * s);
    let sign = bulge.signum();
    let c = (apex.0 - nh * sign * r, apex.1 - nz * sign * r);
    let a0 = (a.1 - c.1).atan2(a.0 - c.0);
    let sweep = 4.0 * (2.0 * s / l).atan();
    // Turn the way that passes the apex half way.
    let am = (apex.1 - c.1).atan2(apex.0 - c.0);
    let dir = {
        let ccw = a0 + sweep / 2.0;
        let d = (ccw - am).rem_euclid(2.0 * PI);
        if d < 1e-6 || (2.0 * PI - d) < 1e-6 {
            1.0
        } else {
            -1.0
        }
    };
    let mut pts: Profile = (0..=substeps)
        .map(|i| {
            let t = a0 + dir * sweep * i as f64 / substeps as f64;
            (c.0 + r * t.cos(), c.1 + r * t.sin())
        })
        .collect();
    // Exact ends.
    pts[0] = a;
    pts[substeps] = b;
    Some(pts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(width: f64, tiers: Vec<TowerTier>) -> Tower {
        Tower {
            tiers,
            ..Tower::pyramid(Point2::new(10.0, 5.0), width, 12.0, 1.0)
        }
    }

    fn tier(rise: f64, top_width: f64, bulge: f64) -> TowerTier {
        TowerTier {
            rise,
            top_width,
            bulge,
            material: 2,
        }
    }

    fn close(a: f64, b: f64, rel: f64) -> bool {
        (a - b).abs() <= rel * b.abs().max(1e-12)
    }

    #[test]
    fn a_square_pyramid_has_four_triangles_and_four_hips() {
        // Square a = 4, height h = 3: each triangle has base 4 and slant
        // height sqrt(3² + 2²); hips sqrt(3² + 2·2²).
        let t = square(4.0, vec![tier(3.0, 0.0, 0.0)]);
        let r = t.calculate().unwrap();
        let slant = (9.0f64 + 4.0).sqrt();
        assert!(close(r.roof_area(), 4.0 * 0.5 * 4.0 * slant, 1e-12));
        let tr = &r.tiers[0];
        assert_eq!(tr.hip_count, 4);
        assert!(close(tr.hip_length, (9.0f64 + 8.0).sqrt(), 1e-12));
        assert!(close(tr.pitch_deg, 3.0f64.atan2(2.0).to_degrees(), 1e-12));
        assert!(r.cap.is_none());
        let eave: f64 = r
            .lines
            .iter()
            .filter(|l| l.code == "T")
            .map(|l| l.length)
            .sum();
        assert!(close(eave, 16.0, 1e-12));
        assert_eq!(r.lines.iter().filter(|l| l.code == "G").count(), 4);
        assert!(close(r.top_height, 15.0, 1e-12));
    }

    #[test]
    fn a_cone_matches_pi_r_s() {
        let t = Tower {
            shape: TowerShape::Circle,
            ..square(4.0, vec![tier(5.0, 0.0, 0.0)])
        };
        let r = t.calculate().unwrap();
        let exact = PI * 2.0 * (25.0f64 + 4.0).sqrt();
        assert!(
            close(r.roof_area(), exact, 5e-5),
            "{} vs {exact}",
            r.roof_area()
        );
        assert!(r.lines.iter().all(|l| l.code != "G"));
        let eave: f64 = r
            .lines
            .iter()
            .filter(|l| l.code == "T")
            .map(|l| l.length)
            .sum();
        assert!(close(eave, PI * 4.0, 5e-5));
    }

    #[test]
    fn a_frustum_with_a_flat_top_has_a_cap_and_a_pitch_break() {
        // Square 4 → 2 over 1 m: trapezoids (4 + 2)/2 × sqrt(1 + 1), cap 4.
        let t = square(4.0, vec![tier(1.0, 2.0, 0.0)]);
        let r = t.calculate().unwrap();
        assert!(close(r.tiers[0].area, 4.0 * 3.0 * 2f64.sqrt(), 1e-12));
        assert!(close(r.cap.as_ref().unwrap().area, 4.0, 1e-12));
        assert!(close(r.roof_area(), 12.0 * 2f64.sqrt() + 4.0, 1e-12));
        let k: Vec<_> = r.lines.iter().filter(|l| l.code == "c").collect();
        assert_eq!(k.len(), 1);
        assert!(close(k[0].length, 8.0, 1e-12));
    }

    #[test]
    fn a_hemisphere_dome_matches_two_pi_r_squared() {
        // Circle of radius 2, one tier rising 2 to a point with a bulge of
        // half the chord: a quarter circle in the profile, a hemisphere.
        // The quarter circle through (2, 0) and (0, 2) centred on the axis
        // stands r - r·cos 45° off its chord.
        let sagitta = 2.0 - 2.0 * (PI / 4.0).cos();
        let t = Tower {
            shape: TowerShape::Circle,
            ..square(4.0, vec![tier(2.0, 0.0, sagitta)])
        };
        let r = t.calculate().unwrap();
        let exact = 2.0 * PI * 4.0;
        assert!(
            close(r.roof_area(), exact, 1e-4),
            "{} vs {exact}",
            r.roof_area()
        );
    }

    #[test]
    fn a_spherical_zone_of_an_onion_matches_archimedes() {
        // Archimedes: a spherical zone of height H on a sphere of radius R
        // has the area 2πRH. A hemisphere of radius 2 in two tiers, split
        // 30° up the sphere.
        let r0 = 2.0f64;
        let a1 = PI / 6.0; // 30° up the sphere
        let (h1, z1) = (r0 * a1.cos(), r0 * a1.sin());
        let sag = |c: f64| r0 - r0 * (c / 2.0).cos();
        let t = Tower {
            shape: TowerShape::Circle,
            base_height: 0.0,
            ..square(
                2.0 * r0,
                vec![
                    tier(z1, 2.0 * h1, sag(a1)),
                    tier(r0 - z1, 0.0, sag(PI / 2.0 - a1)),
                ],
            )
        };
        let r = t.calculate().unwrap();
        assert!(close(r.tiers[0].area, 2.0 * PI * r0 * z1, 1e-4));
        assert!(close(r.tiers[1].area, 2.0 * PI * r0 * (r0 - z1), 1e-4));
        // The chords differ, so a pitch break (a seam of the cover) is
        // listed where the tiers meet.
        assert_eq!(r.lines.iter().filter(|l| l.code == "c").count(), 1);
    }

    #[test]
    fn a_lantern_wall_is_not_roof_and_its_roof_starts_with_an_eave() {
        let t = square(
            4.0,
            vec![
                tier(1.0, 2.0, 0.0),
                tier(2.0, 2.0, 0.0),
                tier(2.0, 0.0, 0.0),
            ],
        );
        let r = t.calculate().unwrap();
        assert_eq!(r.tiers[1].kind, TierKind::Wall);
        assert!(close(r.wall_area(), 8.0 * 2.0, 1e-12));
        let codes: Vec<&str> = r
            .lines
            .iter()
            .filter(|l| l.code != "G")
            .map(|l| l.code)
            .collect();
        assert_eq!(codes, ["T", "P", "T"]);
        assert!(r.cap.is_none());
    }

    #[test]
    fn rounded_corners_shorten_the_eave_by_the_corners() {
        // Square 4 with corner radius 1: perimeter 4·(4 - 2) + 2π·1.
        let t = Tower {
            corner_radius: 1.0,
            ..square(4.0, vec![tier(3.0, 0.0, 0.0)])
        };
        let r = t.calculate().unwrap();
        assert!(close(r.base_perimeter, 8.0 + 2.0 * PI, 3e-5));
        assert!(close(r.base_area, 16.0 - 4.0 + PI, 3e-5));
        assert!(r.lines.iter().all(|l| l.code != "G"));
    }

    #[test]
    fn a_regular_polygon_pyramid_matches_its_triangles() {
        // Octagon across flats 4: side 4·tan(22.5°), apothem 2; height 3.
        let t = Tower {
            shape: TowerShape::Polygon,
            sides: 8,
            ..square(4.0, vec![tier(3.0, 0.0, 0.0)])
        };
        let r = t.calculate().unwrap();
        let side = 4.0 * (PI / 8.0).tan();
        let exact = 8.0 * 0.5 * side * (9.0f64 + 4.0).sqrt();
        assert!(close(r.roof_area(), exact, 1e-12));
        assert_eq!(r.tiers[0].hip_count, 8);
    }

    #[test]
    fn a_rectangle_tent_matches_its_four_triangles() {
        // 6 × 4, height 3 to a point: x-facing triangles base 4, slant
        // sqrt(9 + 9); y-facing base 6, slant sqrt(9 + 4).
        let t = Tower {
            depth: Some(4.0),
            ..square(6.0, vec![tier(3.0, 0.0, 0.0)])
        };
        let r = t.calculate().unwrap();
        let exact = 2.0 * 0.5 * 4.0 * 18f64.sqrt() + 2.0 * 0.5 * 6.0 * 13f64.sqrt();
        assert!(close(r.roof_area(), exact, 1e-12));
        assert!(close(
            r.tiers[0].hip_length,
            (9.0f64 + 9.0 + 4.0).sqrt(),
            1e-12
        ));
    }

    #[test]
    fn bad_towers_are_problems_not_results() {
        let ok = square(4.0, vec![tier(3.0, 0.0, 0.0)]);
        assert!(ok.problems().is_empty());
        let codes = |t: Tower| t.problems().into_iter().map(|p| p.0).collect::<Vec<_>>();
        assert_eq!(
            codes(Tower {
                width: 0.0,
                ..ok.clone()
            }),
            ["tower_size"]
        );
        assert_eq!(
            codes(Tower {
                tiers: vec![],
                ..ok.clone()
            }),
            ["tower_no_tiers"]
        );
        assert_eq!(
            codes(Tower {
                corner_radius: 2.1,
                ..ok.clone()
            }),
            ["tower_corner_radius"]
        );
        assert_eq!(
            codes(Tower {
                tiers: vec![tier(1.0, 0.0, 5.0)],
                ..ok.clone()
            }),
            ["tower_bulge"]
        );
        assert_eq!(
            codes(Tower {
                tiers: vec![tier(0.0, 2.0, 0.5)],
                ..ok.clone()
            }),
            ["tower_flat_bulge"]
        );
        assert_eq!(
            codes(Tower {
                base_height: f64::NAN,
                ..ok.clone()
            }),
            ["tower_nan"]
        );
        // A bell curving inward past the axis.
        assert_eq!(
            codes(Tower {
                tiers: vec![tier(4.0, 0.2, -2.0)],
                ..ok.clone()
            }),
            ["tower_bulge"]
        );
        assert!(Tower { width: 0.0, ..ok }.calculate().is_none());
    }

    /// Audit r2: a curve too large names its tier and the largest curve
    /// the tier takes, and that value is taken while a little more is not.
    #[test]
    fn a_curve_too_large_names_the_largest_it_may_be() {
        let ok = square(4.0, vec![tier(3.0, 0.0, 0.0)]);
        for bulge in [5.0, -2.0, 0.9] {
            let t = Tower {
                tiers: vec![tier(1.0, 2.0, 0.0), tier(2.0, 0.2, bulge)],
                ..ok.clone()
            };
            let probs = t.problems();
            let Some((_, msg)) = probs.iter().find(|p| p.0 == "tower_bulge") else {
                assert!(bulge.abs() < 1.0, "{bulge}: {probs:?}");
                continue;
            };
            let (_, max) = msg
                .rsplit_once("tier 2 takes a curve of at most ")
                .unwrap_or_else(|| panic!("{msg}"));
            let max: f64 = max.trim_end_matches(" m").parse().unwrap();
            assert!(max > 0.0 && max < bulge.abs(), "{bulge}: {max}");
            let with = |b: f64| Tower {
                tiers: vec![tier(1.0, 2.0, 0.0), tier(2.0, 0.2, b * bulge.signum())],
                ..ok.clone()
            };
            assert!(with(max).problems().is_empty(), "{bulge}: {max}");
            assert!(!with(max + 0.002).problems().is_empty(), "{bulge}: {max}");
        }
    }

    #[test]
    fn the_footprint_is_the_rotated_base_outline() {
        let t = Tower {
            rotation_deg: 90.0,
            depth: Some(2.0),
            ..square(4.0, vec![tier(3.0, 0.0, 0.0)])
        };
        let f = t.footprint();
        assert_eq!(f.len(), 4);
        // Width 4 along the tower's x axis, turned 90°: 2 along x, 4 along y.
        let xs: Vec<f64> = f.iter().map(|p| p.x).collect();
        let ys: Vec<f64> = f.iter().map(|p| p.y).collect();
        let span = |v: &[f64]| {
            v.iter().cloned().fold(f64::MIN, f64::max) - v.iter().cloned().fold(f64::MAX, f64::min)
        };
        assert!(close(span(&xs), 2.0, 1e-12));
        assert!(close(span(&ys), 4.0, 1e-12));
        assert!(crate::geom::signed_area(&f) > 0.0);
    }
}
