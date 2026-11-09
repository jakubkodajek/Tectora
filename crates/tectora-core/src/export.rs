//! Exports of a finished roof for the offer: the plan drawing with line
//! lengths, face areas and pitches (SVG and PDF), and the text list the
//! user pastes into the price sheet.
//!
//! The text list reproduces DACAD's "Liste" (Data > Listen, DACAD.exe
//! FUN_00d0ac30, string table 1750-1782) as TERRAN's DACAD build writes it
//! for the price sheet: Slovak section heads, line and material names with
//! their numeric prefixes (`6 Odkvap -`, `Škridla - `). TERRAN's price sheet
//! looks up exactly these words, so they are the export payload and stay as
//! they are in every UI language.
//!
//! The layout is byte for byte the list TERRAN's DACAD wrote for a real job
//! (2026-11-08, test `the_list_is_terrans_dacad_list`): a head line, then
//! the faces with their rafter length (`STREŠNÉ PLOCHY`), the wall and
//! material totals (`PLOCHY`, `Stena : 1` first) and the line totals
//! (`LÍNIE`). Values are checked against a list DACAD wrote for a
//! 10 × 8 m hip roof under Wine (2026-11-08).
//!
//! The sheet (`__VYPOCTY_<date> CZ.xls`, sheet DATA, pasted at DH69) reads
//! face areas only from rows 73-115, i.e. the first 41 faces; DACAD's list
//! has the same limit. Not verified against DACAD: how it numbers dormer
//! faces (here `Z<parts + 1>.nn`, one number for all dormers) and the wall
//! area of several joined roofs.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::Project;
use crate::quantities::{Norm, norm_totals};
use crate::roof::{LineKind, RoofGeometry};

/// 60 dashes: DACAD's separator line (DACAD.exe 0x00eae7d4).
const DASHES: &str = "------------------------------------------------------------";

/// How the list is written.
#[derive(Debug, Clone, PartialEq)]
pub struct ListOptions {
    /// First word of the head line (DACAD writes the file path there).
    pub title: String,
    /// Date and time as DACAD writes it, `dd.mm.yyyy hh:mm`.
    pub stamp: String,
    /// Decimals of lengths and areas (DACAD's precision setting; TERRAN
    /// uses millimetres, angles always have two).
    pub decimals: usize,
}

impl ListOptions {
    pub fn new(title: impl Into<String>, stamp: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            stamp: stamp.into(),
            decimals: 3,
        }
    }
}

/// Group name of a line code in TERRAN's DACAD (string table 1542-1559).
fn line_group(code: &str) -> &'static str {
    match code {
        "T" => "6 Odkvap -",
        "F" => "1 Hrebeň -",
        "G" => "2 Nárožie -",
        "K" => "3 Úžlabie -",
        "P" => "7 Pult -",
        "OL" => "4 Krajné ľavé -",
        "OR" => "5 Krajné pravé -",
        "OS" => "94 Krajné šikmo -",
        "WV" => "91 Predné napojenie -",
        "WH" => "93 Zadné napojenie -",
        "WS" => "92 Bočné napojenie -",
        "c" => "Zlom -",
        "v" => "Vertikálne -",
        "h" => "Horizontálne -",
        "S" => "Schlucht -",
        "z" => "Deliaca línia -",
        "n" => "žiadne -",
        _ => "Nedefinované -",
    }
}

/// Material group name in TERRAN's DACAD (strings 1600-1605), or the
/// project's own name for the material.
fn material_group(project: &Project, m: u8) -> String {
    let own = project
        .material_names
        .get(usize::from(m.max(1) - 1))
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());
    if let Some(n) = own {
        return n.to_string();
    }
    match m {
        0 => "žiadny Materiál",
        1 => "Škridla - ",
        2 => "Plech - ",
        3 => "Kameň",
        4 => "Bridlica",
        5 => "Solarpanel",
        _ => return format!("Material {m}"),
    }
    .to_string()
}

/// A number with a decimal comma, as DACAD prints it under a German locale.
pub fn num(v: f64, decimals: usize) -> String {
    let s = format!("{:.*}", decimals, v);
    let s = if s
        .trim_start_matches('-')
        .chars()
        .all(|c| c == '0' || c == '.')
    {
        s.trim_start_matches('-').to_string()
    } else {
        s
    };
    s.replace('.', ",")
}

/// "m²" as DACAD shows it on TERRAN's computers: DACAD writes the ANSI
/// byte 0xB2, which a Central European Windows (code page 1250) shows as
/// "˛". TERRAN's price sheet searches for this glyph.
const M2: &str = "m\u{2db}";

/// A length or area as DACAD's list takes it: from the SOL, where the core
/// printed it with `%10.5f`, read back as a float. The totals are sums of
/// these (5 hips of 6.4627008 m give 32,313 m in TERRAN's list, not the
/// 32,314 m of the exact sum).
fn sol_value(v: f64) -> f64 {
    if !v.is_finite() || v.abs() > 1e9 {
        return v;
    }
    f64::from(crate::coresol::num::requantize(v as f32))
}

struct ListLine {
    code: String,
    length: f64,
}

struct ListFace {
    name: String,
    pitch: f64,
    area: f64,
    rafter: Option<f64>,
    material: u8,
    /// Windows and chimneys cut out of the face, as one negative entry.
    holes: f64,
}

struct Collected {
    lines: Vec<ListLine>,
    /// Faces per part (index = part), then the dormer and hand-drawn faces.
    faces: Vec<Vec<ListFace>>,
    other_faces: Vec<ListFace>,
    minus: BTreeMap<String, f64>,
}

