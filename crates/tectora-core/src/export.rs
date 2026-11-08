//! Exports of a finished roof for the offer: the plan drawing with line
//! lengths, face areas and pitches (SVG and PDF), and the text list the
//! user pastes into the price sheet.
//!
//! The text list reproduces DACAD's "Liste" (Data > Listen, DACAD.exe
//! FUN_00d0ac30, string table 1750-1782) as TERRAN's DACAD build writes it:
//! German section heads and Slovak line and material names with their
//! numeric prefixes (`6 Odkvap -`, `Škridla - `). TERRAN's price sheet looks
//! up exactly these words, so they are the export payload and stay as they
//! are in every UI language. Checked against lists DACAD wrote for a
//! 10 × 8 m hip roof and its Helpsample.dad (2026-11-08, see
//! `tests` below).
//!
//! What the list does not have yet: DACAD's wall areas (`Stena : n`, its
//! "calculate walls" option), the dormer roofs as their own `Dach-Nummer`,
//! and the per-hole split of deductions (one `Zn.ww:1` entry per face sums
//! the windows and chimneys of that face). The order of the members inside a
//! group differs from DACAD's (it follows DACAD's internal tables); names,
//! values and group totals do not.

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

struct ListLine {
    name: String,
    code: String,
    pitch: f64,
    length: f64,
    a: [f64; 3],
    b: [f64; 3],
}

struct ListFace {
    name: String,
    pitch: f64,
    area: f64,
    rafter: Option<f64>,
    material: u8,
    lines: Vec<usize>,
    /// Windows and chimneys cut out of the face, as one negative entry.
    holes: f64,
}

fn line_pitch(a: [f64; 3], b: [f64; 3]) -> f64 {
    let h = (b[0] - a[0]).hypot(b[1] - a[1]);
    (b[2] - a[2]).abs().atan2(h).to_degrees()
}

/// Whether `p` lies on segment `a`-`b` in 3D, within `eps`.
fn on_segment(p: [f64; 3], a: [f64; 3], b: [f64; 3], eps: f64) -> bool {
    let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let len2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
    if len2 < 1e-18 {
        return crate::roof::dist3(p, a) <= eps;
    }
    let t = ((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1] + (p[2] - a[2]) * d[2]) / len2;
    let t = t.clamp(0.0, 1.0);
    let q = [a[0] + t * d[0], a[1] + t * d[1], a[2] + t * d[2]];
    crate::roof::dist3(p, q) <= eps
}

struct Collected {
    lines: Vec<ListLine>,
    /// Faces per part (index = part), then the hand-drawn faces.
    faces: Vec<Vec<ListFace>>,
    manual_faces: Vec<ListFace>,
    minus: BTreeMap<String, f64>,
}

