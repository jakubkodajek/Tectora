//! Cross-checks every figure Tectora reports for a roof against an
//! independent computation and against the other places that show the same
//! figure (results, breakdown, price-sheet list, stored result).
//!
//! Usage: `verify_calcs [--random N] [FILE.tectora|FILE.json|DIR]...`
//! Prints one line per finding and a summary per check.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use geo::{Area, BooleanOps, Coord, LineString, MultiPolygon, Polygon};
use tectora_core::Project;
use tectora_core::geom::Point2;
use tectora_core::model::{Chimney, HalfHip, PitchBreak, RoofPart, RoofWindow};
use tectora_core::roof::RoofFace;

struct Stats {
    runs: BTreeMap<&'static str, (usize, usize, f64)>,
    shown: BTreeMap<&'static str, usize>,
}

impl Stats {
    fn check(&mut self, name: &'static str, case: &str, dev: f64, tol: f64, what: String) {
        let e = self.runs.entry(name).or_default();
        e.0 += 1;
        if dev.is_nan() || dev.abs() > tol {
            e.1 += 1;
            e.2 = e.2.max(if dev.is_nan() {
                f64::INFINITY
            } else {
                dev.abs()
            });
            let s = self.shown.entry(name).or_default();
            if *s
                < std::env::var("VERIFY_SHOW")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(8)
            {
                *s += 1;
                println!("FAIL {name} [{case}] dev {dev:.6}: {what}");
            }
        }
    }
}

fn area3(poly: &[[f64; 3]]) -> f64 {
    let n = poly.len();
    let mut nrm = [0.0f64; 3];
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        nrm[0] += a[1] * b[2] - a[2] * b[1];
        nrm[1] += a[2] * b[0] - a[0] * b[2];
        nrm[2] += a[0] * b[1] - a[1] * b[0];
    }
    0.5 * (nrm[0] * nrm[0] + nrm[1] * nrm[1] + nrm[2] * nrm[2]).sqrt()
}

fn normal(poly: &[[f64; 3]]) -> [f64; 3] {
    let n = poly.len();
    let mut nrm = [0.0f64; 3];
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        nrm[0] += a[1] * b[2] - a[2] * b[1];
        nrm[1] += a[2] * b[0] - a[0] * b[2];
        nrm[2] += a[0] * b[1] - a[1] * b[0];
    }
    nrm
}

fn plan_area(poly: &[[f64; 3]]) -> f64 {
    let n = poly.len();
    let mut s = 0.0;
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        s += a[0] * b[1] - b[0] * a[1];
    }
    (s / 2.0).abs()
}

fn gpoly(pts: impl IntoIterator<Item = (f64, f64)>) -> Polygon<f64> {
    let v: Vec<Coord<f64>> = pts.into_iter().map(|(x, y)| Coord { x, y }).collect();
    Polygon::new(LineString::new(v), vec![])
}

fn face_plan(f: &RoofFace) -> Polygon<f64> {
    gpoly(f.polygon.iter().map(|p| (p[0], p[1])))
}

/// Max distance of a face's corners from the plane through its corners.
fn off_plane(poly: &[[f64; 3]]) -> f64 {
    let nrm = normal(poly);
    let l = (nrm[0] * nrm[0] + nrm[1] * nrm[1] + nrm[2] * nrm[2]).sqrt();
    if l < 1e-12 {
        return 0.0;
    }
    let c: [f64; 3] = [0, 1, 2].map(|k| poly.iter().map(|p| p[k]).sum::<f64>() / poly.len() as f64);
    poly.iter()
        .map(|p| {
            ((p[0] - c[0]) * nrm[0] + (p[1] - c[1]) * nrm[1] + (p[2] - c[2]) * nrm[2]).abs() / l
        })
        .fold(0.0, f64::max)
}

/// Height of a face's plane over a plan point.
fn z_at(f: &RoofFace, x: f64, y: f64) -> f64 {
    let nrm = normal(&f.polygon);
    let p = f.polygon[0];
    if nrm[2].abs() < 1e-12 {
        return f64::NAN;
    }
    p[2] - (nrm[0] * (x - p[0]) + nrm[1] * (y - p[1])) / nrm[2]
}

