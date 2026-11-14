//! Differential cases for the editor operations (`tectora_core::edit`) and
//! the editor's TypeScript copies of them (`app/src/lib/roofcalc.ts`,
//! `transform.ts`, `roundroof.ts`, `dormers.ts`). Until the UI calls the core
//! for these (ARCH-006), both must give the same result for the same input:
//! this test writes the inputs with the core's outputs to
//! `tests/fixtures/edit/parity.json`, and `app/src/lib/editparity.test.ts`
//! runs the TypeScript functions on the same inputs and compares.
//!
//! The file is checked here against the core. When an operation changes on
//! purpose, regenerate it and change the TypeScript copy with it:
//! `TECTORA_WRITE_EDIT_CASES=1 cargo test -p tectora-core --test edit_parity`.

use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tectora_core::edit::calc::{self, BuildUpEave, ParallelField, Target};
use tectora_core::edit::dormers::{self, DORMER_KINDS, DormerKind, DormerParams, SectionTarget};
use tectora_core::edit::{round, transform};
use tectora_core::geom::Point2;
use tectora_core::model::{Chimney, Dormer, EdgeDef, ManualFace, ManualLine, RoofPart, RoofWindow};
use tectora_core::{Project, examples};

const FILE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/edit/parity.json"
);

fn from<T: DeserializeOwned>(v: &Value) -> T {
    serde_json::from_value(v.clone()).expect("case input")
}

fn to(v: impl serde::Serialize) -> Value {
    serde_json::to_value(v).expect("serializable")
}

fn p(x: f64, y: f64) -> Value {
    json!({ "x": x, "y": y })
}

fn wall(x: f64, y: f64, pitch: f64, eave: f64, overhang: f64) -> Value {
    json!({ "start": p(x, y), "pitch_deg": pitch, "eave_height": eave, "overhang": overhang })
}

/// Outlines the cases run on, all counter-clockwise.
fn outlines() -> Vec<(&'static str, Value)> {
    let rect = |pitches: [f64; 4]| {
        json!([
            wall(0.0, 0.0, pitches[0], 3.0, 0.5),
            wall(10.0, 0.0, pitches[1], 3.0, 0.5),
            wall(10.0, 8.0, pitches[2], 3.0, 0.5),
            wall(0.0, 8.0, pitches[3], 3.0, 0.5),
        ])
    };
    let mut broken = rect([40.0, 40.0, 40.0, 40.0]);
    broken[0]["breaks"] = json!([{ "height": 4.5, "pitch_deg": 60.0 }]);
    broken[2]["breaks"] =
        json!([{ "height": 4.2, "pitch_deg": 25.0 }, { "height": 5.1, "pitch_deg": 50.0 }]);
    let mut gables = rect([40.0, 90.0, 40.0, 90.0]);
    gables[1]["eave_height"] = json!(3.4);
    // A rectangle 9.37 × 6.11 turned by 23.7° with corners rounded to mm.
    let (s, c) = 23.7f64.to_radians().sin_cos();
    let o = (11.113, -3.271);
    let corner = |x: f64, y: f64| {
        let r = |v: f64| (v * 1000.0).round() / 1000.0;
        (r(o.0 + x * c - y * s), r(o.1 + x * s + y * c))
    };
    let rot: Vec<Value> = [(0.0, 0.0), (9.37, 0.0), (9.37, 6.11), (0.0, 6.11)]
        .iter()
        .zip([35.0, 42.5, 35.0, 42.5])
        .map(|(&(x, y), pitch)| {
            let (x, y) = corner(x, y);
            wall(x, y, pitch, 2.85, 0.45)
        })
        .collect();
    let l_shape = json!([
        wall(0.0, 0.0, 38.0, 3.1, 0.6),
        wall(12.0, 0.0, 38.0, 3.1, 0.6),
        wall(12.0, 5.0, 38.0, 3.4, 0.35),
        wall(5.0, 5.0, 38.0, 3.1, 0.6),
        wall(5.0, 10.0, 41.0, 2.6, 0.8),
        wall(0.0, 10.0, 38.0, 3.1, 0.6),
    ]);
    // A hip roof with a direction marker (zero-length wall) at a corner.
    let mut marker = rect([40.0, 40.0, 40.0, 40.0]).as_array().cloned().unwrap();
    let mut m = wall(10.0, 0.0, 40.0, 3.0, 0.5);
    m["direction_rad"] = json!(1.2);
    marker.insert(1, m);
    vec![
        ("gable", gables),
        ("hip", rect([40.0, 40.0, 40.0, 40.0])),
        ("broken", broken),
        ("rot", Value::Array(rot)),
        ("l", l_shape),
        ("marker", Value::Array(marker)),
    ]
}

