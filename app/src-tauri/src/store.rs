//! Project library: one `.tectora` JSON file per design in a plain folder,
//! so the CLI, MCP server and the app can all read and write the same files.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::Serialize;
use tectora_core::Project;
use tectora_core::roof;

pub const EXTENSION: &str = "tectora";

#[derive(Debug, Serialize)]
pub struct ProjectSummary {
    pub id: String,
    pub name: String,
    pub notes: String,
    pub modified_ms: u64,
    pub roof_area: f64,
    pub parts: usize,
    /// Footprint outlines for the card thumbnail.
    pub outlines: Vec<Vec<[f64; 2]>>,
    /// Ridge, hip and valley lines in plan for the thumbnail.
    pub lines: Vec<[[f64; 2]; 2]>,
}

/// Plan view of the inner roof lines (everything except eaves).
pub fn roof_lines(project: &Project) -> Vec<[[f64; 2]; 2]> {
    project
        .parts
        .iter()
        .flat_map(|p| roof::generate(p).lines)
        .filter(|l| l.kind != roof::LineKind::Eave)
        .map(|l| [[l.a[0], l.a[1]], [l.b[0], l.b[1]]])
        .collect()
}

pub fn outlines(project: &Project) -> Vec<Vec<[f64; 2]>> {
    project
        .parts
        .iter()
        .map(|p| p.outline().iter().map(|pt| [pt.x, pt.y]).collect())
        .collect()
}

pub fn summarize(id: String, project: &Project, modified_ms: u64) -> ProjectSummary {
    ProjectSummary {
        id,
        name: project.name.clone(),
        notes: project.notes.clone(),
        modified_ms,
        roof_area: project
            .parts
            .iter()
            .map(|p| roof::generate(p).quantities().roof_area)
            .sum(),
        parts: project.parts.len(),
        outlines: outlines(project),
        lines: roof_lines(project),
    }
}

pub struct Store {
    pub dir: PathBuf,
}

fn modified_ms(path: &Path) -> u64 {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// File-name friendly id derived from a project name.
fn slug(name: &str) -> String {
    let mut s: String = name
        .to_lowercase()
        .chars()
        .map(|c| {
            let c = match c {
                'á' | 'ä' => 'a',
                'č' => 'c',
                'ď' => 'd',
                'é' | 'ě' => 'e',
                'í' => 'i',
                'ň' => 'n',
                'ó' | 'ô' => 'o',
                'ř' => 'r',
                'š' => 's',
                'ť' => 't',
                'ú' | 'ů' => 'u',
                'ý' => 'y',
                'ž' => 'z',
                c => c,
            };
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    while s.contains("--") {
        s = s.replace("--", "-");
    }
    let s = s.trim_matches('-').to_string();
    if s.is_empty() { "navrh".into() } else { s }
}

impl Store {
    pub fn new(dir: PathBuf) -> std::io::Result<Self> {
        fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    pub fn path(&self, id: &str) -> Result<PathBuf, String> {
        if id.is_empty() || id.contains(['/', '\\']) || id.starts_with('.') {
            return Err(format!("invalid project id: {id}"));
        }
        Ok(self.dir.join(format!("{id}.{EXTENSION}")))
    }

    pub fn list(&self) -> Vec<ProjectSummary> {
        let mut out = Vec::new();
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return out;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some(EXTENSION) {
                continue;
            }
            let Some(id) = path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(str::to_string)
            else {
                continue;
            };
            if let Ok(project) = self.load(&id) {
                out.push(summarize(id, &project, modified_ms(&path)));
            }
        }
        out.sort_by(|a, b| b.modified_ms.cmp(&a.modified_ms));
        out
    }

    pub fn load(&self, id: &str) -> Result<Project, String> {
        let text = fs::read_to_string(self.path(id)?).map_err(|e| e.to_string())?;
        Project::from_json(&text).map_err(|e| e.to_string())
    }

    /// Picks an unused id based on the project name.
    pub fn new_id(&self, name: &str) -> String {
        let base = slug(name);
        let mut id = base.clone();
        let mut n = 2;
        while self.dir.join(format!("{id}.{EXTENSION}")).exists() {
            id = format!("{base}-{n}");
            n += 1;
        }
        id
    }

    pub fn save(&self, id: Option<String>, project: &Project) -> Result<String, String> {
        let id = id.unwrap_or_else(|| self.new_id(&project.name));
        let path = self.path(&id)?;
        // Write atomically so a watcher never sees a half-written file.
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, project.to_json().map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
        Ok(id)
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        fs::remove_file(self.path(id)?).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_handles_czech_names() {
        assert_eq!(slug("Rodinný dům Říčany"), "rodinny-dum-ricany");
        assert_eq!(slug("  "), "navrh");
    }

    #[test]
    fn rejects_path_traversal() {
        let store = Store {
            dir: PathBuf::from("/tmp/x"),
        };
        assert!(store.path("../etc/passwd").is_err());
        assert!(store.path("ok-id").is_ok());
    }
}