fn r2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn verify(st: &mut Stats, case: &str, p: &Project) {
    if p.check_limits().is_err() || !p.problems().is_empty() {
        return;
    }
    let geos = match std::panic::catch_unwind(|| tectora_core::openings::generate_project(p)) {
        Ok(g) => g,
        Err(_) => {
            st.check(
                "no_panic",
                case,
                1.0,
                0.0,
                "generate_project panicked".into(),
            );
            return;
        }
    };
    st.check("no_panic", case, 0.0, 0.0, String::new());
    let calculated = geos
        .iter()
        .all(|g| g.error.is_none() && !g.faces.is_empty());
    let joined = p.parts.len() > 1;
    let warned = geos
        .iter()
        .any(|g| g.warnings.iter().any(|w| !w.kind.is_notice()));

    // --- Per face: area, pitch, planarity (independent of the engine). ---
    for (gi, g) in geos.iter().enumerate() {
        for (fi, f) in g.faces.iter().enumerate() {
            st.check(
                "finite",
                case,
                if f.area.is_finite() && f.deducted.is_finite() {
                    0.0
                } else {
                    1.0
                },
                0.0,
                format!("part {gi} face {fi}"),
            );
            st.check(
                "area_not_negative",
                case,
                f.area.min(0.0),
                5e-4,
                format!("part {gi} face {fi} area {}", f.area),
            );
            if f.polygon.len() < 3 {
                st.check(
                    "face_has_outline",
                    case,
                    f.area + f.deducted,
                    1e-3,
                    format!(
                        "part {gi} face {fi} has {} corners and {} m2",
                        f.polygon.len(),
                        f.area + f.deducted
                    ),
                );
                continue;
            }
            let a3 = area3(&f.polygon);
            let gross = f.area + f.deducted;
            let nrm = normal(&f.polygon);
            let l = (nrm[0] * nrm[0] + nrm[1] * nrm[1] + nrm[2] * nrm[2]).sqrt();
            if l < 1e-9 {
                continue;
            }
            let pitch = (nrm[2].abs() / l).clamp(-1.0, 1.0).acos().to_degrees();
            let tag = format!(
                "part {gi} face {fi} (wall {}) {}{}",
                f.edge,
                if joined { "joined " } else { "" },
                if warned { "warned" } else { "" }
            );
            if f.pitch_deg < 89.9 {
                st.check(
                    "pitch_vs_plane",
                    case,
                    pitch - f.pitch_deg,
                    0.05,
                    format!("{tag}: plane {pitch:.3} reported {:.3}", f.pitch_deg),
                );
                let cos = f.pitch_deg.to_radians().cos();
                st.check(
                    "plan_over_cos",
                    case,
                    plan_area(&f.polygon) / cos - a3,
                    2e-3_f64.max(a3 * 2e-4),
                    format!(
                        "{tag}: plan/cos {:.4} 3D {a3:.4}",
                        plan_area(&f.polygon) / cos
                    ),
                );
            }
            st.check("planar", case, off_plane(&f.polygon), 0.02, tag.clone());
            let key = if joined {
                "gross_vs_polygon_joined"
            } else if warned {
                "gross_vs_polygon_warned"
            } else {
                "gross_vs_polygon"
            };
            st.check(
                key,
                case,
                gross - a3,
                2e-3_f64.max(a3 * 2e-4),
                format!("{tag}: reported gross {gross:.4} polygon {a3:.4}"),
            );
        }
    }

    // --- Deductions: windows exactly w*h, chimneys = plan cut / cos. ---
    for (gi, g) in geos.iter().enumerate() {
        let win: f64 = g.window_cuts.iter().map(|w| w.width * w.height).sum();
        let ded: f64 = g.faces.iter().map(|f| f.deducted).sum();
        let ops: f64 = g.openings.iter().map(|o| o.area).sum::<f64>() + g.other_cuts;
        st.check(
            "deducted_vs_openings",
            case,
            ded - ops,
            2e-3,
            format!("part {gi}: faces deduct {ded:.4}, openings+cuts {ops:.4}"),
        );
        if !joined && p.dormers.is_empty() && g.error.is_none() {
            // Chimneys and roof openings: their plan polygon over each face.
            let mut want = win;
            for c in p.chimneys.iter().chain(&p.roof_openings) {
                if c.corners.len() < 3 {
                    continue;
                }
                let cp = MultiPolygon::new(vec![gpoly(c.corners.iter().map(|q| (q.x, q.y)))]);
                for f in g
                    .faces
                    .iter()
                    .filter(|f| f.polygon.len() >= 3 && f.pitch_deg < 89.9)
                {
                    let cut = MultiPolygon::new(vec![face_plan(f)])
                        .intersection(&cp)
                        .unsigned_area();
                    want += cut / f.pitch_deg.to_radians().cos();
                }
            }
            st.check(
                "deduction_closed_form",
                case,
                ded - want,
                5e-3_f64.max(want * 1e-3),
                format!("part {gi}: deducted {ded:.4}, plan cut/cos + windows {want:.4}"),
            );
        }
    }

    // --- Coverage: the faces in plan fill the eave outline, no gap, no overlap. ---
    if calculated && p.dormers.is_empty() && !p.flat_roof {
        for (gi, (part, g)) in p.parts.iter().zip(&geos).enumerate() {
            if joined {
                continue;
            }
            let outline = tectora_core::check::eave_outline(part);
            let mut out = MultiPolygon::new(vec![]);
            for piece in &outline {
                if piece.len() >= 3 {
                    out = out.union(&MultiPolygon::new(vec![gpoly(
                        piece.iter().map(|q| (q.x, q.y)),
                    )]));
                }
            }
            let faces: Vec<&RoofFace> = g
                .faces
                .iter()
                .filter(|f| f.polygon.len() >= 3 && f.pitch_deg < 89.9)
                .collect();
            let sum: f64 = faces.iter().map(|f| plan_area(&f.polygon)).sum();
            let mut cover = MultiPolygon::new(vec![]);
            for f in &faces {
                cover = cover.union(&MultiPolygon::new(vec![face_plan(f)]));
            }
            let gap = out.difference(&cover).unsigned_area();
            let extra = cover.difference(&out).unsigned_area();
            let overlap = sum - cover.unsigned_area();

            let tag = format!("part {gi}{}", if warned { " warned" } else { "" });
            let k = |s: &'static str, w: &'static str| if warned { w } else { s };
            st.check(
                k("coverage_gap", "coverage_gap_warned"),
                case,
                gap,
                0.01,
                format!("{tag}: {gap:.4} m2 of the eave outline has no roof"),
            );
            st.check(
                k("coverage_outside", "coverage_outside_warned"),
                case,
                extra,
                0.01,
                format!("{tag}: {extra:.4} m2 of roof outside the eave outline"),
            );
            st.check(
                k("coverage_overlap", "coverage_overlap_warned"),
                case,
                overlap,
                0.01,
                format!("{tag}: faces overlap by {overlap:.4} m2 in plan"),
            );
            // Overlaps where the two faces are at the same height are area
            // counted twice; a face above another (an overhang over a lower
            // roof) is two layers of roofing.
            let mut doubled = 0.0;
            for i in 0..faces.len() {
                for j in i + 1..faces.len() {
                    let inter = MultiPolygon::new(vec![face_plan(faces[i])])
                        .intersection(&MultiPolygon::new(vec![face_plan(faces[j])]));
                    for piece in &inter.0 {
                        let a = piece.unsigned_area();
                        if a < 1e-4 {
                            continue;
                        }
                        let c = piece
                            .exterior()
                            .0
                            .iter()
                            .take(piece.exterior().0.len().saturating_sub(1))
                            .fold((0.0, 0.0, 0.0), |s, q| (s.0 + q.x, s.1 + q.y, s.2 + 1.0));
                        let (x, y) = (c.0 / c.2, c.1 / c.2);
                        let (zi, zj) = (z_at(faces[i], x, y), z_at(faces[j], x, y));
                        if (zi - zj).abs() < 0.05 {
                            if std::env::var_os("VERIFY_PAIRS").is_some() {
                                println!(
                                    "  pair {i} {j} piece {a:.4} at ({x:.3},{y:.3}) z {zi:.3} {zj:.3}"
                                );
                            }
                            doubled += a;
                        }
                    }
                }
            }
            st.check(
                k("coverage_doubled", "coverage_doubled_warned"),
                case,
                doubled,
                0.01,
                format!("{tag}: {doubled:.4} m2 in plan covered twice at the same height"),
            );
        }
    }

    // --- Joined buildings, one eave height, no objects: the roof covers the
    // union of the eave outlines once, so its net plan area is that union.
    let same_eaves = p.parts.iter().flat_map(|q| &q.edges).all(|e| {
        (e.eave_height - p.parts[0].edges[0].eave_height).abs() < 1e-9
            && e.breaks.is_empty()
            && e.half_hip.is_none()
            && !e.clad
    });
    let objects = !p.dormers.is_empty()
        || !p.chimneys.is_empty()
        || !p.windows.is_empty()
        || !p.roof_openings.is_empty();
    if joined && calculated && same_eaves && !objects && !p.flat_roof {
        let mut out = MultiPolygon::new(vec![]);
        for part in &p.parts {
            for piece in tectora_core::check::eave_outline(part) {
                if piece.len() >= 3 {
                    out = out.union(&MultiPolygon::new(vec![gpoly(
                        piece.iter().map(|q| (q.x, q.y)),
                    )]));
                }
            }
        }
        let net: f64 = geos
            .iter()
            .flat_map(|g| &g.faces)
            .filter(|f| f.pitch_deg < 89.9)
            .map(|f| f.area * f.pitch_deg.to_radians().cos())
            .sum();
        let want = out.unsigned_area();
        st.check(
            if warned {
                "joined_plan_area_warned"
            } else {
                "joined_plan_area"
            },
            case,
            net - want,
            0.01_f64.max(want * 2e-3),
            format!("net plan {net:.3} union of eave outlines {want:.3}"),
        );
    }

    // --- Joined buildings: no roof face of one building lies inside another
    // building's walls under that building's roof (counted twice, hidden).
    if joined && calculated {
        let mut hidden = 0.0;
        for (k, g) in geos.iter().enumerate() {
            for (j, part) in p.parts.iter().enumerate() {
                if j == k {
                    continue;
                }
                let walls =
                    MultiPolygon::new(vec![gpoly(part.outline().iter().map(|q| (q.x, q.y)))]);
                for f in g
                    .faces
                    .iter()
                    .filter(|f| f.polygon.len() >= 3 && f.pitch_deg < 89.9)
                {
                    for piece in &MultiPolygon::new(vec![face_plan(f)]).intersection(&walls).0 {
                        let a = piece.unsigned_area();
                        if a < 1e-3 {
                            continue;
                        }
                        let pts = &piece.exterior().0;
                        let m = pts.len().saturating_sub(1).max(1) as f64;
                        let (x, y) = pts
                            .iter()
                            .take(pts.len().saturating_sub(1))
                            .fold((0.0, 0.0), |s, q| (s.0 + q.x / m, s.1 + q.y / m));
                        let z = z_at(f, x, y);
                        // The other building's roof over that point.
                        let over = geos[j]
                            .faces
                            .iter()
                            .filter(|h| h.polygon.len() >= 3 && h.pitch_deg < 89.9)
                            .filter(|h| {
                                use geo::Contains;
                                face_plan(h).contains(&geo::Point::new(x, y))
                            })
                            .map(|h| z_at(h, x, y))
                            .fold(f64::NEG_INFINITY, f64::max);
                        if over.is_finite() && over > z + 0.05 {
                            hidden += a / f.pitch_deg.to_radians().cos();
                        }
                    }
                }
            }
        }
        st.check(
            if warned {
                "joined_hidden_area_warned"
            } else {
                "joined_hidden_area"
            },
            case,
            hidden,
            0.05,
            format!("{hidden:.3} m2 of roof inside another building's walls under its roof"),
        );
    }

    // --- Every place that shows the result shows the same figures. ---
    let calc = tectora_core::report::calc_of(p, &geos);
    let totals = &calc["totals"];
    let roof_area = totals["roof_area"].as_f64().unwrap_or(f64::NAN);
    let face_sum: f64 = geos
        .iter()
        .map(|g| g.faces.iter().map(|f| f.area).sum::<f64>() + g.clad_area)
        .sum::<f64>()
        + p.manual_faces.iter().map(|f| f.area()).sum::<f64>();
    st.check(
        "total_is_sum_of_faces",
        case,
        roof_area - face_sum,
        1e-6,
        format!("total {roof_area} faces {face_sum}"),
    );
    let mats: f64 = totals["area_by_material"].as_array().map_or(0.0, |a| {
        a.iter().map(|e| e[1].as_f64().unwrap_or(0.0)).sum()
    });
    st.check(
        "materials_sum_to_total",
        case,
        mats - roof_area,
        1e-6,
        format!("materials {mats} total {roof_area}"),
    );
    let codes: BTreeMap<String, f64> =
        serde_json::from_value(calc["codes"].clone()).unwrap_or_default();
    let code = |c: &[&str]| {
        c.iter()
            .map(|k| codes.get(*k).copied().unwrap_or(0.0))
            .sum::<f64>()
    };
    let field = |k: &str| totals[k].as_f64().unwrap_or(0.0);
    for (k, c) in [
        ("eave_length", &["T"][..]),
        ("ridge_length", &["F"]),
        ("hip_length", &["G"]),
        ("valley_length", &["K"]),
        ("verge_length", &["OL", "OR", "OS"]),
        ("pult_length", &["P"]),
        ("knick_length", &["c"]),
        ("gorge_length", &["S"]),
    ] {
        st.check(
            "line_types_vs_codes",
            case,
            field(k) - code(c),
            0.006 * c.len() as f64,
            format!("{k} {} codes {:?} {}", field(k), c, code(c)),
        );
    }
    let ridge_z = geos
        .iter()
        .flat_map(|g| g.lines.iter())
        .flat_map(|l| [l.a[2], l.b[2]])
        .fold(0.0, f64::max);
    st.check(
        "ridge_height",
        case,
        field("ridge_height") - ridge_z,
        1e-9,
        String::new(),
    );
    let face_z = geos
        .iter()
        .flat_map(|g| g.faces.iter())
        .filter(|f| f.pitch_deg < 89.9)
        .flat_map(|f| f.polygon.iter().map(|q| q[2]))
        .fold(0.0, f64::max);
    if calculated {
        st.check(
            "ridge_height_is_roof_top",
            case,
            field("ridge_height") - face_z,
            0.01,
            format!("ridge {} faces top {face_z}", field("ridge_height")),
        );
    }

    // Breakdown ("Postup").
    let b = tectora_core::breakdown::from_geometry(p, &geos);
    st.check(
        "breakdown_area",
        case,
        b.roof_area - roof_area,
        1e-6,
        format!("breakdown {} results {roof_area}", b.roof_area),
    );
    for g in &b.lines {
        let want = codes.get(&g.code).copied().unwrap_or(0.0);
        st.check(
            "breakdown_lines",
            case,
            g.total - want,
            0.0051,
            format!("{} breakdown {} results {want}", g.code, g.total),
        );
        let s: f64 = g.items.iter().map(|i| i.length).sum::<f64>() - g.deducted;
        st.check(
            "breakdown_line_sum",
            case,
            s - g.sum,
            1e-6,
            format!("{} items {s} sum {}", g.code, g.sum),
        );
    }
    for (c, v) in &codes {
        if v.abs() > 0.005 && !b.lines.iter().any(|g| &g.code == c) {
            st.check(
                "breakdown_has_every_code",
                case,
                *v,
                0.0,
                format!("code {c} {v} missing in breakdown"),
            );
        }
    }
    let bm: f64 = b.materials.iter().map(|m| m.area).sum();
    st.check(
        "breakdown_materials",
        case,
        bm - roof_area,
        1e-6,
        format!("breakdown materials {bm} total {roof_area}"),
    );
    for part in &b.parts {
        for f in &part.faces {
            st.check(
                "breakdown_face_formula",
                case,
                f.engine_gross.unwrap_or(f.gross) - f.deducted - f.area,
                6e-4,
                format!(
                    "{}: gross {} - ded {} != {}",
                    part.name, f.gross, f.deducted, f.area
                ),
            );
        }
    }

    // Price-sheet list summary and text.
    let opts = tectora_core::export::ListOptions::new("t", "01.01.2026 00:00");
    let sum = tectora_core::export::list_summary(p, &opts);
    let net: f64 = sum.faces.iter().map(|f| f.area - f.deducted).sum();
    let manual_unpitched: f64 = p
        .manual_faces
        .iter()
        .filter(|f| !f.pitch_counts())
        .map(|f| f.area())
        .sum();
    st.check(
        "list_faces_vs_total",
        case,
        net - (roof_area - manual_unpitched),
        2e-3,
        format!("list net {net} total {roof_area}"),
    );
    for l in &sum.lines {
        let want = codes.get(&l.code).copied().unwrap_or(0.0);
        st.check(
            "list_lines_vs_results",
            case,
            l.length - want,
            0.0051 + 1e-5 * 50.0,
            format!("{} list {} results {want}", l.code, l.length),
        );
    }
    for (c, v) in &codes {
        if v.abs() > 0.005 && !sum.lines.iter().any(|l| &l.code == c) {
            st.check(
                "list_has_every_code",
                case,
                *v,
                0.0,
                format!("code {c} {v} missing in list"),
            );
        }
    }
    let by_mat: BTreeMap<u8, f64> =
        totals["area_by_material"]
            .as_array()
            .map_or_else(BTreeMap::new, |a| {
                a.iter()
                    .map(|e| {
                        (
                            e[0].as_u64().unwrap_or(0) as u8,
                            e[1].as_f64().unwrap_or(0.0),
                        )
                    })
                    .collect()
            });
    for m in &sum.materials {
        let want = by_mat.get(&m.material).copied().unwrap_or(0.0);
        st.check(
            "list_materials_vs_results",
            case,
            m.area - want,
            2e-3,
            format!("material {} list {} results {want}", m.material, m.area),
        );
    }
    let text = tectora_core::export::dacad_list(p, &opts);
    let mut in_lines = false;
    let mut text_lines = Vec::new();
    for line in text.lines() {
        if line.starts_with("LÍNIE") {
            in_lines = true;
            continue;
        }
        if in_lines
            && !line.starts_with('-')
            && let Some(v) = line
                .rsplit(' ')
                .next()
                .and_then(|v| v.strip_suffix('m'))
                .and_then(|v| v.replace(',', ".").parse::<f64>().ok())
        {
            text_lines.push(v);
        }
    }
    let sl: Vec<f64> = sum.lines.iter().map(|l| l.length).collect();
    st.check(
        "list_text_vs_summary_count",
        case,
        text_lines.len() as f64 - sl.len() as f64,
        0.0,
        format!("text {text_lines:?} summary {sl:?}"),
    );
    for (a, b) in text_lines.iter().zip(&sl) {
        st.check(
            "list_text_vs_summary",
            case,
            a - b,
            6e-4,
            format!("text {a} summary {b}"),
        );
    }

    // Plan sheet: face labels less cut-out labels add up to the total.
    let shapes = tectora_core::export::plan_sheet(
        p,
        &tectora_core::export::SheetInfo {
            stamp: String::new(),
            decimals: 3,
        },
    );
    let (mut pos, mut neg, mut total, mut n_labels) = (0.0, 0.0, f64::NAN, 0usize);
    let parse = |t: &str| {
        t.trim()
            .replace(',', ".")
            .replace(['\u{a0}', ' '], "")
            .parse::<f64>()
            .ok()
    };
    for s in &shapes {
        if let tectora_core::export::Shape::Text { text, .. } = s {
            if let Some(v) = text
                .strip_prefix("A = ")
                .and_then(|t| t.strip_suffix(" m²"))
                .and_then(parse)
            {
                total = v;
            } else if let Some(v) = text.strip_suffix("m²").and_then(parse) {
                n_labels += 1;
                if v < 0.0 {
                    neg -= v;
                } else {
                    pos += v;
                }
            }
        }
    }
    if calculated && !total.is_nan() {
        st.check(
            "sheet_total_vs_results",
            case,
            total - roof_area,
            6e-4,
            format!("sheet {total} results {roof_area}"),
        );
        let clad: f64 = geos.iter().map(|g| g.clad_area).sum();
        st.check(
            "sheet_labels_add_up",
            case,
            pos - neg - (total - clad),
            6e-4 * (n_labels as f64 + 1.0),
            format!("faces {pos:.3} - cut-outs {neg:.3} vs total {total:.3} (clad {clad:.3})"),
        );
    }

    // Stored result.
    let snap = tectora_core::snapshot::take_of(&geos, p);
    st.check(
        "snapshot_area",
        case,
        snap.roof_area - r2(roof_area),
        0.0051,
        format!("snapshot {} results {roof_area}", snap.roof_area),
    );
    for (c, v) in &snap.lines_by_code {
        let want = codes.get(c).copied().unwrap_or(0.0);
        st.check(
            "snapshot_lines",
            case,
            v - want,
            0.0051,
            format!("{c} snapshot {v} results {want}"),
        );
    }

    // Area per pitch adds up to the pitched faces.
    let pitched: f64 = geos
        .iter()
        .flat_map(|g| g.faces.iter())
        .map(|f| f.area)
        .sum();
    let bp: f64 = tectora_core::quantities::area_by_pitch_of(&geos)
        .iter()
        .map(|(_, a)| a)
        .sum();
    let n = tectora_core::quantities::area_by_pitch_of(&geos).len() as f64;
    st.check(
        "area_by_pitch_sum",
        case,
        bp - pitched,
        0.005 * n.max(1.0),
        format!("by pitch {bp} faces {pitched}"),
    );
}