/// Inputs name the shared outlines and project instead of repeating them:
/// `{"$ref": "outline", "name": ...}` is an outline's walls,
/// `{"$ref": "edge", "name": ..., "i": ...}` one wall, `{"$ref": "project"}`
/// the project of [`busy_project`].
fn outline_ref(name: &str) -> Value {
    json!({ "$ref": "outline", "name": name })
}

fn edge_ref(name: &str, i: usize) -> Value {
    json!({ "$ref": "edge", "name": name, "i": i })
}

struct Cases(Vec<Value>);

impl Cases {
    fn add(&mut self, op: &str, input: Value, out: Value) {
        self.0.push(json!({ "op": op, "in": input, "out": out }));
    }
}

fn calc_cases(c: &mut Cases) {
    let sets = outlines();
    let all_edges: Vec<(Value, Value)> = sets
        .iter()
        .flat_map(|(name, es)| {
            let es = es.as_array().cloned().unwrap();
            (0..es.len()).map(move |k| (edge_ref(name, k), es[k].clone()))
        })
        .collect();

    for (e, ed) in &all_edges {
        let ed: EdgeDef = from(ed);
        for (w, r, h) in [
            (Some(3.7), None, Some(6.0)),
            (Some(4.0), None, Some(7.33)),
            (None, Some(5.5), Some(6.0)),
            (None, Some(6.123), Some(7.33)),
            (Some(4.0), Some(5.5), None),
            (Some(5.25), Some(6.123), None),
            (Some(4.0), Some(5.5), Some(6.0)),
            (Some(4.0), None, Some(2.0)),
            (None, Some(1.0), Some(6.0)),
            (Some(4.0), None, None),
        ] {
            for with_ov in [false, true] {
                c.add(
                    "triangle",
                    json!({ "edge": e, "width": w, "rafter": r, "ridge": h, "with_overhang": with_ov }),
                    to(calc::triangle(&ed, w, r, h, with_ov)),
                );
            }
        }
        c.add(
            "triangle_patch",
            json!({ "edge": e, "pitch_deg": 33.3333 }),
            to(calc::triangle_patch(&ed, 33.3333)),
        );
        for d in [0.2, -0.15, -2.0, 0.0375] {
            for mode in [
                BuildUpEave::Normal,
                BuildUpEave::Horizontal,
                BuildUpEave::Vertical,
            ] {
                c.add(
                    "build_up",
                    json!({ "edge": e, "d": d, "eave": mode }),
                    to(calc::build_up(&ed, d, mode)),
                );
            }
        }
        for (d, z) in [
            (1.3, 4.1),
            (2.71, 5.2),
            (4.05, 6.6),
            (0.5, 2.0),
            (3.333, 4.5),
        ] {
            for t in [Target::Pitch, Target::Eave, Target::Overhang] {
                c.add(
                    "solve",
                    json!({ "edge": e, "d": d, "z": z, "target": t }),
                    to(calc::solve(&ed, d, z, t)),
                );
            }
            c.add(
                "shift_for",
                json!({ "edge": e, "d": d, "z": z }),
                to(calc::shift_for(&ed, d, z)),
            );
        }
        for d in [-0.5, 0.0, 1.3, 3.9, 7.2] {
            c.add(
                "roof_height",
                json!({ "edge": e, "d": d }),
                to(calc::roof_height(&ed, d)),
            );
        }
    }

    for (name, raw) in &sets {
        let edges: Vec<EdgeDef> = from(raw);
        let es = &outline_ref(name);
        let n = edges.len();
        for i in 0..n {
            c.add(
                "same_way",
                json!({ "edges": es, "i": i }),
                to(calc::same_way(&edges, i)),
            );
            for q in [(1.0, 1.0), (4.37, 2.9), (-3.0, 12.0)] {
                c.add(
                    "inward",
                    json!({ "edges": es, "i": i, "p": p(q.0, q.1) }),
                    to(calc::inward(&edges, i, Point2::new(q.0, q.1))),
                );
            }
            for delta in [0.35, -1.2, 0.0005] {
                c.add(
                    "move_wall",
                    json!({ "edges": es, "i": i, "delta": delta }),
                    to(calc::move_wall(&edges, i, delta)),
                );
            }
            for eave in [2.5, 3.0, 3.5] {
                c.add(
                    "eave_below_gable",
                    json!({ "edges": es, "i": i, "eave": eave }),
                    to(calc::eave_below_gable(&edges, i, eave)),
                );
            }
            let walls: Vec<usize> = (0..n).collect();
            let mates = calc::same_way(&edges, i);
            for t in [Target::Eave, Target::Overhang, Target::Pitch] {
                c.add(
                    "merge_onto",
                    json!({ "edges": es, "ref": i, "walls": walls, "target": t }),
                    to(calc::merge_onto(&edges, i, &walls, t)),
                );
            }
            c.add(
                "merge_by_moving",
                json!({ "edges": es, "ref": i, "walls": mates }),
                to(calc::merge_by_moving(&edges, i, &mates)),
            );
            let j = (i + 1) % n;
            for (hip, r1, r2) in [
                (7.5, 5.5, 6.0),
                (8.2, 5.874, 5.874),
                (6.0, 5.5, 6.0),
                (9.0, 2.0, 2.0),
            ] {
                for (a, b) in [(i, j), (j, i)] {
                    c.add(
                        "hip_pitches",
                        json!({ "edges": es, "i": a, "j": b, "hip": hip, "r1": r1, "r2": r2 }),
                        to(calc::hip_pitches(&edges, a, b, hip, r1, r2)),
                    );
                }
            }
            for (hip, e1, e2) in [(7.5, 5.0, 4.5), (6.4, 3.3, 4.1), (3.0, 5.0, 4.5)] {
                c.add(
                    "hip_pitches_from_eaves",
                    json!({ "edges": es, "i": i, "j": j, "hip": hip, "e1": e1, "e2": e2 }),
                    to(calc::hip_pitches_from_eaves(&edges, i, j, hip, e1, e2)),
                );
            }
        }
        // Two facing walls: where they meet, and the 2 Parallele dialog.
        for i in 0..n {
            let Some(j) = round::opposite_wall(&edges, i).filter(|&j| j > i) else {
                continue;
            };
            let span = round::span_between(&edges, i, j);
            let (a, b) = (&edges[i], &edges[j]);
            c.add(
                "meeting",
                json!({ "a": edge_ref(name, i), "b": edge_ref(name, j), "span": span }),
                to(calc::meeting(a, b, span)),
            );
            let Some((w, h)) = calc::meeting(a, b, span) else {
                continue;
            };
            if ["gable", "hip", "rot"].contains(name) {
                parallel_cases(c, &edge_ref(name, i), &edge_ref(name, j), a, b, span, w, h);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn parallel_cases(
    c: &mut Cases,
    ja: &Value,
    jb: &Value,
    a: &EdgeDef,
    b: &EdgeDef,
    span: f64,
    w: f64,
    h: f64,
) {
    use ParallelField::*;
    let st0 = calc::parallel_start(a, b, span, w, h);
    let seqs: Vec<Vec<(ParallelField, f64)>> = vec![
        vec![(WidthA, 3.2)],
        vec![(WidthB, 4.6)],
        vec![(WidthA, 4.85)],
        vec![(RafterA, 5.9)],
        vec![(RafterB, 6.37)],
        vec![(RafterA, 5.9), (RafterB, 6.2)],
        vec![(Ridge, 6.8)],
        vec![(Ridge, 5.1234)],
        vec![(Top, 1.0)],
        vec![(Top, 1.5), (Ridge, 7.0)],
        vec![(WidthA, 3.5), (Ridge, 6.5)],
        vec![(Top, 100.0)],
    ];
    for target in [
        Target::Pitch,
        Target::Eave,
        Target::Overhang,
        Target::Position,
    ] {
        for with_ov in [false, true] {
            for steps in &seqs {
                let mut st = Some(st0.clone());
                for &(f, v) in steps {
                    st = st.and_then(|s| calc::parallel_edit(&s, f, v, target, with_ov));
                }
                let patches = match (&st, target) {
                    (Some(s), Target::Pitch | Target::Eave | Target::Overhang) => {
                        to(calc::parallel_patches(s, target))
                    }
                    _ => Value::Null,
                };
                let rafters = st.as_ref().map(|s| {
                    json!([
                        calc::parallel_rafter(s, true),
                        calc::parallel_rafter(s, false)
                    ])
                });
                c.add(
                    "parallel",
                    json!({ "a": ja, "b": jb, "span": span, "w": w, "h": h, "steps": steps, "target": target, "with_ov": with_ov }),
                    json!({ "state": st, "patches": patches, "rafters": rafters }),
                );
            }
        }
    }
}

fn round_cases(c: &mut Cases) {
    for span in [6.0, 8.4, 1.6, 0.3] {
        for overhang in [0.5, 0.0] {
            for segments in [0, 4, 31] {
                for (mode, value) in [
                    (round::ArcMode::Default, 0.0),
                    (round::ArcMode::Radius, 6.0),
                    (round::ArcMode::Radius, 0.2),
                    (round::ArcMode::Ridge, 6.5),
                    (round::ArcMode::Ridge, 4.1),
                    (round::ArcMode::Length, 13.0),
                    (round::ArcMode::Length, 2.0),
                ] {
                    let input = round::ArcInput {
                        span,
                        overhang,
                        eave_height: 3.0,
                        segments,
                        mode,
                        value,
                    };
                    c.add("arc_for", to(input), to(round::arc_for(&input)));
                }
            }
        }
    }
    for deg in [30.0, 45.0, 60.0, 11.25] {
        for radius in [1.0, 5.0, 0.5] {
            c.add(
                "segments_for",
                json!({ "deg": deg, "radius": radius }),
                to(round::segments_for(deg, radius)),
            );
        }
    }
    for (name, raw) in outlines() {
        let edges: Vec<EdgeDef> = from(&raw);
        let es = outline_ref(name);
        for i in 0..edges.len() {
            let j = round::opposite_wall(&edges, i);
            c.add("opposite_wall", json!({ "edges": es, "i": i }), to(j));
            if let Some(j) = j {
                c.add(
                    "span_between",
                    json!({ "edges": es, "i": i, "j": j }),
                    to(round::span_between(&edges, i, j)),
                );
            }
        }
    }
}

/// The hip example with a dormer, a chimney, an opening, a window and a
/// hand-drawn line and face on the main part, and a second part elsewhere.
fn busy_project() -> Project {
    let mut project = examples::find("hip").expect("example").project();
    project.parts[0].name = "Main".into();
    let mut second = project.parts[0].clone();
    second.name = "Main (2)".into();
    for e in &mut second.edges {
        e.start = Point2::new(e.start.x + 40.0, e.start.y - 7.5);
    }
    project.parts.push(second);
    let c = transform::part_centre(&project.parts[0]);
    let mut params = dormers::default_params(
        DormerKind::Gable,
        Point2::new(c.x - 1.0, c.y - 1.5),
        0.0,
        3.6,
    );
    params.width = 2.0;
    let mut dormer = dormers::build_dormer(&params);
    dormer.host_wall = Some([Point2::new(0.0, 0.0), Point2::new(5.0, 0.0)]);
    project.dormers.push(dormer);
    let sq = |x: f64, y: f64| {
        vec![
            Point2::new(x, y),
            Point2::new(x + 0.6, y),
            Point2::new(x + 0.6, y + 0.6),
            Point2::new(x, y + 0.6),
        ]
    };
    project.chimneys.push(Chimney {
        corners: sq(c.x + 1.0, c.y + 0.4),
        height: 1.2,
    });
    project.roof_openings.push(Chimney {
        corners: sq(c.x - 2.0, c.y + 0.7),
        height: 0.0,
    });
    project.windows.push(RoofWindow {
        x: c.x + 0.3,
        y: c.y - 0.9,
        width: 0.78,
        height: 1.18,
    });
    project.manual_lines.push(ManualLine {
        a: [c.x - 1.0, c.y, 4.0],
        b: [c.x + 1.0, c.y + 0.5, 4.5],
        code: "M".into(),
        length: None,
        label: "line".into(),
    });
    project.manual_faces.push(ManualFace {
        points: sq(c.x + 0.2, c.y + 0.2),
        pitch_deg: 30.0,
        label: "face".into(),
    });
    // A direction marker on the main part.
    let mut marker = project.parts[0].edges[1].clone();
    marker.direction_rad = Some(0.7);
    project.parts[0].edges.insert(1, marker);
    project
}

fn transform_cases(c: &mut Cases) {
    let maps = [
        json!({ "kind": "move", "dx": 1.2345, "dy": -0.0005 }),
        json!({ "kind": "move", "dx": -0.0005, "dy": 2.0 }),
        json!({ "kind": "rotate", "centre": p(5.0, 4.0), "deg": 33.3 }),
        json!({ "kind": "rotate", "centre": p(-1.25, 0.5), "deg": -90.0 }),
        json!({ "kind": "mirror", "centre": p(5.0, 4.0), "deg": 17.0 }),
        json!({ "kind": "mirror", "centre": p(0.0, 0.0), "deg": 90.0 }),
    ];
    let project = busy_project();
    for m in &maps {
        let map: transform::PlanMap = from(m);
        for copies in [0, 2] {
            for pi in [0, 1, 5] {
                let mut out = project.clone();
                let idx = transform::transform_part(&mut out, pi, map, copies);
                c.add(
                    "transform_part",
                    json!({ "project": { "$ref": "project" }, "pi": pi, "map": m, "copies": copies }),
                    json!({ "project": out, "parts": idx }),
                );
            }
        }
        for q in [(0.0, 0.0), (1.0005, -2.0015), (-3.3335, 7.25)] {
            c.add(
                "plan_map",
                json!({ "map": m, "p": p(q.0, q.1) }),
                to(map.apply(Point2::new(q.0, q.1))),
            );
        }
    }
    for (name, es) in outlines() {
        let part = RoofPart {
            height: None,
            name: name.into(),
            edges: from(&es),
        };
        let jp = json!({ "name": name, "edges": outline_ref(name) });
        c.add(
            "part_centre",
            json!({ "part": jp }),
            to(transform::part_centre(&part)),
        );
        for q in [(1.0, 1.0), (8.0, 7.0), (11.0, 2.0), (5.0, 5.0), (-1.0, 3.0)] {
            c.add(
                "inside_outline",
                json!({ "part": jp, "p": p(q.0, q.1) }),
                to(transform::inside_outline(&part, Point2::new(q.0, q.1))),
            );
        }
        for ei in 0..part.edges.len() {
            for d in [0.5, -0.25, 1.0005] {
                let mut out = part.clone();
                let ok = transform::offset_wall(&mut out, ei, d);
                c.add(
                    "offset_wall",
                    json!({ "part": jp, "ei": ei, "d": d }),
                    json!({ "ok": ok, "part": out }),
                );
            }
        }
        // The same outline clockwise.
        let mut cw = part.clone();
        cw.edges.reverse();
        let jcw = to(&cw);
        for ei in 0..cw.edges.len() {
            let mut out = cw.clone();
            let ok = transform::offset_wall(&mut out, ei, 0.5);
            c.add(
                "offset_wall",
                json!({ "part": jcw, "ei": ei, "d": 0.5 }),
                json!({ "ok": ok, "part": out }),
            );
        }
    }
    for sides in [3, 5, 8, 12] {
        for rot in [0.0, 17.0] {
            c.add(
                "regular_polygon",
                json!({ "c": p(1.5, 2.5), "sides": sides, "side": 4.2, "rotation_deg": rot }),
                to(transform::regular_polygon(
                    Point2::new(1.5, 2.5),
                    sides,
                    4.2,
                    rot,
                )),
            );
        }
    }
}

fn dormer_cases(c: &mut Cases) {
    let mut built: Vec<Dormer> = Vec::new();
    for kind in DORMER_KINDS {
        c.add(
            "default_params",
            json!({ "kind": kind, "start": p(1.0, 2.0), "angle": 0.4, "eave": 3.3 }),
            to(dormers::default_params(
                kind,
                Point2::new(1.0, 2.0),
                0.4,
                3.3,
            )),
        );
        for (segments, material, angle) in [(0, 1, 0.3), (5, 3, -2.1), (40, 1, 3.0)] {
            let params = DormerParams {
                kind,
                start: Point2::new(3.1, 0.4),
                width: 2.4,
                angle,
                eave_height: 3.6,
                pitch_deg: 42.0,
                front_pitch_deg: 18.0,
                overhang: 0.25,
                side_overhang: 0.4,
                half_hip_height: 0.55,
                half_hip_pitch_deg: 58.0,
                segments,
                material,
            };
            let d = dormers::build_dormer(&params);
            c.add("build_dormer", to(&params), to(&d));
            built.push(d);
        }
    }
    // Another chain: kept as "general".
    let mut general = built[0].clone();
    general.edges.push(general.edges[1].clone());
    built.push(general);
    for d in &built {
        c.add("parse_dormer", to(d), to(dormers::parse_dormer(d)));
    }
    let parts: Vec<RoofPart> = outlines()
        .into_iter()
        .map(|(name, es)| RoofPart {
            height: None,
            name: name.into(),
            edges: from(&es),
        })
        .collect();
    let refs: Vec<Value> = outlines()
        .iter()
        .map(|(name, _)| json!({ "name": name, "edges": outline_ref(name) }))
        .collect();
    // The dormer dialog's section and its inverse, on roofs of 10-50°
    // and a flat one.
    let hosts = [(3.2, 0.0), (3.5, 0.36), (4.1, 0.839), (2.0, 1.19)];
    let mut variants: Vec<DormerParams> = built
        .iter()
        .filter(|d| d.edges.len() == 3)
        .map(dormers::parse_dormer)
        .collect();
    // No overhang, and pitches the section cannot use (MATH-001).
    for kind in DORMER_KINDS {
        let mut q = dormers::default_params(kind, Point2::new(0.0, 0.0), 0.0, 3.6);
        variants.push(q.clone());
        // Front and side overhangs apart, a flat-topped trapezoid among them.
        let mut o = q.clone();
        o.overhang = 0.5;
        o.side_overhang = 0.1;
        o.eave_height = 4.4;
        variants.push(o);
        q.pitch_deg = 0.0;
        q.half_hip_pitch_deg = 0.0;
        q.front_pitch_deg = 90.0;
        variants.push(q);
    }
    for params in variants {
        for &(height, tan) in &hosts {
            let host = dormers::HostSlope { height, tan };
            c.add(
                "dormer_section",
                json!({ "p": to(&params), "host": to(host) }),
                to(dormers::section(&params, host)),
            );
            for target in [
                SectionTarget::WallHeight(0.8),
                SectionTarget::WallHeight(-0.1),
                SectionTarget::Depth(2.7),
                SectionTarget::RidgeLength(1.9),
                SectionTarget::RidgeLength(0.0),
            ] {
                let out = match dormers::eave_for(&params, host, target) {
                    Ok(v) => to(v),
                    Err(_) => json!({ "error": true }),
                };
                c.add(
                    "eave_for",
                    json!({ "p": to(&params), "host": to(host), "target": to(target) }),
                    out,
                );
            }
            for width in [0.6, 1.5, 0.0, 9.0] {
                let out = match dormers::pitch_for_back_width(&params, host, width) {
                    Ok(v) => to(v),
                    Err(_) => json!({ "error": true }),
                };
                let solved = dormers::solve(
                    &params,
                    host,
                    Some(SectionTarget::WallHeight(1.1)),
                    Some(width),
                );
                c.add(
                    "solve_dormer",
                    json!({ "p": to(&params), "host": to(host), "target": to(SectionTarget::WallHeight(1.1)), "width": width }),
                    solved.map_or_else(|_| json!({ "error": true }), |q| to(&q)),
                );
                c.add(
                    "pitch_for_back_width",
                    json!({ "p": to(&params), "host": to(host), "width": width }),
                    out,
                );
            }
        }
    }
    for q in [(1.0, -0.5), (12.5, 2.0), (6.0, 5.2), (20.0, 20.0)] {
        c.add(
            "nearest_wall_angle",
            json!({ "parts": refs, "p": p(q.0, q.1) }),
            to(dormers::nearest_wall_angle(&parts, Point2::new(q.0, q.1))),
        );
    }
}

fn cases() -> Vec<Value> {
    let mut c = Cases(Vec::new());
    calc_cases(&mut c);
    round_cases(&mut c);
    transform_cases(&mut c);
    dormer_cases(&mut c);
    c.0
}

/// The shared inputs, then one case per line, so a change shows as a
/// readable diff.
fn render(cases: &[Value]) -> String {
    let outlines: serde_json::Map<String, Value> = outlines()
        .into_iter()
        .map(|(n, es)| (n.to_string(), es))
        .collect();
    let lines: Vec<String> = cases.iter().map(|c| c.to_string()).collect();
    format!(
        "{{\n\"outlines\": {},\n\"project\": {},\n\"cases\": [\n{}\n]\n}}\n",
        Value::Object(outlines),
        to(busy_project()),
        lines.join(",\n")
    )
}

/// Equal up to float noise between platforms' maths libraries.
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
            (x - y).abs() <= 1e-9 * x.abs().max(y.abs()).max(1.0)
        }
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(x, y)| same(x, y))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same(v, w)))
        }
        _ => a == b,
    }
}

