//! Fuzzes two buildings drawn next to or into each other (the editor's
//! "buildings join" case) through the same command the app calls after
//! every edit, and prints each case whose roof is not calculated.
//!
//! Usage: `join_fuzz [CASES] [SEED] [OUT_DIR]`.

use tectora_core::Project;
use tectora_core::geom::Point2;
use tectora_core::model::EdgeDef;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn f(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
    fn range(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.f()
    }
    fn pick<T: Copy>(&mut self, v: &[T]) -> T {
        v[(self.next() % v.len() as u64) as usize]
    }
}

fn snap(v: f64, step: f64) -> f64 {
    if step > 0.0 {
        (v / step).round() * step
    } else {
        v
    }
}

fn rect(r: &mut Rng, x: f64, y: f64, w: f64, d: f64, eave: f64, base: &EdgeDef) -> Vec<EdgeDef> {
    let pts = [
        Point2::new(x, y),
        Point2::new(x + w, y),
        Point2::new(x + w, y + d),
        Point2::new(x, y + d),
    ];
    let kind = r.pick(&["hip", "gable_x", "gable_y", "shed", "wild"]);
    let pitch = r.pick(&[22.0, 30.0, 35.0, 38.0, 45.0]);
    let ov = r.pick(&[0.0, 0.3, 0.5, 0.6]);
    pts.iter()
        .enumerate()
        .map(|(i, &p)| {
            let mut e = base.clone();
            e.breaks.clear();
            e.half_hip = None;
            e.start = p;
            e.eave_height = eave;
            e.overhang = ov;
            if kind == "wild" {
                e.pitch_deg = r.pick(&[90.0, 20.0, 30.0, 38.0, 45.0, 60.0]);
                e.overhang = r.pick(&[0.0, 0.3, 0.5, 1.0, 3.0]);
                e.eave_height = eave + r.pick(&[0.0, 0.0, 0.5, -0.5, 1.0]);
                if e.pitch_deg == 90.0 && r.f() < 0.3 {
                    e.half_hip = Some(tectora_core::model::HalfHip {
                        height: e.eave_height + 2.0,
                        pitch_deg: 50.0,
                    });
                }
                return e;
            }
            e.pitch_deg = match (kind, i) {
                ("gable_x", 1 | 3) | ("gable_y", 0 | 2) => 90.0,
                ("shed", 1..=3) => 90.0,
                _ => pitch,
            };
            e
        })
        .collect()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cases: usize = args.get(1).and_then(|a| a.parse().ok()).unwrap_or(500);
    let seed: u64 = args.get(2).and_then(|a| a.parse().ok()).unwrap_or(1);
    let out = args.get(3).cloned();
    let base_project = tectora_core::examples::find("hip").unwrap().project();
    let base = base_project.parts[0].edges[0].clone();
    let mut r = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let mut bad = 0;
    let mut joined = 0;
    let (mut errs, mut dacad, mut fallback) = (0, 0, 0);
    for case in 0..cases {
        let step: f64 = r.pick(&[0.0, 0.01, 0.05, 0.1]);
        let w = snap(r.range(6.0, 14.0), step.max(0.01));
        let d = snap(r.range(5.0, 11.0), step.max(0.01));
        let eave = r.pick(&[2.5, 3.0, 3.0, 4.0]);
        let mut p: Project = base_project.clone();
        p.dormers.clear();
        p.chimneys.clear();
        p.windows.clear();
        p.parts.truncate(1);
        p.parts[0].edges = rect(&mut r, 0.0, 0.0, w, d, eave, &base);
        // The second building: on one side, touching or reaching in.
        let w2 = snap(r.range(3.0, 9.0), step.max(0.01));
        let d2 = snap(r.range(3.0, 8.0), step.max(0.01));
        let into = r.pick(&[0.0, 0.0, 0.0, 0.5, 1.0, 2.0, 0.003, -0.003]);
        let side = r.next() % 4;
        let along = snap(r.range(-2.0, 10.0), step);
        let (x, y) = match side {
            0 => (along, -d2 + into), // below
            1 => (w - into, along),   // right
            2 => (along, d - into),   // above
            _ => (-w2 + into, along), // left
        };
        let eave2 = r.pick(&[eave, eave, eave - 0.5, eave + 0.5, eave - 1.0]);
        let mut e2 = rect(&mut r, snap(x, step), snap(y, step), w2, d2, eave2, &base);
        let mut part2 = p.parts[0].clone();
        part2.name = "B".into();
        part2.edges = std::mem::take(&mut e2);
        p.parts.push(part2);
        let t0 = std::time::Instant::now();
        let v = tectora_core::commands::calc(&p);
        let dt = t0.elapsed().as_secs_f64();
        if dt > 2.0 {
            println!("case {case}: slow {dt:.1}s");
        }
        let why = match &v {
            Err(e) => {
                errs += 1;
                Some(format!("ERROR {e}"))
            }
            Ok(v) => {
                let parts = v["parts"].as_array().unwrap();
                if let Some(e) = parts
                    .iter()
                    .find_map(|p| p["geometry"]["error"].as_object())
                {
                    dacad += 1;
                    Some(format!(
                        "DACAD-ERR {}",
                        serde_json::Value::Object(e.clone())
                    ))
                } else if parts.iter().any(|p| {
                    p["geometry"]["warnings"]
                        .as_array()
                        .is_some_and(|w| w.iter().any(|w| w["kind"] == "not_calculated_by_core"))
                }) {
                    fallback += 1;
                    Some("FALLBACK".into())
                } else if parts.iter().any(|p| {
                    p["geometry"]["faces"]
                        .as_array()
                        .is_none_or(|f| f.is_empty())
                }) {
                    Some("NO-FACES".into())
                } else {
                    if parts.iter().any(|p| {
                        p["geometry"]["joined_with"]
                            .as_array()
                            .is_some_and(|j| !j.is_empty())
                    }) {
                        joined += 1;
                    }
                    None
                }
            }
        };
        if let Some(mut why) = why {
            bad += 1;
            // Each building alone.
            let alone: Vec<bool> = (0..p.parts.len())
                .map(|k| {
                    let mut q = p.clone();
                    q.parts = vec![p.parts[k].clone()];
                    tectora_core::commands::calc(&q).is_ok_and(|v| {
                        v["parts"][0]["geometry"]["error"].is_null()
                            && v["parts"][0]["geometry"]["faces"]
                                .as_array()
                                .is_some_and(|f| !f.is_empty())
                    })
                })
                .collect();
            if alone.iter().all(|&ok| ok) {
                why = format!("JOIN-ONLY {why}");
            } else {
                why = format!("alone-ok {alone:?} {why}");
            }
            println!("case {case}: {why}");
            if let Some(dir) = &out {
                std::fs::write(
                    format!("{dir}/case{seed}_{case}.json"),
                    serde_json::to_string_pretty(&p).unwrap(),
                )
                .unwrap();
            }
        }
    }
    println!("joined {joined}");
    println!(
        "{bad}/{cases} not calculated: {errs} errors, {dacad} core errors, {fallback} fallback"
    );
}