// ---------- corpus ----------

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
    fn range(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.next()
    }
    fn pick<T: Copy>(&mut self, v: &[T]) -> T {
        v[((self.next() * v.len() as f64) as usize).min(v.len() - 1)]
    }
    fn chance(&mut self, p: f64) -> bool {
        self.next() < p
    }
}

fn outline(r: &mut Rng) -> Vec<(f64, f64)> {
    let w = (r.range(5.0, 16.0) * 100.0).round() / 100.0;
    let d = (r.range(4.0, 11.0) * 100.0).round() / 100.0;
    match r.pick(&[0, 0, 1, 2]) {
        0 => vec![(0.0, 0.0), (w, 0.0), (w, d), (0.0, d)],
        1 => {
            let a = (w * r.range(0.35, 0.65) * 100.0).round() / 100.0;
            let b = (d * r.range(1.4, 2.0) * 100.0).round() / 100.0;
            vec![(0.0, 0.0), (w, 0.0), (w, d), (a, d), (a, b), (0.0, b)]
        }
        _ => {
            let a = (w * r.range(0.3, 0.4) * 100.0).round() / 100.0;
            let b = (d * r.range(1.4, 2.0) * 100.0).round() / 100.0;
            vec![
                (0.0, 0.0),
                (w, 0.0),
                (w, d),
                (w - a, d),
                (w - a, b),
                (a, b),
                (a, d),
                (0.0, d),
            ]
        }
    }
}