fn collect(project: &Project, geos: &[RoofGeometry]) -> Collected {
    let mut lines = Vec::new();
    let mut minus: BTreeMap<String, f64> = BTreeMap::new();
    let mut faces = Vec::new();
    let mut dormer_faces = Vec::new();
    let dormer_roof = geos.len() + 1;
    let mut wall_offset = 0usize;
    for (pi, geo) in geos.iter().enumerate() {
        for l in &geo.lines {
            lines.push(ListLine {
                code: l.dacad_code().to_string(),
                length: sol_value(l.length()),
            });
        }
        for (code, len) in &geo.minus_lines {
            *minus.entry(code.clone()).or_default() += len;
        }
        let n_walls = project.parts.get(pi).map_or(0, |p| p.edges.len());
        let mut part_faces = Vec::new();
        for f in &geo.faces {
            let zs = f.polygon.iter().map(|p| p[2]);
            let (lo, hi) = zs.fold((f64::MAX, f64::MIN), |(lo, hi), z| (lo.min(z), hi.max(z)));
            let s = f.pitch_deg.to_radians().sin();
            let rafter = (f.pitch_deg > 0.01 && s > 1e-9 && hi >= lo).then(|| (hi - lo) / s);
            let mut face = ListFace {
                name: String::new(),
                pitch: f.pitch_deg,
                area: sol_value(f.area + f.deducted),
                rafter,
                material: f.material,
                holes: f.deducted,
            };
            // Dormer faces belong to no wall of the part (edge usize::MAX).
            if f.edge < n_walls {
                face.name = format!("Z{}.{:02}", pi + 1, wall_offset + f.edge + 1);
                part_faces.push(face);
            } else {
                face.name = format!("Z{dormer_roof}.{:02}", dormer_faces.len() + 1);
                dormer_faces.push(face);
            }
        }
        for c in &geo.clad_faces {
            part_faces.push(ListFace {
                name: format!("Z{}.{:02}", pi + 1, wall_offset + c.edge.min(n_walls) + 1),
                pitch: 90.0,
                area: c.area,
                rafter: None,
                material: c.material,
                holes: 0.0,
            });
        }
        faces.push(part_faces);
        wall_offset += n_walls;
    }
    for l in project.manual_lines.iter().filter(|l| l.counted()) {
        lines.push(ListLine {
            code: l.code.clone(),
            length: sol_value(l.length()),
        });
    }
    let manual = project
        .manual_faces
        .iter()
        .enumerate()
        .filter(|(_, f)| f.pitch_counts())
        .map(|(i, f)| ListFace {
            name: format!("Z0.{:02}", i + 1),
            pitch: f.pitch_deg,
            area: f.area(),
            rafter: None,
            material: crate::model::DEFAULT_MATERIAL,
            holes: 0.0,
        });
    dormer_faces.extend(manual);
    Collected {
        lines,
        faces,
        other_faces: dormer_faces,
        minus,
    }
}

fn section(out: &mut String, head: &str) {
    out.push_str("\n\n");
    out.push_str(DASHES);
    out.push('\n');
    out.push_str(head);
    out.push('\n');
    out.push_str(DASHES);
    out.push('\n');
}

/// The text list for the price sheet (DACAD's "Liste", see the module docs).
/// Lines end with `\n`; a pasted list fills one spreadsheet row per line.
pub fn dacad_list(project: &Project, opts: &ListOptions) -> String {
    let d = opts.decimals.min(6);
    let len = |v: f64| format!("{}m", num(v, d));
    let area = |v: f64| format!("{}{M2}", num(v, d));
    let deg = |v: f64| format!("{}°", num(v, 2));
    let geos = crate::openings::generate_project(project);
    let c = collect(project, &geos);

    // Head: path, padded to 60 characters, then date and time.
    let title: String = opts.title.chars().filter(|ch| !ch.is_control()).collect();
    let pad = 60usize
        .saturating_sub(title.chars().count() + opts.stamp.chars().count())
        .max(1);
    let mut out = format!("{title}{}{}", " ".repeat(pad), opts.stamp);

    // Faces with their rafter length; a face with windows or chimneys is
    // followed by its deduction.
    section(&mut out, "STREŠNÉ PLOCHY");
    for f in c.faces.iter().flatten().chain(&c.other_faces) {
        let _ = write!(out, "{} {} {}", f.name, deg(f.pitch), area(f.area));
        if let Some(r) = f.rafter {
            let _ = write!(out, " Dĺžka krokvy={}", len(sol_value(r)));
        }
        out.push('\n');
        if f.holes > 1e-9 {
            let _ = writeln!(out, "{}:1 {} {}", f.name, deg(f.pitch), area(-f.holes));
        }
    }
    if project.parts.len() > 1 {
        for (i, fs) in c.faces.iter().enumerate() {
            let s: f64 = fs.iter().map(|f| f.area - f.holes).sum();
            let _ = writeln!(out, "   Dach-Nummer {}: normal: S={}", i + 1, area(s));
        }
    }
    out.push_str(DASHES);
    out.push('\n');

    // The wall area, the area per material, then the norm area where it
    // differs (one material only).
    section(&mut out, "PLOCHY");
    if let Some(w) = crate::openings::wall_area(project).filter(|w| *w > 1e-9) {
        let _ = writeln!(out, "Stena : 1 {}", area(w));
    }
    let mut by_material: BTreeMap<u8, f64> = BTreeMap::new();
    for f in c.faces.iter().flatten().chain(&c.other_faces) {
        if f.material > 0 {
            *by_material.entry(f.material).or_default() += f.area - f.holes;
        }
    }
    let single = by_material.len() == 1;
    for (m, total) in &by_material {
        let name = material_group(project, *m);
        let _ = writeln!(out, "{name} {}", area(*total));
        if single {
            let norm = norm_totals(project, Norm::Din).roof_area;
            if (norm - total).abs() >= 0.5 * 10f64.powi(-(d as i32)) {
                let _ = writeln!(out, "{name}-Norm {}", area(norm));
            }
        }
    }

    // Line totals, sorted by group name like DACAD's group list.
    section(&mut out, "LÍNIE");
    let mut groups: BTreeMap<&'static str, (String, f64)> = BTreeMap::new();
    for l in &c.lines {
        if l.code == "n" || l.code == "z" {
            continue;
        }
        let g = groups
            .entry(line_group(&l.code))
            .or_insert_with(|| (l.code.clone(), 0.0));
        g.1 += l.length;
    }
    for (name, (code, total)) in &groups {
        let total = total - c.minus.get(code).copied().unwrap_or(0.0);
        let _ = writeln!(out, "{name} {}", len(total));
    }
    out
}

/// `dd.mm.yyyy hh:mm` of a Unix time in milliseconds, in UTC (for callers
/// without the user's clock, e.g. the MCP server; the app passes local time).
pub fn utc_stamp(ms: u64) -> String {
    let secs = ms / 1000;
    let (days, rem) = ((secs / 86_400) as i64, secs % 86_400);
    // Civil date from days since 1970-01-01 (H. Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{day:02}.{month:02}.{year:04} {:02}:{:02}",
        rem / 3600,
        rem % 3600 / 60
    )
}

// ---------------------------------------------------------------------------
// Plan drawing

/// A4 landscape in PostScript points.
const PAGE_W: f64 = 841.89;
const PAGE_H: f64 = 595.28;
const MARGIN: f64 = 28.0;

/// Line colours of the light theme (`app/src/app.css`, `--line-*`).
fn kind_color(kind: LineKind) -> &'static str {
    match kind {
        LineKind::Eave => "#2f6fd6",
        LineKind::Verge => "#c47f17",
        LineKind::Ridge => "#d6402f",
        LineKind::Hip => "#2c9a5b",
        LineKind::Valley => "#8a46c9",
        LineKind::Pult => "#c2417a",
        LineKind::Knick => "#1f9aa8",
        LineKind::Gorge => "#6b4fd8",
        _ => "#8b5a3c",
    }
}

const INK: &str = "#1d1d1f";
const MUTED: &str = "#5c5b5a";
const BRAND: &str = "#a8362c";

