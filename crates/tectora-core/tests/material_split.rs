//! Roof material per face: every face takes the material of the wall it
//! rises from, and the area per material is exactly the area of those
//! faces. Sheet metal on a wall never lands in the tiles' area and never
//! changes the roof itself. Checked on every standard and example roof,
//! with half-hips, pitch breaks, dormers, chimneys, windows, clad gables,
//! joined and separate buildings, wall by wall.

use tectora_core::check::WarningKind;
use tectora_core::edit::dormers::{DORMER_KINDS, build_dormer, default_params};
use tectora_core::examples::{EXAMPLES, StandardRoof, standard_roof};
use tectora_core::geom::Point2;
use tectora_core::openings::generate_project;
use tectora_core::quantities::roof_totals_of;
use tectora_core::{Chimney, PitchBreak, Project, RoofWindow};

const SHEET: u8 = 2;
const TILES: u8 = 1;

fn by(q: &tectora_core::roof::RoofQuantities, m: u8) -> f64 {
    q.area_by_material
        .iter()
        .find(|(k, _)| *k == m)
        .map_or(0.0, |x| x.1)
}

/// The roofs the check runs on: the examples as they are and with each
/// kind of object and wall option, and buildings side by side and joined.
fn roofs() -> Vec<(String, Project)> {
    let mut out = Vec::new();
    for (k, ex) in EXAMPLES.iter().enumerate() {
        let base = ex.project();
        out.push((ex.id.to_string(), base.clone()));
        let pts: Vec<Point2> = base.parts[0].outline();
        let x0 = pts.iter().fold(f64::MAX, |a, p| a.min(p.x));
        let x1 = pts.iter().fold(f64::MIN, |a, p| a.max(p.x));
        let y0 = pts.iter().fold(f64::MAX, |a, p| a.min(p.y));
        let mid = x0 + (x1 - x0) * 0.4;
        let e0 = &base.parts[0].edges[0];

        // A dormer on the first wall's face.
        let mut p = base.clone();
        let host = e0.eave_height + 1.2 * e0.pitch_deg.to_radians().tan();
        p.dormers.push(build_dormer(&default_params(
            DORMER_KINDS[k % DORMER_KINDS.len()],
            Point2::new(mid, y0 + 1.2),
            0.0,
            (host * 100.0).round() / 100.0 + 1.0,
        )));
        out.push((format!("{} + dormer", ex.id), p));

        // A chimney and a roof window on the first wall's face.
        let mut p = base.clone();
        let (cx, cy) = (mid, y0 + 2.0);
        p.chimneys.push(Chimney {
            corners: vec![
                Point2::new(cx, cy),
                Point2::new(cx + 0.6, cy),
                Point2::new(cx + 0.6, cy + 0.6),
                Point2::new(cx, cy + 0.6),
            ],
            height: e0.eave_height + 5.0,
        });
        p.windows.push(RoofWindow {
            x: mid + 1.5,
            y: y0 + 1.5,
            width: 0.78,
            height: 1.18,
        });
        out.push((format!("{} + chimney + window", ex.id), p));

        // Clad gables and a pitch break on every eave wall.
        let mut p = base.clone();
        for e in p.parts[0].edges.iter_mut() {
            if e.is_gable() {
                e.clad = true;
            } else if e.pitch_deg > 20.0 {
                e.breaks = vec![PitchBreak {
                    height: e.eave_height + 1.0,
                    pitch_deg: e.pitch_deg - 15.0,
                }];
            }
        }
        out.push((format!("{} + clad gables + breaks", ex.id), p));
    }

    // Two buildings side by side (each its own roof).
    let mut two = Project::new("two");
    two.parts
        .push(standard_roof(StandardRoof::Hip, 12.0, 8.0, 35.0, 3.0, 0.5));
    let mut b = standard_roof(StandardRoof::Gable, 10.0, 8.0, 38.0, 3.0, 0.3);
    for e in b.edges.iter_mut() {
        e.start.x += 20.0;
    }
    two.parts.push(b);
    out.push(("two buildings".into(), two));

    // A wing joined to a house (the multi-roof join).
    let mut joined = Project::new("joined");
    joined.parts.push(standard_roof(
        StandardRoof::Gable,
        12.0,
        8.0,
        40.0,
        3.0,
        0.5,
    ));
    let mut w = standard_roof(StandardRoof::Gable, 6.0, 5.0, 40.0, 3.0, 0.5);
    for e in w.edges.iter_mut() {
        // Turned 90° and pushed into the house's long side.
        let (x, y) = (e.start.x, e.start.y);
        e.start = Point2::new(3.0 + y, -4.0 + x);
    }
    w.normalize_ccw();
    joined.parts.push(w);
    out.push(("joined wing".into(), joined));
    out
}