fn random_project(seed: u64) -> Project {
    let mut r = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    for _ in 0..3 {
        r.next();
    }
    let pts = outline(&mut r);
    let base_pitch = r.pick(&[15.0, 22.0, 30.0, 35.0, 40.0, 45.0, 50.0]);
    let eave = r.pick(&[2.8, 3.5, 4.2, 6.0]);
    let ov = r.pick(&[0.0, 0.3, 0.5, 0.8]);
    let n = pts.len();
    let mut edges = Vec::new();
    for (i, &(x, y)) in pts.iter().enumerate() {
        let mut e = tectora_core::edit::plain_edge(Point2::new(x, y), base_pitch, eave, ov);
        if r.chance(0.15) {
            e.pitch_deg = r.pick(&[25.0, 35.0, 55.0]);
        }
        if n == 4 && i % 2 == 1 && r.chance(0.5) {
            e.pitch_deg = 90.0;
        }
        if r.chance(0.1) {
            e.overhang = r.pick(&[0.0, 0.4, 0.9]);
        }
        if r.chance(0.1) {
            e.eave_height = eave + r.pick(&[-0.8, 0.5, 1.0]);
        }
        if e.pitch_deg < 89.0 && r.chance(0.12) {
            e.breaks.push(PitchBreak {
                height: e.eave_height + r.range(0.6, 1.4),
                pitch_deg: r.pick(&[15.0, 25.0, 60.0]),
            });
        }
        if e.pitch_deg >= 89.0 && r.chance(0.3) {
            e.half_hip = Some(HalfHip {
                height: e.eave_height + r.range(1.0, 2.0),
                pitch_deg: r.pick(&[45.0, 60.0]),
            });
        }
        if r.chance(0.1) {
            e.material = 2;
        }
        if e.pitch_deg >= 89.0 && r.chance(0.2) {
            e.clad = true;
        }
        edges.push(e);
    }
    let mut part = RoofPart {
        height: None,
        name: "A".into(),
        edges,
    };
    part.normalize_ccw();
    let mut p = Project::new(format!("rand{seed}"));
    p.parts.push(part);
    // A second building now and then, beside or touching the first.
    if r.chance(0.15) {
        let (w, d) = (r.range(3.0, 6.0), r.range(3.0, 5.0));
        let x0 = pts.iter().map(|q| q.0).fold(f64::MIN, f64::max) + r.pick(&[0.0, 0.0, 2.0]);
        let mut e2 = Vec::new();
        for (x, y) in [(x0, 0.5), (x0 + w, 0.5), (x0 + w, 0.5 + d), (x0, 0.5 + d)] {
            e2.push(tectora_core::edit::plain_edge(
                Point2::new(x, y),
                r.pick(&[20.0, 35.0]),
                eave - r.pick(&[0.0, 0.6]),
                ov,
            ));
        }
        p.parts.push(RoofPart {
            height: None,
            name: "B".into(),
            edges: e2,
        });
    }
    // Objects on the roof, placed inside the footprint.
    let (minx, maxx) = pts
        .iter()
        .fold((f64::MAX, f64::MIN), |a, q| (a.0.min(q.0), a.1.max(q.0)));
    let (miny, maxy) = pts
        .iter()
        .fold((f64::MAX, f64::MIN), |a, q| (a.0.min(q.1), a.1.max(q.1)));
    let inside = |x: f64, y: f64| {
        let mut c = false;
        for i in 0..n {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            if (a.1 > y) != (b.1 > y) && x < a.0 + (y - a.1) / (b.1 - a.1) * (b.0 - a.0) {
                c = !c;
            }
        }
        c
    };
    for _ in 0..3 {
        if r.chance(0.35) {
            let (x, y) = (r.range(minx, maxx), r.range(miny, maxy));
            let s = r.range(0.4, 1.0);
            if inside(x, y) && inside(x + s, y + s) && inside(x + s, y) && inside(x, y + s) {
                p.chimneys.push(Chimney {
                    corners: vec![
                        Point2::new(x, y),
                        Point2::new(x + s, y),
                        Point2::new(x + s, y + s),
                        Point2::new(x, y + s),
                    ],
                    height: eave + 6.0,
                });
            }
        }
    }
    if r.chance(0.3) {
        let (x, y) = (
            r.range(minx + 0.5, maxx - 1.5),
            r.range(miny + 0.3, miny + 1.5),
        );
        if inside(x, y) {
            p.windows.push(RoofWindow {
                x,
                y,
                width: 0.78,
                height: 1.18,
            });
        }
    }
    // A dormer on the first wall's slope of a plain rectangle.
    if n == 4 && p.parts.len() == 1 && p.parts[0].edges[0].pitch_deg < 60.0 && r.chance(0.25) {
        let e0 = &p.parts[0].edges[0];
        let (w, d) = (maxx - minx, maxy - miny);
        let y = miny + d * r.range(0.12, 0.25);
        let x = minx + w * r.range(0.2, 0.5);
        let host = e0.eave_height + (y - miny + e0.overhang) * e0.pitch_deg.to_radians().tan();
        let kind = r.pick(&tectora_core::edit::dormers::DORMER_KINDS);
        let mut dp = tectora_core::edit::dormers::default_params(
            kind,
            Point2::new(x, y),
            0.0,
            host + r.range(0.2, 0.6),
        );
        dp.width = r.range(1.2, 2.5).min(w * 0.4);
        dp.overhang = r.pick(&[0.0, 0.2]);
        dp.side_overhang = r.pick(&[0.0, 0.2]);
        p.dormers
            .push(tectora_core::edit::dormers::build_dormer(&dp));
    }
    if r.chance(0.08) {
        let (x, y) = (r.range(minx, maxx - 1.0), r.range(miny, maxy - 1.0));
        if inside(x, y) && inside(x + 0.8, y + 0.8) {
            p.roof_openings.push(Chimney {
                corners: vec![
                    Point2::new(x, y),
                    Point2::new(x + 0.8, y),
                    Point2::new(x + 0.8, y + 0.8),
                    Point2::new(x, y + 0.8),
                ],
                height: 0.0,
            });
        }
    }
    if r.chance(0.06) {
        let z = eave + 1.0;
        p.manual_lines.push(tectora_core::model::ManualLine {
            a: [minx, miny, eave],
            b: [minx, miny + 2.0, z],
            code: r.pick(&["T", "F", "WS", "M"]).to_string(),
            length: if r.chance(0.3) { Some(1.5) } else { None },
            label: String::new(),
        });
    }
    if r.chance(0.06) {
        p.manual_faces.push(tectora_core::model::ManualFace {
            points: vec![
                Point2::new(maxx + 5.0, 0.0),
                Point2::new(maxx + 7.0, 0.0),
                Point2::new(maxx + 7.0, 3.0),
            ],
            pitch_deg: r.pick(&[0.0, 20.0, 45.0]),
            label: String::new(),
        });
    }
    if r.chance(0.03) {
        p.flat_roof = true;
    }
    p
}