fn collect(project: &Project, geos: &[RoofGeometry]) -> Collected {
    let mut counters: BTreeMap<String, usize> = BTreeMap::new();
    let mut name = |code: &str| {
        let n = counters.entry(code.to_string()).or_default();
        *n += 1;
        format!("{code}{:03}", n)
    };
    let mut lines = Vec::new();
    let mut minus: BTreeMap<String, f64> = BTreeMap::new();
    let mut faces = Vec::new();
    let mut wall_offset = 0usize;
    for (pi, geo) in geos.iter().enumerate() {
        let first = lines.len();
        for l in &geo.lines {
            let code = l.dacad_code().to_string();
            lines.push(ListLine {
                name: name(&code),
                code,
                pitch: line_pitch(l.a, l.b),
                length: l.length(),
                a: l.a,
                b: l.b,
            });
        }
        for (code, len) in &geo.minus_lines {
            *minus.entry(code.clone()).or_default() += len;
        }
        let own = &lines[first..];
        let mut part_faces = Vec::new();
        // Faces are named after their wall (`Zpart.wall`, the walls counted
        // over all parts). Dormer faces belong to no wall (`edge` is
        // `usize::MAX`): they are numbered on after the part's walls.
        let walls = project.parts.get(pi).map_or(0, |p| p.edges.len());
        let mut extra = 0usize;
        let mut face_no = |edge: usize| -> usize {
            let own = if edge < walls {
                edge + 1
            } else {
                extra += 1;
                walls.saturating_add(extra)
            };
            wall_offset.saturating_add(own)
        };
        for f in &geo.faces {
            let zs = f.polygon.iter().map(|p| p[2]);
            let (lo, hi) = zs.fold((f64::MAX, f64::MIN), |(lo, hi), z| (lo.min(z), hi.max(z)));
            let s = f.pitch_deg.to_radians().sin();
            let rafter = (f.pitch_deg > 0.01 && s > 1e-9 && hi >= lo).then(|| (hi - lo) / s);
            let n = f.polygon.len();
            let mut bounding = Vec::new();
            for i in 0..n {
                let (a, b) = (f.polygon[i], f.polygon[(i + 1) % n]);
                for (k, l) in own.iter().enumerate() {
                    let idx = first + k;
                    if !bounding.contains(&idx)
                        && on_segment(l.a, a, b, 1e-3)
                        && on_segment(l.b, a, b, 1e-3)
                    {
                        bounding.push(idx);
                    }
                }
            }
            part_faces.push(ListFace {
                name: format!("Z{}.{:02}", pi + 1, face_no(f.edge)),
                pitch: f.pitch_deg,
                area: f.area + f.deducted,
                rafter,
                material: f.material,
                lines: bounding,
                holes: f.deducted,
            });
        }
        for c in &geo.clad_faces {
            part_faces.push(ListFace {
                name: format!("Z{}.{:02}", pi + 1, face_no(c.edge)),
                pitch: 90.0,
                area: c.area,
                rafter: None,
                material: c.material,
                lines: Vec::new(),
                holes: 0.0,
            });
        }
        faces.push(part_faces);
        wall_offset = wall_offset.saturating_add(walls);
    }
    for l in project.manual_lines.iter().filter(|l| l.counted()) {
        lines.push(ListLine {
            name: name(&l.code),
            code: l.code.clone(),
            pitch: line_pitch(l.a, l.b),
            length: l.length(),
            a: l.a,
            b: l.b,
        });
    }
    let manual_faces = project
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
            lines: Vec::new(),
            holes: 0.0,
        })
        .collect();
    Collected {
        lines,
        faces,
        manual_faces,
        minus,
    }
}