#[test]
fn edit_cases_match_the_core() {
    let now = cases();
    if std::env::var_os("TECTORA_WRITE_EDIT_CASES").is_some() {
        std::fs::create_dir_all(std::path::Path::new(FILE).parent().unwrap()).unwrap();
        std::fs::write(FILE, render(&now)).unwrap();
        return;
    }
    let text = std::fs::read_to_string(FILE).unwrap_or_else(|e| {
        panic!("{FILE}: {e}; write it with TECTORA_WRITE_EDIT_CASES=1 cargo test -p tectora-core --test edit_parity")
    });
    let file: Value = serde_json::from_str(&text).expect("parity.json is JSON");
    let shared: Value = serde_json::from_str(&render(&[])).expect("header");
    assert!(
        same(&file["outlines"], &shared["outlines"]) && same(&file["project"], &shared["project"]),
        "the shared inputs changed: rerun with TECTORA_WRITE_EDIT_CASES=1"
    );
    let stored = file["cases"].as_array().expect("cases");
    assert_eq!(
        stored.len(),
        now.len(),
        "the case list changed: rerun with TECTORA_WRITE_EDIT_CASES=1 and update the TypeScript copy"
    );
    for (i, (s, n)) in stored.iter().zip(&now).enumerate() {
        assert!(
            same(s, n),
            "case {i} ({}) differs from the core:\nstored {}\ncore   {}\n\
             rerun with TECTORA_WRITE_EDIT_CASES=1 once the TypeScript copy matches",
            n["op"],
            s,
            n
        );
    }
}

#[test]
fn every_operation_has_cases() {
    let ops: std::collections::BTreeSet<String> = cases()
        .iter()
        .map(|c| c["op"].as_str().unwrap().to_string())
        .collect();
    for op in [
        "triangle",
        "triangle_patch",
        "parallel",
        "build_up",
        "solve",
        "roof_height",
        "meeting",
        "merge_onto",
        "merge_by_moving",
        "same_way",
        "shift_for",
        "move_wall",
        "hip_pitches",
        "hip_pitches_from_eaves",
        "eave_below_gable",
        "inward",
        "arc_for",
        "segments_for",
        "opposite_wall",
        "span_between",
        "transform_part",
        "plan_map",
        "offset_wall",
        "regular_polygon",
        "part_centre",
        "inside_outline",
        "build_dormer",
        "parse_dormer",
        "default_params",
        "nearest_wall_angle",
    ] {
        assert!(ops.contains(op), "no case for {op}");
    }
    // Every case gives some answer other than "refused" for a part of the
    // inputs, so a broken generator that only produces nulls shows up.
    let answered = cases().iter().filter(|c| !c["out"].is_null()).count();
    assert!(answered * 2 > cases().len(), "{answered} answered");
}