/// One element of the drawing, in page points with y down.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    Line {
        a: [f64; 2],
        b: [f64; 2],
        color: &'static str,
        width: f64,
        dashed: bool,
    },
    Text {
        at: [f64; 2],
        /// Degrees, counter-clockwise on the page.
        angle: f64,
        size: f64,
        color: &'static str,
        bold: bool,
        /// Horizontal anchor: 0 start, 0.5 middle, 1 end.
        anchor: f64,
        text: String,
    },
}

/// What the plan sheet says besides the drawing.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SheetInfo {
    pub stamp: String,
    pub decimals: usize,
}

/// The plan sheet: the roof seen from above with every line's length and
/// every face's area and pitch, the job in the head and the totals at the
/// foot. Page coordinates (A4 landscape, points).
pub fn plan_sheet(project: &Project, info: &SheetInfo) -> Vec<Shape> {
    let d = info.decimals.min(6);
    let geos = crate::openings::generate_project(project);
    let mut shapes = Vec::new();

    // Head: job name, customer and address, date on the right.
    let mut head = project.name.trim().to_string();
    if !project.job.number.trim().is_empty() {
        head = format!("{} · {head}", project.job.number.trim());
    }
    shapes.push(Shape::Text {
        at: [MARGIN, MARGIN + 12.0],
        angle: 0.0,
        size: 14.0,
        color: INK,
        bold: true,
        anchor: 0.0,
        text: head,
    });
    let sub: Vec<&str> = [project.job.customer.trim(), project.job.address.trim()]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect();
    if !sub.is_empty() {
        shapes.push(Shape::Text {
            at: [MARGIN, MARGIN + 27.0],
            angle: 0.0,
            size: 9.0,
            color: MUTED,
            bold: false,
            anchor: 0.0,
            text: sub.join(", "),
        });
    }
    shapes.push(Shape::Text {
        at: [PAGE_W - MARGIN, MARGIN + 12.0],
        angle: 0.0,
        size: 9.0,
        color: MUTED,
        bold: false,
        anchor: 1.0,
        text: info.stamp.clone(),
    });
    shapes.push(Shape::Line {
        a: [MARGIN, MARGIN + 34.0],
        b: [PAGE_W - MARGIN, MARGIN + 34.0],
        color: MUTED,
        width: 0.5,
        dashed: false,
    });

    // Fit the plan into the box between head and foot.
    let mut pts: Vec<[f64; 2]> = Vec::new();
    for g in &geos {
        pts.extend(
            g.faces
                .iter()
                .flat_map(|f| f.polygon.iter().map(|p| [p[0], p[1]])),
        );
        pts.extend(
            g.lines
                .iter()
                .flat_map(|l| [[l.a[0], l.a[1]], [l.b[0], l.b[1]]]),
        );
    }
    for part in &project.parts {
        pts.extend(part.edges.iter().map(|e| [e.start.x, e.start.y]));
    }
    let foot_h = 46.0;
    let (bx0, by0, bx1, by1) = (
        MARGIN + 20.0,
        MARGIN + 60.0,
        PAGE_W - MARGIN - 20.0,
        PAGE_H - MARGIN - foot_h - 20.0,
    );
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for p in pts.iter().filter(|p| p[0].is_finite() && p[1].is_finite()) {
        x0 = x0.min(p[0]);
        y0 = y0.min(p[1]);
        x1 = x1.max(p[0]);
        y1 = y1.max(p[1]);
    }
    if x0 > x1 {
        (x0, y0, x1, y1) = (0.0, 0.0, 1.0, 1.0);
    }
    let scale = ((bx1 - bx0) / (x1 - x0).max(1e-6)).min((by1 - by0) / (y1 - y0).max(1e-6));
    let ox = bx0 + ((bx1 - bx0) - (x1 - x0) * scale) / 2.0;
    let oy = by0 + ((by1 - by0) - (y1 - y0) * scale) / 2.0;
    let map = |x: f64, y: f64| [ox + (x - x0) * scale, oy + (y1 - y) * scale];

    // Walls: thin dashed grey.
    for part in &project.parts {
        let n = part.edges.len();
        for i in 0..n {
            let (a, b) = (part.edges[i].start, part.edges[(i + 1) % n].start);
            shapes.push(Shape::Line {
                a: map(a.x, a.y),
                b: map(b.x, b.y),
                color: MUTED,
                width: 0.5,
                dashed: true,
            });
        }
    }
    for ch in project.chimneys.iter().chain(&project.roof_openings) {
        let n = ch.corners.len();
        for i in 0..n {
            let (a, b) = (ch.corners[i], ch.corners[(i + 1) % n]);
            shapes.push(Shape::Line {
                a: map(a.x, a.y),
                b: map(b.x, b.y),
                color: MUTED,
                width: 0.8,
                dashed: false,
            });
        }
    }

    // Roof lines with their true length along them.
    let label = (7.5_f64).min((scale * 0.45).max(5.0));
    for g in &geos {
        for l in &g.lines {
            let (a, b) = (map(l.a[0], l.a[1]), map(l.b[0], l.b[1]));
            let color = kind_color(l.kind);
            shapes.push(Shape::Line {
                a,
                b,
                color,
                width: 1.4,
                dashed: false,
            });
            let plan = (b[0] - a[0]).hypot(b[1] - a[1]);
            if plan < 18.0 {
                continue;
            }
            // Upright text along the line, just off it.
            let mut ang = -(b[1] - a[1]).atan2(b[0] - a[0]).to_degrees();
            if ang > 90.0 {
                ang -= 180.0;
            } else if ang <= -90.0 {
                ang += 180.0;
            }
            let r = ang.to_radians();
            let (nx, ny) = (-r.sin(), -r.cos());
            let mid = [
                (a[0] + b[0]) / 2.0 + nx * 2.5,
                (a[1] + b[1]) / 2.0 + ny * 2.5,
            ];
            shapes.push(Shape::Text {
                at: mid,
                angle: ang,
                size: label,
                color,
                bold: false,
                anchor: 0.5,
                text: format!("{}m", num(l.length(), d)),
            });
        }
    }

    // Faces: area and pitch at the middle of the face.
    for g in &geos {
        for f in &g.faces {
            let poly: Vec<[f64; 2]> = f.polygon.iter().map(|p| map(p[0], p[1])).collect();
            let Some(c) = centroid(&poly) else { continue };
            shapes.push(Shape::Text {
                at: [c[0], c[1] - 1.0],
                angle: 0.0,
                size: 8.5,
                color: INK,
                bold: false,
                anchor: 0.5,
                text: format!("{}m²", num(f.area, d)),
            });
            shapes.push(Shape::Text {
                at: [c[0], c[1] + 10.0],
                angle: 0.0,
                size: 9.5,
                color: BRAND,
                bold: true,
                anchor: 0.5,
                text: format!("{}°", num(f.pitch_deg, 2)),
            });
        }
    }

    // Foot: total area and line totals by code.
    let fy = PAGE_H - MARGIN - foot_h;
    shapes.push(Shape::Line {
        a: [MARGIN, fy],
        b: [PAGE_W - MARGIN, fy],
        color: MUTED,
        width: 0.5,
        dashed: false,
    });
    let q = crate::report::calc(project);
    let total = q["totals"]["roof_area"].as_f64().unwrap_or(0.0);
    shapes.push(Shape::Text {
        at: [MARGIN, fy + 18.0],
        angle: 0.0,
        size: 11.0,
        color: INK,
        bold: true,
        anchor: 0.0,
        text: format!("A = {} m²", num(total, d)),
    });
    let mut x = MARGIN + 150.0;
    let codes = crate::quantities::dacad_line_totals(project);
    for (code, v) in codes.iter().filter(|(_, v)| v.abs() > 1e-9) {
        let kind = match code.as_str() {
            "T" => Some(LineKind::Eave),
            "F" => Some(LineKind::Ridge),
            "G" => Some(LineKind::Hip),
            "K" => Some(LineKind::Valley),
            "OL" | "OR" | "OS" | "O" => Some(LineKind::Verge),
            "P" => Some(LineKind::Pult),
            "c" => Some(LineKind::Knick),
            "S" => Some(LineKind::Gorge),
            _ => None,
        };
        if x > PAGE_W - MARGIN - 60.0 {
            break;
        }
        shapes.push(Shape::Text {
            at: [x, fy + 18.0],
            angle: 0.0,
            size: 9.0,
            color: kind.map_or(MUTED, kind_color),
            bold: true,
            anchor: 0.0,
            text: code.clone(),
        });
        shapes.push(Shape::Text {
            at: [x + 16.0, fy + 18.0],
            angle: 0.0,
            size: 9.0,
            color: INK,
            bold: false,
            anchor: 0.0,
            text: format!("{} m", num(*v, d)),
        });
        x += 92.0;
    }
    shapes.push(Shape::Text {
        at: [MARGIN, fy + 36.0],
        angle: 0.0,
        size: 7.0,
        color: MUTED,
        bold: false,
        anchor: 0.0,
        text: format!("1 : {}", scale_label(scale)),
    });
    shapes
}