/// Plan point-in-polygon (even-odd).
fn inside(x: f64, y: f64, poly: &[[f64; 3]]) -> bool {
    let mut c = false;
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + n - 1) % n]);
        if (a[1] > y) != (b[1] > y) && x < (b[0] - a[0]) * (y - a[1]) / (b[1] - a[1]) + a[0] {
            c = !c;
        }
    }
    c
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

    section(&mut out, "DÄCHER/GAUBEN");
    for i in 0..project.parts.len() {
        let _ = writeln!(out, "Dach-Nummer {}: normal", i + 1);
    }

    section(&mut out, "WAND/DACH-DEFINITIONEN");
    let mut wall = 0usize;
    for (pi, part) in project.parts.iter().enumerate() {
        for e in &part.edges {
            wall += 1;
            let def = format!("Flächendef.{}.{:02}", pi + 1, wall);
            let _ = writeln!(
                out,
                "    {def} V({})   H({})   DN({})",
                len(e.overhang),
                len(e.eave_height),
                deg(e.pitch_deg)
            );
            let knicks = e
                .half_hip
                .iter()
                .map(|h| (h.height, h.pitch_deg))
                .chain(e.breaks.iter().map(|b| (b.height, b.pitch_deg)));
            for (h, p) in knicks {
                let _ = writeln!(out, "    {def}              H({})   DN({})", len(h), deg(p));
            }
        }
    }

    // Roof windows, with the face they sit in (DACAD: Fenster<roof>.<wall>/<n>).
    if !project.windows.is_empty() {
        section(&mut out, "STREŠNÉ OKNÁ");
        let mut per_face: BTreeMap<String, usize> = BTreeMap::new();
        for (i, w) in project.windows.iter().enumerate() {
            let host = geos.iter().zip(&c.faces).find_map(|(g, fs)| {
                g.faces
                    .iter()
                    .zip(fs)
                    .find(|(f, _)| inside(w.x, w.y, &f.polygon))
                    .map(|(_, lf)| lf.name.trim_start_matches('Z').to_string())
            });
            let label = match host {
                Some(h) => {
                    let n = per_face.entry(h.clone()).or_default();
                    *n += 1;
                    format!("Fenster{h}/{n}")
                }
                None => format!("Fenster{}", i + 1),
            };
            let _ = writeln!(
                out,
                " {label}   Breite = {}   Höhe = {}   Material = {}",
                len(w.width),
                len(w.height),
                material_group(project, 0)
            );
        }
    }

    if !project.chimneys.is_empty() || !project.roof_openings.is_empty() {
        section(&mut out, "KOMÍN / OTVORY");
        for (i, ch) in project.chimneys.iter().enumerate() {
            let _ = writeln!(
                out,
                " Schornstein{}    Výška komína = {}",
                i + 1,
                len(ch.height)
            );
        }
        for i in 0..project.roof_openings.len() {
            let _ = writeln!(out, " Öffnung{}", i + 1);
        }
    }

    // Roof lines, sorted by name, a separator between line types.
    section(&mut out, "DACHLINIEN");
    let mut order: Vec<usize> = (0..c.lines.len()).collect();
    order.sort_by(|&a, &b| c.lines[a].name.cmp(&c.lines[b].name));
    let mut last: Option<&str> = None;
    for &i in &order {
        let l = &c.lines[i];
        if last.is_some_and(|code| code != l.code) {
            out.push_str(DASHES);
            out.push('\n');
        }
        last = Some(&l.code);
        let _ = writeln!(out, "{} {} {}", l.name, deg(l.pitch), len(l.length));
    }
    out.push_str(DASHES);
    out.push('\n');

    // Faces with their rafter length and bounding lines.
    section(&mut out, "STREŠNÉ PLOCHY");
    let face_line = |out: &mut String, f: &ListFace| {
        let _ = write!(out, "{} {} {}", f.name, deg(f.pitch), area(f.area));
        if let Some(r) = f.rafter {
            let _ = write!(out, " Dĺžka krokvy={}", len(r));
        }
        out.push('\n');
        for &i in &f.lines {
            let l = &c.lines[i];
            let _ = writeln!(out, "{} {} {}  ", l.name, deg(l.pitch), len(l.length));
        }
        if f.holes > 1e-9 {
            let _ = writeln!(out, "{}:1 {} {}", f.name, deg(f.pitch), area(-f.holes));
        }
    };
    for f in c.faces.iter().flatten().chain(&c.manual_faces) {
        face_line(&mut out, f);
    }
    if project.parts.len() > 1 {
        for (i, fs) in c.faces.iter().enumerate() {
            let s: f64 = fs.iter().map(|f| f.area - f.holes).sum();
            let _ = writeln!(out, "   Dach-Nummer {}: normal: S={}", i + 1, area(s));
        }
    }
    out.push_str(DASHES);
    out.push('\n');

    // Area groups per material, then the norm group where the norm differs.
    section(&mut out, "PLOCHY");
    let mut by_material: BTreeMap<u8, Vec<&ListFace>> = BTreeMap::new();
    for f in c.faces.iter().flatten().chain(&c.manual_faces) {
        if f.material > 0 {
            by_material.entry(f.material).or_default().push(f);
        }
    }
    let single = by_material.len() == 1;
    for (m, fs) in &by_material {
        let name = material_group(project, *m);
        let total: f64 = fs.iter().map(|f| f.area - f.holes).sum();
        let _ = writeln!(out, "{name} {}", area(total));
        for f in fs.iter().rev() {
            if f.holes > 1e-9 {
                let _ = writeln!(out, "    {}:1 {}", f.name, area(-f.holes));
            }
            let _ = writeln!(out, "    {} {}", f.name, area(f.area));
        }
        out.push_str(DASHES);
        out.push('\n');
        if single {
            let norm = norm_totals(project, Norm::Din).roof_area;
            if (norm - total).abs() >= 0.5 * 10f64.powi(-(d as i32)) {
                let _ = writeln!(out, "{name}-Norm {}", area(norm));
                for f in fs.iter().rev() {
                    let _ = writeln!(out, "    {} {}", f.name, area(f.area));
                }
                out.push_str(DASHES);
                out.push('\n');
            }
        }
    }

    // Line groups, sorted by group name like DACAD's group list.
    section(&mut out, "LÍNIE");
    let mut groups: BTreeMap<&'static str, Vec<usize>> = BTreeMap::new();
    for (i, l) in c.lines.iter().enumerate() {
        if l.code == "n" || l.code == "z" {
            continue;
        }
        groups.entry(line_group(&l.code)).or_default().push(i);
    }
    for (name, members) in &groups {
        let code = &c.lines[members[0]].code;
        let total: f64 = members.iter().map(|&i| c.lines[i].length).sum::<f64>()
            - c.minus.get(code).copied().unwrap_or(0.0);
        let _ = writeln!(out, "{name} {}", len(total));
        for &i in members {
            let _ = writeln!(out, "    {} {}", c.lines[i].name, len(c.lines[i].length));
        }
        out.push_str(DASHES);
        out.push('\n');
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

    #[test]
    fn the_list_has_dacads_lines_for_a_hip_roof() {
        let text = dacad_list(&dacad_hip(), &ListOptions::new("", "08.11.2026 13:02"));
        let lines: Vec<&str> = text.lines().collect();
        // Head: padded to 60 characters.
        assert_eq!(lines[0].chars().count(), 60, "{:?}", lines[0]);
        assert!(lines[0].ends_with("08.11.2026 13:02"));
        for want in [
            DASHES,
            "DÄCHER/GAUBEN",
            "Dach-Nummer 1: normal",
            "WAND/DACH-DEFINITIONEN",
            "    Flächendef.1.01 V(0,500m)   H(3,000m)   DN(30,00°)",
            "    Flächendef.1.04 V(0,500m)   H(3,000m)   DN(30,00°)",
            "DACHLINIEN",
            "F001 0,00° 2,000m",
            "G001 22,21° 6,874m",
            "T001 0,00° 11,000m",
            "STREŠNÉ PLOCHY",
            "Z1.01 30,00° 33,775m˛ Dĺžka krokvy=5,196m",
            "Z1.02 30,00° 23,383m˛ Dĺžka krokvy=5,196m",
            "T001 0,00° 11,000m  ",
            "F001 0,00° 2,000m  ",
            "PLOCHY",
            "Škridla -  114,315m˛",
            "    Z1.01 33,775m˛",
            "LÍNIE",
            "1 Hrebeň - 2,000m",
            "    F001 2,000m",
            "2 Nárožie - 27,495m",
            "6 Odkvap - 40,000m",
        ] {
            assert!(lines.contains(&want), "missing {want:?} in\n{text}");
        }
        // Sections start after two empty lines, like DACAD's "\n\n".
        let i = lines.iter().position(|l| *l == "DACHLINIEN").unwrap();
        assert_eq!(&lines[i - 3..i], &["", "", DASHES]);
        // Line groups in DACAD's order (sorted by name).
        let f = lines
            .iter()
            .position(|l| l.starts_with("1 Hrebeň"))
            .unwrap();
        let g = lines
            .iter()
            .position(|l| l.starts_with("2 Nárožie"))
            .unwrap();
        let t = lines
            .iter()
            .position(|l| l.starts_with("6 Odkvap"))
            .unwrap();
        assert!(f < g && g < t);
        assert!(text.ends_with(&format!("{DASHES}\n")));
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
        // Gable walls are listed with their 90° definition.
        assert!(text.contains("DN(90,00°)"), "{text}");
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
        assert!(text.contains("DACHLINIEN"));
        let shapes = plan_sheet(&p, &SheetInfo::default());
        assert!(!sheet_pdf(&shapes, "").is_empty());
    }
}
