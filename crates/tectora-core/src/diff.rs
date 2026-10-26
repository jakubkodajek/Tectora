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
    /// `name`, `notes`, `job.<field>`, `part`, `edge`, `vertex`,
    /// `pitch_deg`, `eave_height`, `overhang`, `half_hip`, `breaks`, `priority`.
    pub field: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub part: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge: Option<usize>,
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
        before,
        after,
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
    fn identical_projects_have_no_changes() {
        let a = examples::find("l-shape").unwrap().project();
        assert!(diff(&a, &a.clone()).is_empty());
    }
}