/// Every wall of every roof in turn in sheet metal: its faces (and its
/// clad gable) are sheet metal, all other faces tiles; the area per
/// material adds up to the roof area, which does not change.
#[test]
fn each_wall_in_sheet_metal_moves_exactly_its_own_faces() {
    let mut checked = 0;
    for (name, base) in roofs() {
        let geos = generate_project(&base);
        // A variant the core gives no roof for (a dormer off a shed roof's
        // only face) has nothing to split.
        if geos.iter().any(|g| g.error.is_some()) {
            continue;
        }
        let all = roof_totals_of(&geos, &base);
        assert!(all.roof_area > 1.0, "{name}: no roof");
        if name == "joined wing" {
            assert!(!geos[0].joined_with.is_empty(), "the wing is not joined");
        }
        assert!(
            (by(&all, TILES) - all.roof_area).abs() < 1e-6,
            "{name}: all tiles {:?} vs {}",
            all.area_by_material,
            all.roof_area
        );
        for pi in 0..base.parts.len() {
            for wi in 0..base.parts[pi].edges.len() {
                let mut p = base.clone();
                p.parts[pi].edges[wi].material = SHEET;
                let g = generate_project(&p);
                let q = roof_totals_of(&g, &p);
                let ctx = format!("{name}, building {pi}, wall {wi}");

                // The roof is the same roof.
                assert!(
                    (q.roof_area - all.roof_area).abs() < 1e-6,
                    "{ctx}: roof area {} became {}",
                    all.roof_area,
                    q.roof_area
                );
                for (a, b) in geos.iter().zip(&g) {
                    assert_eq!(a.faces.len(), b.faces.len(), "{ctx}");
                    for (fa, fb) in a.faces.iter().zip(&b.faces) {
                        assert_eq!(fa.polygon, fb.polygon, "{ctx}");
                        assert_eq!(fa.area, fb.area, "{ctx}");
                    }
                }

                // Each face is in its own wall's material (dormer faces in
                // the dormer's, tiles here).
                let mut sheet = 0.0;
                for (gi, geo) in g.iter().enumerate() {
                    for f in &geo.faces {
                        let want = if gi == pi && f.edge == wi {
                            SHEET
                        } else {
                            TILES
                        };
                        assert_eq!(
                            f.material, want,
                            "{ctx}: face of wall {} in building {gi} ({:.2} m²)",
                            f.edge, f.area
                        );
                        if want == SHEET {
                            sheet += f.area;
                        }
                    }
                    for c in &geo.clad_faces {
                        let want = if gi == pi && c.edge == wi {
                            SHEET
                        } else {
                            TILES
                        };
                        assert_eq!(c.material, want, "{ctx}: clad gable of wall {}", c.edge);
                        if want == SHEET {
                            sheet += c.area;
                        }
                    }
                }

                // The split is exactly those faces, and adds up.
                assert!(
                    (by(&q, SHEET) - sheet).abs() < 1e-4,
                    "{ctx}: sheet metal {:?}, its faces {sheet}",
                    q.area_by_material
                );
                assert!(
                    (by(&q, TILES) - (all.roof_area - sheet)).abs() < 1e-6,
                    "{ctx}: tiles {:?}, roof {} less sheet {sheet}",
                    q.area_by_material,
                    all.roof_area
                );
                let sum: f64 = q.area_by_material.iter().map(|x| x.1).sum();
                assert!(
                    (sum - q.roof_area).abs() < 1e-6,
                    "{ctx}: {sum} vs {}",
                    q.roof_area
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 150, "{checked} walls checked");
}

/// The price-sheet list and the results agree on every material, and a
/// window or chimney on the sheet-metal face is taken off sheet metal.
#[test]
fn the_list_books_each_material_like_the_results() {
    use tectora_core::export::{ListOptions, list_summary};
    for (name, base) in roofs() {
        let mut p = base.clone();
        p.parts[0].edges[0].material = SHEET;
        let q = roof_totals_of(&generate_project(&p), &p);
        let s = list_summary(&p, &ListOptions::new("", "21.11.2026 13:00"));
        for m in [TILES, SHEET] {
            let list = s
                .materials
                .iter()
                .find(|x| x.material == m)
                .map_or(0.0, |x| x.area);
            // The list reads areas back as float32, as the core's file
            // has them (`export::sol_value`): a few mm² at most.
            assert!(
                (list - by(&q, m)).abs() < 1e-3,
                "{name}: material {m} list {list} results {:?}",
                q.area_by_material
            );
        }
    }
}

/// A window on the sheet-metal face reduces sheet metal, not tiles.
#[test]
fn a_window_on_sheet_metal_is_taken_off_sheet_metal() {
    let mut p = Project::new("w");
    p.parts.push(standard_roof(
        StandardRoof::Gable,
        12.0,
        8.0,
        40.0,
        3.0,
        0.0,
    ));
    let front = p.parts[0]
        .edges
        .iter()
        .position(|e| !e.is_gable() && e.start.y.abs() < 1e-9)
        .unwrap();
    p.parts[0].edges[front].material = SHEET;
    let before = roof_totals_of(&generate_project(&p), &p);
    p.windows.push(RoofWindow {
        x: 5.0,
        y: 1.5,
        width: 1.0,
        height: 1.0,
    });
    let after = roof_totals_of(&generate_project(&p), &p);
    assert!((by(&before, TILES) - by(&after, TILES)).abs() < 1e-9);
    assert!(
        (by(&before, SHEET) - by(&after, SHEET) - 1.0).abs() < 1e-6,
        "{:?} -> {:?}",
        before.area_by_material,
        after.area_by_material
    );
}

/// Hip roof 12 × 8 m, 35°, 0.5 m overhang, its front wall split by an
/// extra corner at x = 3 into two walls in line. The core roofs the two as
/// one plane (DACAD's collinear merge does not look at the material).
fn front_split_at_3() -> (Project, usize) {
    let mut p = Project::new("split wall");
    p.parts
        .push(standard_roof(StandardRoof::Hip, 12.0, 8.0, 35.0, 3.0, 0.5));
    let front = p.parts[0]
        .edges
        .iter()
        .position(|e| e.start.y.abs() < 1e-9 && e.start.x.abs() < 1e-9)
        .unwrap();
    let mut half = p.parts[0].edges[front].clone();
    half.start = Point2::new(3.0, 0.0);
    p.parts[0].edges.insert(front + 1, half);
    (p, front)
}

fn merged_warnings(g: &[tectora_core::roof::RoofGeometry]) -> usize {
    g.iter()
        .flat_map(|g| &g.warnings)
        .filter(|w| w.kind == WarningKind::MergedMaterials)
        .count()
}

/// One part of the front wall in sheet metal: the front plane is split
/// square to the wall at the corner (Jakub, 2026-11-21). In plan the front
/// face is the trapezoid from the eave at y = −0.5 (x −0.5 … 12.5) to the
/// ridge at y = 4 (x 4 … 8), its left hip on y = x, so the strip left of
/// x = 3 is the triangle ∫(x + 0.5) dx from −0.5 to 3 = 3.5²/2 = 6.125 m²
/// and the rest 17/2 · 4.5 − 6.125 = 32.125 m², each over cos 35°. The
/// roof, its area and its lines' lengths stay the same; the eave is split
/// at the corner, 3.5 m and 9.5 m.
#[test]
fn walls_in_line_with_two_materials_split_the_plane_per_wall() {
    let (mut p, front) = front_split_at_3();
    let base = generate_project(&p);
    let all = roof_totals_of(&base, &p);
    assert_eq!(merged_warnings(&base), 0);
    // The core reads the walls as float32: closed forms agree to 1e-4.
    let sec = 1.0 / 35f64.to_radians().cos();
    let (left, right) = (6.125 * sec, 32.125 * sec);

    for (left_mat, right_mat) in [(TILES, SHEET), (SHEET, TILES)] {
        p.parts[0].edges[front].material = left_mat;
        p.parts[0].edges[front + 1].material = right_mat;
        let g = generate_project(&p);
        let ctx = format!("left {left_mat}, right {right_mat}");
        assert!(g[0].error.is_none(), "{ctx}");
        assert_eq!(merged_warnings(&g), 0, "{ctx}: {:?}", g[0].warnings);
        let q = roof_totals_of(&g, &p);
        assert!((q.roof_area - all.roof_area).abs() < 1e-9, "{ctx}");
        let sheet = if left_mat == SHEET { left } else { right };
        assert!(
            (by(&q, SHEET) - sheet).abs() < 1e-4,
            "{ctx}: sheet {} want {sheet}",
            by(&q, SHEET)
        );
        assert!((by(&q, TILES) + by(&q, SHEET) - q.roof_area).abs() < 1e-9);
        // Each wall has its own face now, with its material.
        let on = |w: usize| -> f64 {
            g[0].faces
                .iter()
                .filter(|f| f.edge == w)
                .map(|f| f.area)
                .sum()
        };
        assert!((on(front) - left).abs() < 1e-4, "{ctx}: {}", on(front));
        assert!((on(front + 1) - right).abs() < 1e-4, "{ctx}");
        for f in &g[0].faces {
            if f.edge == front || f.edge == front + 1 {
                assert_eq!(f.material, p.parts[0].edges[f.edge].material, "{ctx}");
                assert!(f.polygon.iter().all(|v| v.iter().all(|c| c.is_finite())));
            }
        }
        // Same lines, same lengths; the eave split at the corner.
        for code in ["T", "F", "G"] {
            let len = |g: &[tectora_core::roof::RoofGeometry]| -> f64 {
                g[0].lines
                    .iter()
                    .filter(|l| l.dacad_code() == code)
                    .map(|l| l.length())
                    .sum()
            };
            assert!((len(&g) - len(&base)).abs() < 1e-9, "{ctx} {code}");
        }
        let lines = tectora_core::quantities::lines_by_material_of(&g);
        let eave = |m: u8| {
            lines
                .iter()
                .find(|(k, _)| *k == m)
                .and_then(|(_, l)| l.get("T").copied())
                .unwrap_or(0.0)
        };
        let sheet_eave = if left_mat == SHEET { 3.5 } else { 9.5 };
        assert!((eave(SHEET) - sheet_eave).abs() < 0.011, "{ctx}: {lines:?}");
    }

    // A different overhang keeps the walls apart: the core makes two
    // planes itself, nothing to split or report.
    p.parts[0].edges[front + 1].overhang = 0.8;
    let g = generate_project(&p);
    assert_eq!(merged_warnings(&g), 0);
}

/// Windows and a chimney on the split plane: each window is taken off the
/// strip its corner lies on, a chimney across the cut by its share on
/// each side; what the faces lose in all stays what the core deducted.
#[test]
fn objects_on_a_split_plane_are_taken_off_their_own_strip() {
    let (mut p, front) = front_split_at_3();
    // A window on each strip, a chimney 0.6 × 0.6 from x 2.7 to 3.3.
    p.windows.push(RoofWindow {
        x: 1.0,
        y: 0.0,
        width: 0.78,
        height: 1.18,
    });
    p.windows.push(RoofWindow {
        x: 6.0,
        y: 0.5,
        width: 0.78,
        height: 1.18,
    });
    p.chimneys.push(Chimney {
        corners: vec![
            Point2::new(2.7, 2.0),
            Point2::new(3.3, 2.0),
            Point2::new(3.3, 2.6),
            Point2::new(2.7, 2.6),
        ],
        height: 9.0,
    });
    let g0 = generate_project(&p);
    let mut sheet = p.clone();
    sheet.parts[0].edges[front].material = SHEET;
    let g = generate_project(&sheet);
    assert!(g[0].error.is_none());
    assert_eq!(merged_warnings(&g), 0, "{:?}", g[0].warnings);
    let q0 = roof_totals_of(&g0, &p);
    let q = roof_totals_of(&g, &sheet);
    assert!((q.roof_area - q0.roof_area).abs() < 1e-9);

    let sec = 1.0 / 35f64.to_radians().cos();
    let window = 0.78 * 1.18;
    // The chimney's half left of x = 3 (0.3 × 0.6 in plan) is on the left
    // strip (under the hip y = x there).
    let chimney_left = 0.3 * 0.6 * sec;
    let all_ded: f64 = g0[0].faces.iter().map(|f| f.deducted).sum();
    let split_ded: f64 = g[0].faces.iter().map(|f| f.deducted).sum();
    assert!(
        (split_ded - all_ded).abs() < 1e-9,
        "{split_ded} vs {all_ded}"
    );
    let ded: f64 = g[0]
        .faces
        .iter()
        .filter(|f| f.edge == front)
        .map(|f| f.deducted)
        .sum();
    assert!(
        (ded - (window + chimney_left)).abs() < 2e-3,
        "left strip deducted {ded}"
    );
    assert!(
        (by(&q, SHEET) - (6.125 * sec - window - chimney_left)).abs() < 2e-3,
        "sheet {}",
        by(&q, SHEET)
    );
    // Each window cut names the face it now lies on.
    let edges: Vec<usize> = g[0]
        .window_cuts
        .iter()
        .map(|w| g[0].faces[w.face].edge)
        .collect();
    assert_eq!(edges, vec![front, front + 1]);
}

/// A dormer in sheet metal: its own faces are sheet metal, the roof it
/// stands on stays tiles, and the split adds up.
#[test]
fn a_sheet_metal_dormer_books_its_faces_as_sheet_metal() {
    use tectora_core::edit::dormers::DormerKind;
    let mut p = Project::new("dormer");
    p.parts.push(standard_roof(
        StandardRoof::Gable,
        12.0,
        8.0,
        40.0,
        3.0,
        0.0,
    ));
    let mut params = default_params(DormerKind::Gable, Point2::new(5.0, 1.2), 0.0, 5.0);
    params.material = SHEET;
    p.dormers.push(build_dormer(&params));
    let g = generate_project(&p);
    assert!(g[0].error.is_none());
    let q = roof_totals_of(&g, &p);
    let dormer: f64 = g[0]
        .faces
        .iter()
        .filter(|f| f.edge >= 4)
        .map(|f| f.area)
        .sum();
    assert!(
        dormer > 0.5,
        "{:?}",
        g[0].faces
            .iter()
            .map(|f| (f.edge, f.material))
            .collect::<Vec<_>>()
    );
    for f in &g[0].faces {
        let want = if f.edge >= 4 { SHEET } else { TILES };
        assert_eq!(f.material, want, "face of wall {}", f.edge);
    }
    assert!(
        (by(&q, SHEET) - dormer).abs() < 1e-6,
        "{:?}",
        q.area_by_material
    );
    assert!((by(&q, TILES) + by(&q, SHEET) - q.roof_area).abs() < 1e-6);
}

/// A gable roof 12 × 8 m, 40°, 0.5 m overhang, its front wall split at
/// x = 4 and x = 8 with the middle wall in sheet metal: the strip over the
/// middle wall is the sheet metal, 4 m wide from the eave to the ridge,
/// 4 × 4.5 / cos 40° in all, also with a pitch break on the three walls
/// (two faces, each split).
#[test]
fn a_middle_wall_in_sheet_metal_takes_its_strip_up_to_the_ridge() {
    for with_break in [false, true] {
        let mut p = Project::new("three in line");
        p.parts.push(standard_roof(
            StandardRoof::Gable,
            12.0,
            8.0,
            40.0,
            3.0,
            0.5,
        ));
        let front = p.parts[0]
            .edges
            .iter()
            .position(|e| e.start.y.abs() < 1e-9 && e.start.x.abs() < 1e-9)
            .unwrap();
        if with_break {
            p.parts[0].edges[front].breaks = vec![PitchBreak {
                height: 4.5,
                pitch_deg: 25.0,
            }];
        }
        for x in [8.0, 4.0] {
            let mut e = p.parts[0].edges[front].clone();
            e.start = Point2::new(x, 0.0);
            p.parts[0].edges.insert(front + 1, e);
        }
        let g0 = generate_project(&p);
        let q0 = roof_totals_of(&g0, &p);
        p.parts[0].edges[front + 1].material = SHEET;
        let g = generate_project(&p);
        let q = roof_totals_of(&g, &p);
        let ctx = format!("break {with_break}");
        assert!(g[0].error.is_none(), "{ctx}");
        assert_eq!(merged_warnings(&g), 0, "{ctx}: {:?}", g[0].warnings);
        assert!((q.roof_area - q0.roof_area).abs() < 1e-9, "{ctx}");
        // The front plane(s): the sheet metal share is 4 of the 13 m.
        let front_area: f64 = g0[0]
            .faces
            .iter()
            .filter(|f| f.edge == front)
            .map(|f| f.area)
            .sum();
        assert!(
            (by(&q, SHEET) - front_area * 4.0 / 13.0).abs() < 1e-4,
            "{ctx}: sheet {} of {front_area}",
            by(&q, SHEET)
        );
        if !with_break {
            let want = 4.0 * 4.5 / 40f64.to_radians().cos();
            assert!((by(&q, SHEET) - want).abs() < 1e-4, "{ctx}");
        }
        let sheet_faces = g[0].faces.iter().filter(|f| f.material == SHEET).count();
        assert_eq!(sheet_faces, if with_break { 2 } else { 1 }, "{ctx}");
        assert!(
            g[0].faces
                .iter()
                .filter(|f| f.material == SHEET)
                .all(|f| f.edge == front + 1)
        );
    }
}

/// A roof with faces on the given walls only, for the warning alone.
fn faces_on(walls: &[usize]) -> tectora_core::roof::RoofGeometry {
    tectora_core::roof::RoofGeometry {
        faces: walls
            .iter()
            .map(|&w| tectora_core::roof::RoofFace {
                edge: w,
                pitch_deg: 35.0,
                polygon: vec![[0.0, 0.0, 3.0], [1.0, 0.0, 3.0], [0.0, 1.0, 4.0]],
                area: 1.0,
                deducted: 0.0,
                material: 1,
            })
            .collect(),
        ..Default::default()
    }
}

/// The warning (the fallback where the split cannot be made) sees the
/// pair of the last and the first wall too, and walls that differ in
/// cladding, eave height or overhang are not one plane.
#[test]
fn the_merged_materials_warning_checks_every_pair_and_every_condition() {
    use tectora_core::check::merged_materials;
    // Hip 12 × 8 with the front wall split at x = 3, turned so the split
    // pair is the last wall and the first.
    let (p, front) = front_split_at_3();
    let mut part = p.parts[0].clone();
    let n = part.edges.len();
    part.edges.rotate_left(front + 1);
    let (last, first) = (n - 1, 0);
    assert!((part.edges[first].start.x - 3.0).abs() < 1e-9);
    part.edges[first].material = SHEET;

    let w = merged_materials(&part, &faces_on(&[last, 1, 2, 3]));
    assert_eq!(w.len(), 1, "{w:?}");
    assert_eq!(w[0].edges, vec![last, first]);
    let w = merged_materials(&part, &faces_on(&[first, 1, 2, 3]));
    assert_eq!(w[0].edges, vec![first, last]);
    // Both walls have faces: nothing lost.
    assert!(merged_materials(&part, &faces_on(&[last, first, 1, 2, 3])).is_empty());

    for change in ["clad", "eave", "overhang", "gable"] {
        let mut q = part.clone();
        let e = &mut q.edges[first];
        match change {
            "clad" => e.clad = !e.clad,
            "eave" => e.eave_height += 0.2,
            "overhang" => e.overhang += 0.2,
            _ => e.pitch_deg = 90.0,
        }
        assert!(
            merged_materials(&q, &faces_on(&[last, 1, 2, 3])).is_empty(),
            "{change}"
        );
    }
}

/// The split works across the place where the wall loop closes: the split
/// pair is the last wall and the first.
#[test]
fn a_plane_over_the_last_and_the_first_wall_is_split_too() {
    let (mut p, front) = front_split_at_3();
    let n = p.parts[0].edges.len();
    p.parts[0].edges.rotate_left(front + 1);
    let (last, first) = (n - 1, 0);
    p.parts[0].edges[last].material = SHEET;
    let g = generate_project(&p);
    assert!(g[0].error.is_none());
    assert_eq!(merged_warnings(&g), 0, "{:?}", g[0].warnings);
    let q = roof_totals_of(&g, &p);
    let sec = 1.0 / 35f64.to_radians().cos();
    // The last wall is the part from x = 0 to 3: the 6.125 m² triangle.
    assert!(
        (by(&q, SHEET) - 6.125 * sec).abs() < 1e-4,
        "{:?}",
        q.area_by_material
    );
    assert!(g[0].faces.iter().any(|f| f.edge == first));
}
