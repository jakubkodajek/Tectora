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
                    (by(&q, SHEET) - sheet).abs() < 1e-6,
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

/// A wall split by an extra corner into two walls in line, one half in
/// sheet metal: the calculation joins walls in line with the same eave
/// height and overhang into one plane (DACAD's collinear merge, which does
/// not look at the material), so the plane has the first wall's material.
/// That must never pass silently: the roof carries `merged_materials`
/// naming both walls.
#[test]
fn walls_in_line_with_two_materials_are_reported() {
    let mut p = Project::new("split wall");
    p.parts
        .push(standard_roof(StandardRoof::Hip, 12.0, 8.0, 35.0, 3.0, 0.5));
    let front = p.parts[0]
        .edges
        .iter()
        .position(|e| e.start.y.abs() < 1e-9 && e.start.x.abs() < 1e-9)
        .unwrap();
    let mut half = p.parts[0].edges[front].clone();
    half.start = Point2::new(6.0, 0.0);
    p.parts[0].edges.insert(front + 1, half);

    // Same material: one plane, nothing to report.
    let g = generate_project(&p);
    assert!(
        g[0].warnings
            .iter()
            .all(|w| w.kind != WarningKind::MergedMaterials)
    );

    p.parts[0].edges[front + 1].material = SHEET;
    let g = generate_project(&p);
    let w: Vec<_> = g[0]
        .warnings
        .iter()
        .filter(|w| w.kind == WarningKind::MergedMaterials)
        .collect();
    assert_eq!(w.len(), 1, "{:?}", g[0].warnings);
    assert_eq!(w[0].edges, vec![front, front + 1]);

    // Sheet metal first: the plane is sheet metal, the tiles' wall is named.
    p.parts[0].edges[front].material = SHEET;
    p.parts[0].edges[front + 1].material = TILES;
    let g = generate_project(&p);
    let w: Vec<_> = g[0]
        .warnings
        .iter()
        .filter(|w| w.kind == WarningKind::MergedMaterials)
        .collect();
    assert_eq!(w.len(), 1, "{:?}", g[0].warnings);
    assert_eq!(w[0].edges, vec![front, front + 1]);

    // A different overhang keeps the walls apart: no merge, no report.
    p.parts[0].edges[front + 1].overhang = 0.8;
    let g = generate_project(&p);
    assert!(
        g[0].warnings
            .iter()
            .all(|w| w.kind != WarningKind::MergedMaterials)
    );
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
