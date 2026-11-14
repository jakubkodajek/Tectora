//! What changed between two versions of a project, so the app can show the
//! user exactly what an AI agent (or any other tool) did to a design.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::model::{EdgeDef, Project, RoofPart};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Added,
    Removed,
    Modified,
}

/// One change. `part`/`edge` index into the *new* project, except for
/// removed items where they index into the old one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Change {
    pub kind: ChangeKind,
    /// Project: `name`, `notes`, `job.<field>`, `flat_roof`,
    /// `material_names`, `result_snapshot`. Parts and walls: `part`,
    /// `part_name`, `edge`, `vertex`, `pitch_deg`, `eave_height`,
    /// `overhang`, `half_hip`, `breaks`, `priority`, `clad`, `direction`,
    /// `material`. Items (with `item`): `dormer`, `chimney`, `window`,
    /// `roof_opening`, `manual_line`, `manual_face`.
    ///
    /// Logs written before item changes existed hold one `dormers`,
    /// `chimneys`, `windows` or `roof_openings` change with the whole list.
    pub field: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub part: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge: Option<usize>,
    /// Index of the dormer, chimney, window, opening or hand-drawn line or
    /// face, into the new list (the old one for removed items).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<usize>,
    pub before: Value,
    pub after: Value,
}

const EPS: f64 = 1e-6;

fn change(
    kind: ChangeKind,
    field: &str,
    part: Option<usize>,
    edge: Option<usize>,
    before: Value,
    after: Value,
) -> Change {
    Change {
        kind,
        field: field.to_string(),
        part,
        edge,
        item: None,
        before,
        after,
    }
}

/// Changes between two lists of items (dormers, chimneys, …). Items that
/// are equal in both lists are paired first, so deleting the first of three
/// windows reads as one removed window, not two modified and one removed;
/// the rest are paired in order as modified, and the surplus is added or
/// removed.
fn diff_items<T: PartialEq + Serialize>(field: &str, a: &[T], b: &[T], out: &mut Vec<Change>) {
    if a == b {
        return;
    }
    let mut used_a = vec![false; a.len()];
    let mut used_b = vec![false; b.len()];
    for (j, y) in b.iter().enumerate() {
        if let Some(i) = (0..a.len()).find(|&i| !used_a[i] && a[i] == *y) {
            used_a[i] = true;
            used_b[j] = true;
        }
    }
    let rest_a: Vec<usize> = (0..a.len()).filter(|&i| !used_a[i]).collect();
    let rest_b: Vec<usize> = (0..b.len()).filter(|&j| !used_b[j]).collect();
    let item = |kind, i: usize, before: Value, after: Value| Change {
        item: Some(i),
        ..change(kind, field, None, None, before, after)
    };
    for (k, &j) in rest_b.iter().enumerate() {
        out.push(match rest_a.get(k) {
            Some(&i) => item(ChangeKind::Modified, j, json!(a[i]), json!(b[j])),
            None => item(ChangeKind::Added, j, Value::Null, json!(b[j])),
        });
    }
    for &i in rest_a.iter().skip(rest_b.len()) {
        out.push(item(ChangeKind::Removed, i, json!(a[i]), Value::Null));
    }
}

fn same(a: f64, b: f64) -> bool {
    (a - b).abs() < EPS
}

fn same_point(a: &EdgeDef, b: &EdgeDef) -> bool {
    same(a.start.x, b.start.x) && same(a.start.y, b.start.y)
}

fn edge_fields(pi: usize, ei: usize, a: &EdgeDef, b: &EdgeDef, out: &mut Vec<Change>) {
    let m = |field: &str, before: Value, after: Value| {
        change(
            ChangeKind::Modified,
            field,
            Some(pi),
            Some(ei),
            before,
            after,
        )
    };
    if !same_point(a, b) {
        out.push(m(
            "vertex",
            json!([a.start.x, a.start.y]),
            json!([b.start.x, b.start.y]),
        ));
    }
    if !same(a.pitch_deg, b.pitch_deg) {
        out.push(m("pitch_deg", json!(a.pitch_deg), json!(b.pitch_deg)));
    }
    if !same(a.eave_height, b.eave_height) {
        out.push(m("eave_height", json!(a.eave_height), json!(b.eave_height)));
    }
    if !same(a.overhang, b.overhang) {
        out.push(m("overhang", json!(a.overhang), json!(b.overhang)));
    }
    if a.half_hip != b.half_hip {
        out.push(m("half_hip", json!(a.half_hip), json!(b.half_hip)));
    }
    if a.priority != b.priority {
        out.push(m("priority", json!(a.priority), json!(b.priority)));
    }
    if a.breaks != b.breaks {
        out.push(m("breaks", json!(a.breaks), json!(b.breaks)));
    }
    if a.clad != b.clad {
        out.push(m("clad", json!(a.clad), json!(b.clad)));
    }
    if a.material != b.material {
        out.push(m("material", json!(a.material), json!(b.material)));
    }
    if a.direction_rad != b.direction_rad {
        out.push(m(
            "direction",
            json!(a.direction_rad),
            json!(b.direction_rad),
        ));
    }
}