/// The drawing scale, 1 : n, rounded (1 m = `scale` points; 1 pt = 25.4/72 mm).
fn scale_label(scale: f64) -> String {
    let n = 1000.0 / (scale * 25.4 / 72.0);
    format!("{}", (n / 5.0).round().max(1.0) * 5.0)
}

fn centroid(poly: &[[f64; 2]]) -> Option<[f64; 2]> {
    let n = poly.len();
    if n < 3 {
        return None;
    }
    let (mut a, mut cx, mut cy) = (0.0, 0.0, 0.0);
    for i in 0..n {
        let (p, q) = (poly[i], poly[(i + 1) % n]);
        let cr = p[0] * q[1] - q[0] * p[1];
        a += cr;
        cx += (p[0] + q[0]) * cr;
        cy += (p[1] + q[1]) * cr;
    }
    if a.abs() < 1e-9 {
        let m = poly
            .iter()
            .fold([0.0, 0.0], |s, p| [s[0] + p[0], s[1] + p[1]]);
        return Some([m[0] / n as f64, m[1] / n as f64]);
    }
    Some([cx / (3.0 * a), cy / (3.0 * a)])
}

fn esc_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The sheet as an SVG document (A4 landscape, white paper).
pub fn sheet_svg(shapes: &[Shape]) -> String {
    let f = |v: f64| format!("{:.2}", v);
    let mut s = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}pt\" height=\"{h}pt\" viewBox=\"0 0 {w} {h}\">\n<rect width=\"{w}\" height=\"{h}\" fill=\"#ffffff\"/>\n<g font-family=\"Helvetica, Arial, sans-serif\" stroke-linecap=\"round\">\n",
        w = f(PAGE_W),
        h = f(PAGE_H)
    );
    for sh in shapes {
        match sh {
            Shape::Line {
                a,
                b,
                color,
                width,
                dashed,
            } => {
                let dash = if *dashed {
                    " stroke-dasharray=\"3 2\""
                } else {
                    ""
                };
                let _ = writeln!(
                    s,
                    "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{color}\" stroke-width=\"{}\"{dash}/>",
                    f(a[0]),
                    f(a[1]),
                    f(b[0]),
                    f(b[1]),
                    f(*width)
                );
            }
            Shape::Text {
                at,
                angle,
                size,
                color,
                bold,
                anchor,
                text,
            } => {
                let anchor = if *anchor < 0.25 {
                    "start"
                } else if *anchor > 0.75 {
                    "end"
                } else {
                    "middle"
                };
                let weight = if *bold { " font-weight=\"bold\"" } else { "" };
                let rot = if angle.abs() > 1e-6 {
                    format!(
                        " transform=\"rotate({} {} {})\"",
                        f(-angle),
                        f(at[0]),
                        f(at[1])
                    )
                } else {
                    String::new()
                };
                let _ = writeln!(
                    s,
                    "<text x=\"{}\" y=\"{}\" font-size=\"{}\" fill=\"{color}\" text-anchor=\"{anchor}\"{weight}{rot}>{}</text>",
                    f(at[0]),
                    f(at[1]),
                    f(*size),
                    esc_xml(text)
                );
            }
        }
    }
    s + "</g>\n</svg>\n"
}

/// Helvetica glyph widths (Adobe AFM, 1/1000 em) for ASCII 32-126.
const HELVETICA: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556,
    556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556, 1015, 667, 667, 722, 722, 667,
    611, 778, 722, 278, 500, 667, 556, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667,
    667, 611, 278, 278, 278, 469, 556, 333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500,
    222, 833, 556, 556, 556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584,
];

/// A character in PDF's WinAnsiEncoding (cp1252), letters it lacks without
/// their accent (č -> c), anything else as "?".
fn win_ansi(c: char) -> u8 {
    let u = c as u32;
    if (0x20..0x7f).contains(&u) || (0xa0..=0xff).contains(&u) {
        return u as u8;
    }
    match c {
        '€' => 0x80,
        '‚' => 0x82,
        '„' => 0x84,
        '…' => 0x85,
        '–' => 0x96,
        '—' => 0x97,
        'Š' => 0x8a,
        'š' => 0x9a,
        'Ž' => 0x8e,
        'ž' => 0x9e,
        'Ÿ' => 0x9f,
        '·' => 0xb7,
        'Σ' => b'S',
        'č' | 'ć' => b'c',
        'Č' | 'Ć' => b'C',
        'ď' => b'd',
        'Ď' => b'D',
        'ě' | 'ę' => b'e',
        'Ě' | 'Ę' => b'E',
        'ľ' | 'ĺ' | 'ł' => b'l',
        'Ľ' | 'Ĺ' | 'Ł' => b'L',
        'ň' | 'ń' => b'n',
        'Ň' | 'Ń' => b'N',
        'ř' | 'ŕ' => b'r',
        'Ř' | 'Ŕ' => b'R',
        'ť' => b't',
        'Ť' => b'T',
        'ů' | 'ű' => b'u',
        'Ů' | 'Ű' => b'U',
        'ő' => b'o',
        'Ő' => b'O',
        'ś' => b's',
        'Ś' => b'S',
        'ź' | 'ż' => b'z',
        'Ź' | 'Ż' => b'Z',
        _ => b'?',
    }
}