fn load(path: &Path) -> Option<Project> {
    let s = std::fs::read_to_string(path).ok()?;
    if let Ok(p) = Project::from_json(&s) {
        return Some(p);
    }
    let v: serde_json::Value = serde_json::from_str(&s).ok()?;
    serde_json::from_value(v.get("project")?.clone()).ok()
}

fn collect(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_dir() {
        let mut v: Vec<PathBuf> = std::fs::read_dir(path)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        v.sort();
        for p in v {
            collect(&p, out);
        }
    } else if path
        .extension()
        .is_some_and(|x| x == "tectora" || x == "json")
    {
        out.push(path.to_path_buf());
    }
}

fn main() {
    let mut st = Stats {
        runs: BTreeMap::new(),
        shown: BTreeMap::new(),
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut files = Vec::new();
    let mut random = 0u64;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--random" {
            random = args[i + 1].parse().unwrap();
            i += 2;
            continue;
        }
        collect(Path::new(&args[i]), &mut files);
        i += 1;
    }
    let mut n = 0;
    for ex in tectora_core::examples::EXAMPLES {
        verify(&mut st, ex.id, &ex.project());
        n += 1;
    }
    for f in &files {
        if let Some(p) = load(f) {
            verify(&mut st, &f.file_name().unwrap().to_string_lossy(), &p);
            n += 1;
        }
    }
    for s in 0..random {
        let first: u64 = std::env::var("VERIFY_SEED")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1);
        let p = random_project(s + first);
        if std::env::var_os("VERIFY_DUMP").is_some() {
            std::fs::write(
                format!("target/work/verify/rand/{}.tectora", p.name),
                serde_json::to_string_pretty(&p).unwrap(),
            )
            .unwrap();
        }
        verify(&mut st, &p.name.clone(), &p);
        n += 1;
    }
    println!("\n{n} projects");
    for (k, (runs, fails, worst)) in &st.runs {
        println!("{k:32} {runs:7} checks {fails:6} failed  worst {worst:.4}");
    }
}