fn diff_part(pi: usize, a: &RoofPart, b: &RoofPart, out: &mut Vec<Change>) {
    if a.name != b.name {
        out.push(change(
            ChangeKind::Modified,
            "part_name",
            Some(pi),
            None,
            json!(a.name),
            json!(b.name),
        ));
    }
    // The building height as the editor shows it: storing the height a
    // building already showed (`freeze_height`) is no change (review
    // TEST-010).
    let (ha, hb) = (a.building_height(), b.building_height());
    // Without a stored height on either side it follows the eaves, whose
    // changes are listed already.
    if (a.height.is_some() || b.height.is_some()) && ha != hb {
        out.push(change(
            ChangeKind::Modified,
            "building_height",
            Some(pi),
            None,
            json!(ha),
            json!(hb),
        ));
    }
    if a.edges.len() == b.edges.len() {
        for (i, (ea, eb)) in a.edges.iter().zip(&b.edges).enumerate() {
            edge_fields(pi, i, ea, eb, out);
        }
        return;
    }
    // Different vertex count: pair edges by their start point; the rest
    // were inserted or deleted.
    let mut used = vec![false; a.edges.len()];
    for (j, eb) in b.edges.iter().enumerate() {
        match a
            .edges
            .iter()
            .enumerate()
            .position(|(i, ea)| !used[i] && same_point(ea, eb))
        {
            Some(i) => {
                used[i] = true;
                edge_fields(pi, j, &a.edges[i], eb, out);
            }
            None => out.push(change(
                ChangeKind::Added,
                "edge",
                Some(pi),
                Some(j),
                Value::Null,
                json!([eb.start.x, eb.start.y]),
            )),
        }
    }
    for (i, ea) in a.edges.iter().enumerate() {
        if !used[i] {
            out.push(change(
                ChangeKind::Removed,
                "edge",
                Some(pi),
                Some(i),
                json!([ea.start.x, ea.start.y]),
                Value::Null,
            ));
        }
    }
}