fn text_width(text: &[u8], size: f64, bold: bool) -> f64 {
    let w: f64 = text
        .iter()
        .map(|&b| match b {
            32..=126 => f64::from(HELVETICA[usize::from(b - 32)]),
            0xb2 => 333.0,
            0xb0 => 400.0,
            0xb7 => 278.0,
            _ => 556.0,
        })
        .sum();
    w * size / 1000.0 * if bold { 1.05 } else { 1.0 }
}

fn rgb(hex: &str) -> String {
    let v = u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap_or(0);
    let c = |s: u32| format!("{:.3}", f64::from((v >> s) & 0xff) / 255.0);
    format!("{} {} {}", c(16), c(8), c(0))
}

/// The sheet as a one-page PDF (vector, Helvetica; no fonts embedded).
pub fn sheet_pdf(shapes: &[Shape], title: &str) -> Vec<u8> {
    let f = |v: f64| format!("{:.2}", v);
    let mut c = String::new();
    for sh in shapes {
        match sh {
            Shape::Line {
                a,
                b,
                color,
                width,
                dashed,
            } => {
                let dash = if *dashed { "[3 2] 0 d" } else { "[] 0 d" };
                let _ = writeln!(
                    c,
                    "{} RG {} w 1 J {dash} {} {} m {} {} l S",
                    rgb(color),
                    f(*width),
                    f(a[0]),
                    f(PAGE_H - a[1]),
                    f(b[0]),
                    f(PAGE_H - b[1])
                );
            }
            Shape::Text {
                at,
                angle,
                size,
                color,
                bold,
                anchor,
                text,
            } => {
                let bytes: Vec<u8> = text.chars().map(win_ansi).collect();
                let w = text_width(&bytes, *size, *bold) * anchor;
                let r = angle.to_radians();
                let (cs, sn) = (r.cos(), r.sin());
                let (x, y) = (at[0] - w * cs, PAGE_H - at[1] - w * sn);
                let mut lit = String::new();
                for b in bytes {
                    match b {
                        b'(' | b')' | b'\\' => {
                            lit.push('\\');
                            lit.push(b as char);
                        }
                        32..=126 => lit.push(b as char),
                        _ => {
                            let _ = write!(lit, "\\{:03o}", b);
                        }
                    }
                }
                let font = if *bold { "F2" } else { "F1" };
                let _ = writeln!(
                    c,
                    "BT {} rg /{font} {} Tf {:.4} {:.4} {:.4} {:.4} {} {} Tm ({lit}) Tj ET",
                    rgb(color),
                    f(*size),
                    cs,
                    sn,
                    -sn,
                    cs,
                    f(x),
                    f(y)
                );
            }
        }
    }
    let title: String = title
        .chars()
        .map(win_ansi)
        .filter(|b| (32..=126).contains(b) && !matches!(b, b'(' | b')' | b'\\'))
        .map(char::from)
        .collect();
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Resources << /Font << /F1 5 0 R /F2 6 0 R >> >> /Contents 4 0 R >>",
            f(PAGE_W),
            f(PAGE_H)
        ),
        format!("<< /Length {} >>\nstream\n{c}endstream", c.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
            .to_string(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>"
            .to_string(),
        format!("<< /Title ({title}) /Producer (Tectora) >>"),
    ];
    let mut out = b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    let mut tail = format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1);
    for o in offsets {
        let _ = writeln!(tail, "{o:010} 00000 n ");
    }
    let _ = write!(
        tail,
        "trailer\n<< /Size {} /Root 1 0 R /Info 7 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        objects.len() + 1
    );
    out.extend_from_slice(tail.as_bytes());
    out
}

/// The list and the plan sheet (SVG) of a project from the app or an AI
/// client: the project is checked against the model's limits first and a
/// failing calculation is an error, never a panic.
pub fn checked(project: &Project, opts: &ListOptions) -> Result<(String, String), String> {
    project.check_limits()?;
    crate::model::guarded(|| {
        let info = SheetInfo {
            stamp: opts.stamp.clone(),
            decimals: opts.decimals,
        };
        (
            dacad_list(project, opts),
            sheet_svg(&plan_sheet(project, &info)),
        )
    })
}