pub fn diff(before: &Project, after: &Project) -> Vec<Change> {
    let mut out = Vec::new();
    let m = |field: &str, a: Value, b: Value| change(ChangeKind::Modified, field, None, None, a, b);
    if before.name != after.name {
        out.push(m("name", json!(before.name), json!(after.name)));
    }
    if before.notes != after.notes {
        out.push(m("notes", json!(before.notes), json!(after.notes)));
    }
    let (ja, jb) = (&before.job, &after.job);
    for (field, a, b) in [
        ("job.number", &ja.number, &jb.number),
        ("job.customer", &ja.customer, &jb.customer),
        ("job.address", &ja.address, &jb.address),
    ] {
        if a != b {
            out.push(m(field, json!(a), json!(b)));
        }
    }
    if ja.status != jb.status {
        out.push(m("job.status", json!(ja.status), json!(jb.status)));
    }
    if before.flat_roof != after.flat_roof {
        out.push(m(
            "flat_roof",
            json!(before.flat_roof),
            json!(after.flat_roof),
        ));
    }
    if before.material_names != after.material_names {
        out.push(m(
            "material_names",
            json!(before.material_names),
            json!(after.material_names),
        ));
    }
    match (&before.result_snapshot, &after.result_snapshot) {
        (None, None) => {}
        (Some(a), Some(b)) if a == b => {}
        (a, b) => {
            let kind = match (a, b) {
                (None, _) => ChangeKind::Added,
                (_, None) => ChangeKind::Removed,
                _ => ChangeKind::Modified,
            };
            // The figures themselves are in the project; the log only says
            // that the stored result was taken, replaced or dropped.
            out.push(change(
                kind,
                "result_snapshot",
                None,
                None,
                Value::Null,
                Value::Null,
            ));
        }
    }
    diff_items("dormer", &before.dormers, &after.dormers, &mut out);
    diff_items("chimney", &before.chimneys, &after.chimneys, &mut out);
    diff_items("window", &before.windows, &after.windows, &mut out);
    diff_items(
        "roof_opening",
        &before.roof_openings,
        &after.roof_openings,
        &mut out,
    );
    diff_items(
        "manual_line",
        &before.manual_lines,
        &after.manual_lines,
        &mut out,
    );
    diff_items(
        "manual_face",
        &before.manual_faces,
        &after.manual_faces,
        &mut out,
    );
    let n = before.parts.len().min(after.parts.len());
    for i in 0..n {
        diff_part(i, &before.parts[i], &after.parts[i], &mut out);
    }
    for (i, p) in after.parts.iter().enumerate().skip(n) {
        out.push(change(
            ChangeKind::Added,
            "part",
            Some(i),
            None,
            Value::Null,
            json!(p.name),
        ));
    }
    for (i, p) in before.parts.iter().enumerate().skip(n) {
        out.push(change(
            ChangeKind::Removed,
            "part",
            Some(i),
            None,
            json!(p.name),
            Value::Null,
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::examples;
    use crate::geom::Point2;
    use crate::model::{Chimney, RoofWindow};

    #[test]
    fn detects_pitch_and_vertex_changes() {
        let a = examples::find("hip").unwrap().project();
        let mut b = a.clone();
        b.parts[0].edges[1].pitch_deg = 45.0;
        b.parts[0].edges[2].start.x += 1.0;
        let d = diff(&a, &b);
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].field, "pitch_deg");
        assert_eq!(d[0].edge, Some(1));
        assert_eq!(d[1].field, "vertex");
        assert_eq!(d[1].edge, Some(2));
    }

    #[test]
    fn detects_inserted_vertex() {
        let a = examples::find("hip").unwrap().project();
        let mut b = a.clone();
        let mut e = b.parts[0].edges[0].clone();
        e.start.x = 6.0;
        b.parts[0].edges.insert(1, e);
        let d = diff(&a, &b);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].kind, ChangeKind::Added);
        assert_eq!(d[0].edge, Some(1));
    }

    #[test]
    fn removing_the_first_window_is_one_removed_item() {
        let mut a = examples::find("hip").unwrap().project();
        let w = |x: f64| RoofWindow {
            x,
            y: 1.0,
            width: 0.8,
            height: 1.2,
        };
        a.windows = vec![w(1.0), w(3.0), w(5.0)];
        let mut b = a.clone();
        b.windows.remove(0);
        let d = diff(&a, &b);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].field, "window");
        assert_eq!(d[0].kind, ChangeKind::Removed);
        assert_eq!(d[0].item, Some(0));
    }

    #[test]
    fn moved_chimney_is_modified_and_new_one_added() {
        let mut a = examples::find("hip").unwrap().project();
        let c = |x: f64| Chimney {
            corners: vec![
                Point2 { x, y: 2.0 },
                Point2 { x: x + 0.5, y: 2.0 },
                Point2 { x: x + 0.5, y: 2.5 },
                Point2 { x, y: 2.5 },
            ],
            height: 9.0,
        };
        a.chimneys = vec![c(2.0)];
        let mut b = a.clone();
        b.chimneys[0] = c(3.0);
        b.chimneys.push(c(6.0));
        let d = diff(&a, &b);
        assert_eq!(d.len(), 2);
        assert_eq!(
            (d[0].kind.clone(), d[0].item),
            (ChangeKind::Modified, Some(0))
        );
        assert_eq!((d[1].kind.clone(), d[1].item), (ChangeKind::Added, Some(1)));
        assert_eq!(d[0].field, "chimney");
    }

    #[test]
    fn detects_project_options_and_wall_material() {
        let a = examples::find("hip").unwrap().project();
        let mut b = a.clone();
        b.flat_roof = true;
        b.material_names = vec!["Bramac".into()];
        b.parts[0].edges[0].material = 2;
        b.result_snapshot = Some(crate::snapshot::take(&a));
        let fields: Vec<String> = diff(&a, &b).into_iter().map(|c| c.field).collect();
        assert_eq!(
            fields,
            ["flat_roof", "material_names", "result_snapshot", "material"]
        );
    }

    #[test]
    fn old_log_entries_without_item_still_parse() {
        let c: Change =
            serde_json::from_str(r#"{"kind":"modified","field":"dormers","before":[],"after":[]}"#)
                .unwrap();
        assert_eq!(c.item, None);
    }

    #[test]
    fn identical_projects_have_no_changes() {
        let a = examples::find("l-shape").unwrap().project();
        assert!(diff(&a, &a.clone()).is_empty());
    }

    #[test]
    fn lists_a_building_height_change_but_not_storing_the_height_shown() {
        let a = examples::find("hip").unwrap().project();
        let mut frozen = a.clone();
        frozen.parts[0].freeze_height();
        assert!(diff(&a, &frozen).is_empty());
        let mut b = frozen.clone();
        b.parts[0].height = Some(b.parts[0].building_height().unwrap() + 1.0);
        let d = diff(&frozen, &b);
        assert_eq!(d.len(), 1, "{d:?}");
        assert_eq!(d[0].field, "building_height");
        assert_eq!(d[0].part, Some(0));
    }
}