/// The plan sheet as a file: `pdf` or `svg` bytes, checked like [`checked`].
pub fn checked_sheet_file(project: &Project, stamp: &str, format: &str) -> Result<Vec<u8>, String> {
    project.check_limits()?;
    let info = SheetInfo {
        stamp: stamp.to_string(),
        decimals: 3,
    };
    match format {
        "pdf" => crate::model::guarded(|| sheet_pdf(&plan_sheet(project, &info), &project.name)),
        "svg" => crate::model::guarded(|| sheet_svg(&plan_sheet(project, &info)).into_bytes()),
        other => Err(format!("unknown plan format `{other}` (pdf or svg)")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::examples::{StandardRoof, standard_roof};

    /// The 10 × 8 m hip roof drawn in DACAD under Wine on 2026-11-08:
    /// pitch 30°, eave height 3 m, overhang 0.5 m, tiles, millimetre
    /// precision. Every line below is copied from the list DACAD wrote.
    fn dacad_hip() -> Project {
        let mut p = Project::new("hip");
        p.parts
            .push(standard_roof(StandardRoof::Hip, 10.0, 8.0, 30.0, 3.0, 0.5));
        p
    }

    /// The L-shaped hip roof of a TERRAN job, two 15 m wings 8 m deep:
    /// pitch 38°, eave height 3 m, no overhang, tiles.
    fn terran_l_roof() -> Project {
        let mut p = Project::new("L");
        let mut part = standard_roof(StandardRoof::Hip, 15.0, 8.0, 38.0, 3.0, 0.0);
        let e = part.edges[0].clone();
        part.edges = [
            (0.0, 0.0),
            (15.0, 0.0),
            (15.0, 8.0),
            (8.0, 8.0),
            (8.0, 15.0),
            (0.0, 15.0),
        ]
        .iter()
        .map(|&(x, y)| crate::model::EdgeDef {
            start: crate::geom::Point2::new(x, y),
            ..e.clone()
        })
        .collect();
        p.parts.push(part);
        p
    }

    /// The list TERRAN's DACAD wrote for [`terran_l_roof`] (sent by TERRAN
    /// on 2026-11-08, from the first separator on).
    const TERRAN_L_LIST: &str = "\
------------------------------------------------------------
STREŠNÉ PLOCHY
------------------------------------------------------------
Z1.01 38,00° 55,837m˛ Dĺžka krokvy=5,076m
Z1.02 38,00° 20,304m˛ Dĺžka krokvy=5,076m
Z1.03 38,00° 35,533m˛ Dĺžka krokvy=5,076m
Z1.04 38,00° 35,533m˛ Dĺžka krokvy=5,076m
Z1.05 38,00° 20,304m˛ Dĺžka krokvy=5,076m
Z1.06 38,00° 55,837m˛ Dĺžka krokvy=5,076m
------------------------------------------------------------


------------------------------------------------------------
PLOCHY
------------------------------------------------------------
Stena : 1 180,001m˛
Škridla -  223,347m˛


------------------------------------------------------------
LÍNIE
------------------------------------------------------------
1 Hrebeň - 14,000m
2 Nárožie - 32,313m
3 Úžlabie - 6,463m
6 Odkvap - 60,000m
";

    #[test]
    fn the_list_is_terrans_dacad_list() {
        let text = dacad_list(&terran_l_roof(), &ListOptions::new("L", "08.11.2026 16:26"));
        let head = format!("L{}08.11.2026 16:26\n\n", " ".repeat(43));
        assert_eq!(text, format!("{head}{TERRAN_L_LIST}"));
    }

    /// What TERRAN's price sheet (`__VYPOCTY_<date> CZ.xls`, sheet DATA)
    /// reads from a list pasted at DH69: its per-row formulas, with Excel's
    /// 1-based character positions. Face areas by row (only rows 73-115
    /// count), line totals by word (`MID(DH, FIND(" ")+1, FIND("-")-4)`, the
    /// value `MID(DH, FIND("-")+2, LEN(DH)-FIND("-")-2)`), the tile total
    /// by its first word.
    struct Sheet {
        faces: Vec<(usize, f64)>,
        lines: Vec<(String, f64)>,
        /// Line totals by leading code (`LEFT(DH, FIND(" ")-1)`, the
        /// abutments 91-93 in cell AD68).
        codes: Vec<(String, f64)>,
        tiles: Option<String>,
    }

    fn sheet(list: &str) -> Sheet {
        let num = |t: &str| t.trim().replace(',', ".").parse::<f64>().ok();
        let mut out = Sheet {
            faces: Vec::new(),
            lines: Vec::new(),
            codes: Vec::new(),
            tiles: None,
        };
        for (i, row) in list.split('\n').enumerate() {
            let c: Vec<char> = row.chars().collect();
            let find = |pat: &str| row.find(pat).map(|b| row[..b].chars().count() + 1);
            let mid =
                |start: usize, n: usize| -> String { c.iter().skip(start - 1).take(n).collect() };
            if let (Some(deg), Some(m2)) = (find("°"), find("m\u{2db}"))
                && !row.starts_with(['k', 'K'])
                && m2 > deg + 2
                && let Some(a) = num(&mid(deg + 2, m2 - deg - 2))
            {
                out.faces.push((69 + i, a));
            }
            if let (Some(sp), Some(dash)) = (find(" "), find("-"))
                && dash >= 4
                && !row.starts_with('-')
                && let Some(v) = num(&mid(dash + 2, c.len().saturating_sub(dash + 2)))
            {
                out.lines.push((mid(sp + 1, dash - 4), v));
                out.codes.push((mid(1, sp - 1), v));
            }
            if row.starts_with('Š') {
                out.tiles = find("-").map(|d| mid(1, d - 2));
            }
        }
        out
    }

    #[test]
    fn the_price_sheet_reads_every_face_and_total() {
        let text = dacad_list(&terran_l_roof(), &ListOptions::new("L", "08.11.2026 16:26"));
        let s = sheet(&text);
        assert_eq!(s.faces.len(), 6, "{text}");
        assert!(
            s.faces.iter().all(|(row, _)| (73..=115).contains(row)),
            "{:?}",
            s.faces
        );
        let sum: f64 = s.faces.iter().map(|f| f.1).sum();
        assert!((sum - 223.348).abs() < 1e-9, "{sum}");
        for (word, v) in [
            ("Hrebeň", 14.0),
            ("Nárožie", 32.313),
            ("Úžlabie", 6.463),
            ("Odkvap", 60.0),
        ] {
            let got = s.lines.iter().find(|(w, _)| w == word).map(|l| l.1);
            assert_eq!(got, Some(v), "{word} in {:?}", s.lines);
        }
        assert_eq!(s.tiles.as_deref(), Some("Škridla"));
    }

    /// Every kind of roof the editor makes: the standard roofs and the L, T
    /// and U buildings, each as it is and with a dormer, a chimney, a roof
    /// window, a hand-drawn face, a clad gable and sheet metal on one wall;
    /// two buildings side by side; a flat roof.
    fn every_kind_of_roof() -> Vec<(String, Project)> {
        use crate::edit::dormers::{DORMER_KINDS, build_dormer, default_params};
        use crate::geom::Point2;
        let mut out = Vec::new();
        for (k, ex) in crate::examples::EXAMPLES.iter().enumerate() {
            let base = ex.project();
            let pts: Vec<Point2> = base.parts.iter().flat_map(|p| p.outline()).collect();
            let (x0, y0) = pts
                .iter()
                .fold((f64::MAX, f64::MAX), |a, p| (a.0.min(p.x), a.1.min(p.y)));
            let x1 = pts.iter().fold(f64::MIN, |a, p| a.max(p.x));
            let eave = base.parts[0].edges[0].eave_height;
            let mid = x0 + (x1 - x0) * 0.4;
            out.push((ex.id.to_string(), base.clone()));

            let mut p = base.clone();
            let kind = DORMER_KINDS[k % DORMER_KINDS.len()];
            // Front 1.2 m inside the first wall, its eave 1 m above the roof
            // there, as the editor places it.
            let host = eave + 1.2 * base.parts[0].edges[0].pitch_deg.to_radians().tan();
            let at = (host * 100.0).round() / 100.0 + 1.0;
            p.dormers.push(build_dormer(&default_params(
                kind,
                Point2::new(mid, y0 + 1.2),
                0.0,
                at,
            )));
            out.push((format!("{} + dormer {kind:?}", ex.id), p));

            let mut p = base.clone();
            let (cx, cy) = (mid, y0 + 2.0);
            p.chimneys.push(crate::model::Chimney {
                corners: vec![
                    Point2::new(cx, cy),
                    Point2::new(cx + 0.6, cy),
                    Point2::new(cx + 0.6, cy + 0.6),
                    Point2::new(cx, cy + 0.6),
                ],
                height: eave + 4.0,
            });
            p.windows.push(crate::model::RoofWindow {
                x: mid + 1.5,
                y: y0 + 1.5,
                width: 0.78,
                height: 1.18,
            });
            out.push((format!("{} + chimney + window", ex.id), p));

            let mut p = base.clone();
            for e in p.parts[0].edges.iter_mut() {
                if e.pitch_deg >= crate::model::GABLE_PITCH_DEG {
                    e.clad = true;
                }
            }
            p.parts[0].edges[0].material = 2;
            p.manual_faces.push(crate::model::ManualFace {
                points: vec![
                    Point2::new(0.0, 0.0),
                    Point2::new(2.0, 0.0),
                    Point2::new(2.0, 1.0),
                ],
                pitch_deg: 15.0,
                label: String::new(),
            });
            out.push((format!("{} + clad gables, metal, manual face", ex.id), p));
        }
        let mut two = terran_l_roof();
        two.parts.push(standard_roof(
            StandardRoof::Gable,
            10.0,
            8.0,
            38.0,
            3.0,
            0.0,
        ));
        for e in two.parts[1].edges.iter_mut() {
            e.start.x += 15.0;
        }
        out.push(("two buildings".into(), two));
        let mut flat = dacad_hip();
        flat.flat_roof = true;
        out.push(("flat roof".into(), flat));
        out
    }

    /// Whether a list row has one of the shapes DACAD's list has.
    fn known_row(row: &str) -> bool {
        let number = |t: &str| {
            let t = t.strip_prefix('-').unwrap_or(t);
            let (a, b) = t.split_once(',').unwrap_or(("", ""));
            !a.is_empty()
                && a.chars().all(|c| c.is_ascii_digit())
                && b.len() == 3
                && b.chars().all(|c| c.is_ascii_digit())
        };
        let m2 = |t: &str| t.strip_suffix("m\u{2db}").is_some_and(number);
        let m = |t: &str| t.strip_suffix('m').is_some_and(number);
        if row.is_empty() || row == DASHES || ["STREŠNÉ PLOCHY", "PLOCHY", "LÍNIE"].contains(&row)
        {
            return true;
        }
        let w: Vec<&str> = row.split(' ').collect();
        if row.starts_with('Z') && row[1..].starts_with(|c: char| c.is_ascii_digit()) {
            // Z1.01 38,00° 55,837m˛ [Dĺžka krokvy=5,076m] or Z1.01:1 38,00° -1,000m˛
            let pitch = w.get(1).and_then(|p| p.strip_suffix('°')).is_some_and(|p| {
                let (a, b) = p.split_once(',').unwrap_or(("", ""));
                !a.is_empty() && b.len() == 2
            });
            let rafter = match w.get(3) {
                None => true,
                Some(r) => {
                    w.len() == 5 && *r == "Dĺžka" && w[4].strip_prefix("krokvy=").is_some_and(m)
                }
            };
            return pitch && w.len() >= 3 && m2(w[2]) && rafter;
        }
        if let Some(rest) = row.strip_prefix("   Dach-Nummer ") {
            return rest.split_once(": normal: S=").is_some_and(|(_, a)| m2(a));
        }
        if let Some(rest) = row.strip_prefix("Stena : 1 ") {
            return m2(rest);
        }
        let last = w.last().copied().unwrap_or("");
        m2(last) || (row.contains(" - ") && m(last))
    }

    #[test]
    fn every_kind_of_roof_exports_a_list_the_price_sheet_reads() {
        for (name, p) in every_kind_of_roof() {
            let text = dacad_list(&p, &ListOptions::new(&name, "08.11.2026 20:30"));
            for row in text.lines().skip(1) {
                assert!(known_row(row), "{name}: row {row:?} in\n{text}");
            }
            let s = sheet(&text);
            let geos = crate::openings::generate_project(&p);
            let shown = |a: f64| a >= 0.0005;
            let n_faces = geos
                .iter()
                .map(|g| {
                    g.faces
                        .iter()
                        .filter(|f| shown(f.area + f.deducted))
                        .count()
                        + g.clad_faces.iter().filter(|f| shown(f.area)).count()
                })
                .sum::<usize>()
                + p.manual_faces
                    .iter()
                    .filter(|f| f.pitch_counts() && shown(f.area()))
                    .count();
            // One sheet row per face with an area (deductions read as 0),
            // all in reach.
            let faces: Vec<_> = s.faces.iter().filter(|f| f.1 > 0.0).collect();
            assert_eq!(faces.len(), n_faces, "{name}:\n{text}");
            if n_faces <= 41 {
                assert!(
                    faces.iter().all(|f| (73..=115).contains(&f.0)),
                    "{name}:\n{text}"
                );
            }
            // Line totals as the sheet looks them up, against the quantities.
            let totals = crate::quantities::dacad_line_totals(&p);
            for (code, word) in [
                ("F", "Hrebeň"),
                ("G", "Nárožie"),
                ("K", "Úžlabie"),
                ("OL", "Krajné ľavé"),
                ("OR", "Krajné pravé"),
                ("T", "Odkvap"),
                ("P", "Pult"),
            ] {
                let want = totals.get(code).copied().unwrap_or(0.0);
                let got = s.lines.iter().find(|(w, _)| w == word).map_or(0.0, |l| l.1);
                assert!(
                    (got - want).abs() < 0.011,
                    "{name}: {word} {got} vs {want}\n{text}"
                );
            }
            for (code, sheet_code) in [("WV", "91"), ("WS", "92"), ("WH", "93")] {
                let want = totals.get(code).copied().unwrap_or(0.0);
                let got = s
                    .codes
                    .iter()
                    .find(|(c, _)| c == sheet_code)
                    .map_or(0.0, |l| l.1);
                assert!(
                    (got - want).abs() < 0.011,
                    "{name}: {code} {got} vs {want}\n{text}"
                );
            }
            // The tile area is the net roof area of the tiles.
            let q = crate::quantities::roof_totals(&p);
            let tiles: f64 = q
                .area_by_material
                .iter()
                .filter(|m| m.0 == 1)
                .map(|m| m.1)
                .sum();
            if tiles > 0.0 {
                assert_eq!(s.tiles.as_deref(), Some("Škridla"), "{name}:\n{text}");
                let row = text.lines().find(|r| r.starts_with("Škridla -  ")).unwrap();
                let v: f64 = row["Škridla -  ".len()..]
                    .trim_end_matches("m\u{2db}")
                    .replace(',', ".")
                    .parse()
                    .unwrap();
                assert!(
                    (v - tiles).abs() < 0.011,
                    "{name}: tiles {v} vs {tiles}\n{text}"
                );
            }
        }
    }

    #[test]
    fn the_list_names_every_line_type_as_the_sheet_looks_it_up() {
        // The words of the sheet's VLOOKUPs (cells AD60-AD67).
        let words = [
            "Hrebeň",
            "Nárožie",
            "Úžlabie",
            "Krajné ľavé",
            "Krajné pravé",
            "Odkvap",
            "Pult",
        ];
        for code in ["F", "G", "K", "OL", "OR", "T", "P"] {
            let row = format!("{} 1,000m", line_group(code));
            let s = sheet(&row);
            assert!(
                words.contains(&s.lines[0].0.as_str()),
                "{code}: {:?}",
                s.lines
            );
            assert_eq!(s.lines[0].1, 1.0);
        }
    }

    #[test]
    fn the_list_has_dacads_values_for_a_hip_roof() {
        let text = dacad_list(&dacad_hip(), &ListOptions::new("", "08.11.2026 13:02"));
        let lines: Vec<&str> = text.lines().collect();
        // Head: padded to 60 characters.
        assert_eq!(lines[0].chars().count(), 60, "{:?}", lines[0]);
        assert!(lines[0].ends_with("08.11.2026 13:02"));
        // Every value as DACAD wrote it for this roof.
        for want in [
            "Z1.01 30,00° 33,775m˛ Dĺžka krokvy=5,196m",
            "Z1.02 30,00° 23,383m˛ Dĺžka krokvy=5,196m",
            "Škridla -  114,315m˛",
            "1 Hrebeň - 2,000m",
            "2 Nárožie - 27,495m",
            "6 Odkvap - 40,000m",
        ] {
            assert!(lines.contains(&want), "missing {want:?} in\n{text}");
        }
        // Sections start after two empty lines, like DACAD's "\n\n".
        let i = lines.iter().position(|l| *l == "PLOCHY").unwrap();
        assert_eq!(&lines[i - 3..i], &["", "", DASHES]);
    }

    #[test]
    fn the_list_names_verges_left_and_right_for_a_gable_roof() {
        let mut p = Project::new("gable");
        p.parts.push(standard_roof(
            StandardRoof::Gable,
            14.0,
            7.5,
            33.7,
            3.0,
            0.0,
        ));
        let text = dacad_list(&p, &ListOptions::new("gable.tectora", "01.01.2027 08:00"));
        assert!(text.contains("\n4 Krajné ľavé - "), "{text}");
        assert!(text.contains("\n5 Krajné pravé - "), "{text}");
        // Walls with their gable triangles: 2 × 14 × 3 + 2 × 7.5 × 3 + 2 × 9.38.
        assert!(text.contains("\nStena : 1 147,7"), "{text}");
        // Two faces of 14 × 4.507 m.
        assert_eq!(text.matches("33,70° 63,10").count(), 2, "{text}");
    }

    /// A 12 × 8 m gable roof with a 2 m gable dormer on its front face.
    fn gable_with_dormer() -> Project {
        use crate::edit::dormers::{DormerKind, build_dormer, default_params};
        let mut p = Project::new("dormer");
        p.parts.push(standard_roof(
            StandardRoof::Gable,
            12.0,
            8.0,
            40.0,
            3.0,
            0.5,
        ));
        let d = default_params(
            DormerKind::Gable,
            crate::geom::Point2::new(5.0, 1.0),
            0.0,
            5.0,
        );
        p.dormers.push(build_dormer(&d));
        p
    }

    #[test]
    fn the_list_names_dormer_faces_after_the_walls() {
        let p = gable_with_dormer();
        let geos = crate::openings::generate_project(&p);
        assert!(
            geos[0].faces.iter().any(|f| f.edge == usize::MAX),
            "the dormer is cut into the roof"
        );
        // Used to overflow (`edge` of a dormer face is usize::MAX): a panic
        // in debug builds, every dormer face named Z1.00 in release.
        let text = dacad_list(&p, &ListOptions::new("", "08.11.2026 13:02"));
        assert!(!text.contains("Z1.00"), "{text}");
        let names: Vec<&str> = text
            .lines()
            .filter(|l| l.starts_with("Z1."))
            .map(|l| l.split(' ').next().unwrap())
            .collect();
        let mut unique = names.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), names.len(), "{names:?}");
        // Four walls, so the dormer faces follow as Z1.05 and on.
        assert!(names.contains(&"Z1.05"), "{names:?}");
        // The plan sheet takes the dormer too.
        assert!(!plan_sheet(&p, &SheetInfo::default()).is_empty());
    }

    #[test]
    fn numbers_use_a_decimal_comma_and_no_negative_zero() {
        assert_eq!(num(1.23456, 3), "1,235");
        assert_eq!(num(-0.0001, 2), "0,00");
        assert_eq!(num(-1.5, 1), "-1,5");
    }

    #[test]
    fn utc_stamps_follow_the_calendar() {
        assert_eq!(utc_stamp(0), "01.01.1970 00:00");
        // 2026-11-08 12:44 UTC.
        assert_eq!(utc_stamp(1_794_141_840_000), "08.11.2026 12:44");
        // Leap day.
        assert_eq!(utc_stamp(951_782_400_000), "29.02.2000 00:00");
    }

    #[test]
    fn the_sheet_labels_every_face_with_area_and_pitch() {
        let p = dacad_hip();
        let shapes = plan_sheet(
            &p,
            &SheetInfo {
                stamp: "x".into(),
                decimals: 3,
            },
        );
        let texts: Vec<&str> = shapes
            .iter()
            .filter_map(|s| match s {
                Shape::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(texts.iter().filter(|t| **t == "30,00°").count(), 4);
        assert_eq!(texts.iter().filter(|t| **t == "33,775m²").count(), 2);
        assert!(texts.contains(&"6,874m"));
        assert!(texts.contains(&"A = 114,315 m²"));
        let svg = sheet_svg(&shapes);
        assert!(svg.starts_with("<svg") && svg.contains("30,00°"));
        let pdf = sheet_pdf(&shapes, "Střecha (Novák)");
        assert!(pdf.starts_with(b"%PDF-1.4"));
        assert!(pdf.ends_with(b"%%EOF\n"));
        // The degree sign in WinAnsi, octal 260.
        assert!(pdf.windows(4).any(|w| w == b"\\260"));
        // The xref offsets point at the objects.
        let at = pdf.windows(9).position(|w| w == b"xref\n0 8\n").unwrap();
        let entry = std::str::from_utf8(&pdf[at + 9 + 20..at + 9 + 30]).unwrap();
        let first: usize = entry.parse().unwrap();
        assert!(pdf[first..].starts_with(b"1 0 obj"));
    }

    #[test]
    fn an_empty_project_still_exports() {
        let p = Project::new("empty");
        let text = dacad_list(&p, &ListOptions::new("", ""));
        assert!(text.contains("LÍNIE") && !text.contains("Stena"));
        let shapes = plan_sheet(&p, &SheetInfo::default());
        assert!(!sheet_pdf(&shapes, "").is_empty());
    }
}
