//! Project library: one `.tectora` JSON file per design in a plain folder,
//! so the CLI, MCP server and the app can all read and write the same files.
//!
//! Every save made through [`Store::save_as`] is also recorded in an
//! activity log (`.activity/<id>.jsonl`) together with the previous version,
//! so the app can show who changed what and revert it. AI agents publish
//! what they are doing right now in `.activity/live.json`.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::diff::{Change, diff};
use crate::model::{JobInfo, Project};
use crate::roof;

pub const EXTENSION: &str = "tectora";
pub const ACTIVITY_DIR: &str = ".activity";
pub const LIVE_FILE: &str = "live.json";
/// Files attached to a job (drawings, photos), in `.files/<id>/`.
pub const FILES_DIR: &str = ".files";
/// The actor name used for edits made in the desktop app.
pub const USER_ACTOR: &str = "user";
/// Activity entries kept per project (each holds a full previous version).
const ACTIVITY_KEEP: usize = 50;

/// A file attached to a job.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Attachment {
    pub name: String,
    /// Bytes.
    pub size: u64,
    pub modified_ms: u64,
}

#[derive(Debug, Serialize)]
pub struct ProjectSummary {
    pub id: String,
    pub name: String,
    pub notes: String,
    pub job: JobInfo,
    pub modified_ms: u64,
    pub roof_area: f64,
    pub parts: usize,
    /// Footprint outlines for the card thumbnail.
    pub outlines: Vec<Vec<[f64; 2]>>,
    /// Ridge, hip and valley lines in plan for the thumbnail.
    pub lines: Vec<[[f64; 2]; 2]>,
    /// Latest recorded change, if any.
    pub last_activity: Option<ActivityInfo>,
}

/// An activity entry without the stored previous version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityInfo {
    pub ts_ms: u64,
    pub actor: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityEntry {
    pub ts_ms: u64,
    /// `user` for the desktop app, otherwise the AI client's name.
    pub actor: String,
    /// One sentence on what was done and why.
    pub summary: String,
    /// The project before this change (`None` when it was created).
    pub before: Option<Project>,
    /// What this change did, computed when it was saved.
    #[serde(default)]
    pub changes: Vec<Change>,
}

impl ActivityEntry {
    pub fn info(&self) -> ActivityInfo {
        ActivityInfo {
            ts_ms: self.ts_ms,
            actor: self.actor.clone(),
            summary: self.summary.clone(),
        }
    }
}

/// What an AI agent is doing right now, shown live in the app.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveStatus {
    pub ts_ms: u64,
    pub actor: String,
    /// Short description, e.g. "Reading project rodinny-dum".
    pub message: String,
    pub project_id: Option<String>,
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// `TECTORA_PROJECTS_DIR`, otherwise `Documents/Tectora`.
pub fn default_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("TECTORA_PROJECTS_DIR") {
        return PathBuf::from(dir);
    }
    dirs::document_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Tectora")
}

/// Plan view of the inner roof lines (everything except eaves).
pub fn roof_lines(project: &Project) -> Vec<[[f64; 2]; 2]> {
    crate::openings::generate_project(project)
        .into_iter()
        .flat_map(|g| g.lines)
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

pub fn roof_area(project: &Project) -> f64 {
    crate::quantities::roof_totals(project).roof_area
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

/// Writes via a temporary file so a watcher never sees a half-written file.
/// The temporary name is unique per process and call, so the app and an MCP
/// server saving the same design at once cannot write into each other's file.
fn write_atomic(path: &Path, contents: &str) -> Result<(), String> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp = path.with_extension(format!("{}-{n}.tmp", std::process::id()));
    fs::write(&tmp, contents).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        e.to_string()
    })
}

/// Longest accepted project id (file names stay well inside the 255-byte
/// limit of common file systems).
const MAX_ID_LEN: usize = 120;

/// Names Windows reserves for devices in every folder (`CON.tectora` opens
/// the console, not a file).
const WINDOWS_DEVICE_NAMES: &[&str] = &[
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// A project id is a plain file name inside the library folder on both
/// macOS and Windows: no separators, drive or stream prefix (`C:x`,
/// `x:stream`), characters Windows forbids, hidden names or device names.
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_ID_LEN
        && !id.starts_with('.')
        && !id.ends_with(['.', ' '])
        && !id.chars().any(|c| {
            c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
        })
        && !WINDOWS_DEVICE_NAMES.contains(&id.to_ascii_lowercase().as_str())
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
                'ĺ' | 'ľ' => 'l',
                'ň' => 'n',
                'ó' | 'ô' | 'ö' => 'o',
                'ŕ' | 'ř' => 'r',
                'š' => 's',
                'ß' => 's',
                'ť' => 't',
                'ú' | 'ů' | 'ü' => 'u',
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

    pub fn open_default() -> std::io::Result<Self> {
        Self::new(default_dir())
    }

    pub fn path(&self, id: &str) -> Result<PathBuf, String> {
        if !valid_id(id) {
            return Err(format!("invalid project id: {id}"));
        }
        Ok(self.dir.join(format!("{id}.{EXTENSION}")))
    }

    pub fn activity_dir(&self) -> PathBuf {
        self.dir.join(ACTIVITY_DIR)
    }

    pub(crate) fn activity_path(&self, id: &str) -> Result<PathBuf, String> {
        self.path(id)?;
        Ok(self.activity_dir().join(format!("{id}.jsonl")))
    }

    pub fn exists(&self, id: &str) -> bool {
        self.path(id).is_ok_and(|p| p.exists())
    }

    pub fn summarize(&self, id: &str) -> Result<ProjectSummary, String> {
        let project = self.load(id)?;
        // A broken design still shows up in the library (so it can be opened
        // and fixed or deleted), just without its roof preview.
        let (area, lines) = project
            .check_limits()
            .and_then(|()| crate::model::guarded(|| (roof_area(&project), roof_lines(&project))))
            .unwrap_or_default();
        Ok(ProjectSummary {
            id: id.to_string(),
            name: project.name.clone(),
            notes: project.notes.clone(),
            job: project.job.clone(),
            modified_ms: modified_ms(&self.path(id)?),
            roof_area: area,
            parts: project.parts.len(),
            outlines: outlines(&project),
            lines,
            last_activity: self.activity(id).last().map(ActivityEntry::info),
        })
    }

    pub fn list(&self) -> Vec<ProjectSummary> {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut out: Vec<ProjectSummary> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some(EXTENSION))
            .filter_map(|p| p.file_stem().and_then(|s| s.to_str()).map(str::to_string))
            .filter_map(|id| self.summarize(&id).ok())
            .collect();
        out.sort_by(|a, b| b.modified_ms.cmp(&a.modified_ms));
        out
    }

    pub fn load(&self, id: &str) -> Result<Project, String> {
        let text = fs::read_to_string(self.path(id)?).map_err(|e| format!("{id}: {e}"))?;
        Project::from_json(&text).map_err(|e| e.to_string())
    }

    /// Picks an unused id based on the project name.
    pub fn new_id(&self, name: &str) -> String {
        let mut base = slug(name);
        // Leave room for the "-N" suffix and keep clear of device names.
        base.truncate(MAX_ID_LEN - 8);
        if !valid_id(&base) {
            base.push_str("-navrh");
        }
        let mut id = base.clone();
        let mut n = 2;
        while self.dir.join(format!("{id}.{EXTENSION}")).exists() {
            id = format!("{base}-{n}");
            n += 1;
        }
        id
    }

    /// Saves without recording activity (app autosave of its own edits).
    pub fn save(&self, id: Option<String>, project: &Project) -> Result<String, String> {
        let id = id.unwrap_or_else(|| self.new_id(&project.name));
        write_atomic(
            &self.path(&id)?,
            &project.to_json().map_err(|e| e.to_string())?,
        )?;
        Ok(id)
    }

    /// Saves and records who made the change and why, keeping the previous
    /// version so it can be reviewed and reverted.
    pub fn save_as(
        &self,
        id: Option<String>,
        project: &Project,
        actor: &str,
        summary: &str,
    ) -> Result<String, String> {
        let before = id.as_deref().and_then(|id| self.load(id).ok());
        if before.as_ref() == Some(project) {
            return Ok(id.unwrap_or_default());
        }
        let id = id.unwrap_or_else(|| self.new_id(&project.name));
        // Log first: the app reacts to the project file and then reads the log.
        self.append_activity(
            &id,
            ActivityEntry {
                ts_ms: now_ms(),
                actor: actor.to_string(),
                summary: summary.to_string(),
                changes: before
                    .as_ref()
                    .map(|b| diff(b, project))
                    .unwrap_or_default(),
                before,
            },
        )?;
        self.save(Some(id.clone()), project)
    }

    fn append_activity(&self, id: &str, entry: ActivityEntry) -> Result<(), String> {
        fs::create_dir_all(self.activity_dir()).map_err(|e| e.to_string())?;
        let path = self.activity_path(id)?;
        let mut entries = self.activity(id);
        let line = serde_json::to_string(&entry).map_err(|e| e.to_string())?;
        if entries.len() >= ACTIVITY_KEEP {
            entries.push(entry);
            let keep = &entries[entries.len() - ACTIVITY_KEEP..];
            let text: Vec<String> = keep
                .iter()
                .filter_map(|e| serde_json::to_string(e).ok())
                .collect();
            return write_atomic(&path, &(text.join("\n") + "\n"));
        }
        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| e.to_string())?;
        writeln!(f, "{line}").map_err(|e| e.to_string())
    }

    /// Recorded changes of a project, oldest first.
    pub fn activity(&self, id: &str) -> Vec<ActivityEntry> {
        let Ok(path) = self.activity_path(id) else {
            return Vec::new();
        };
        fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect()
    }

    /// Restores the version before the latest recorded change.
    pub fn revert_last(&self, id: &str, actor: &str) -> Result<Project, String> {
        let last = self
            .activity(id)
            .pop()
            .ok_or_else(|| format!("{id}: no recorded change to revert"))?;
        let before = last
            .before
            .ok_or_else(|| format!("{id}: the latest change created the project"))?;
        self.save_as(
            Some(id.to_string()),
            &before,
            actor,
            &format!("Reverted: {}", last.summary),
        )?;
        Ok(before)
    }

    /// Restores the version from before the change recorded at `ts_ms`,
    /// undoing it and every later change in one step.
    pub fn revert_to(&self, id: &str, ts_ms: u64, actor: &str) -> Result<Project, String> {
        let log = self.activity(id);
        let pos = log
            .iter()
            .rposition(|e| e.ts_ms == ts_ms)
            .ok_or_else(|| format!("{id}: no recorded change at {ts_ms}"))?;
        let before = log[pos]
            .before
            .clone()
            .ok_or_else(|| format!("{id}: that change created the project"))?;
        let undone = log.len() - pos;
        let summary = if undone == 1 {
            format!("Reverted: {}", log[pos].summary)
        } else {
            format!("Reverted {undone} changes since: {}", log[pos].summary)
        };
        self.save_as(Some(id.to_string()), &before, actor, &summary)?;
        Ok(before)
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        fs::remove_file(self.path(id)?).map_err(|e| e.to_string())?;
        let _ = fs::remove_file(self.activity_path(id)?);
        let _ = fs::remove_dir_all(self.files_dir(id)?);
        Ok(())
    }

    /// Folder of the files attached to job `id`.
    pub fn files_dir(&self, id: &str) -> Result<PathBuf, String> {
        self.path(id)?;
        Ok(self.dir.join(FILES_DIR).join(id))
    }

    /// Path of attachment `name` of job `id`.
    pub fn attachment_path(&self, id: &str, name: &str) -> Result<PathBuf, String> {
        if name.is_empty() || name.contains(['/', '\\']) || name.starts_with('.') {
            return Err(format!("invalid file name: {name}"));
        }
        Ok(self.files_dir(id)?.join(name))
    }

    /// The files attached to job `id`, by name.
    pub fn attachments(&self, id: &str) -> Vec<Attachment> {
        let Ok(dir) = self.files_dir(id) else {
            return Vec::new();
        };
        let mut out: Vec<Attachment> = fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
            .filter_map(|e| {
                let name = e.file_name().into_string().ok()?;
                (!name.starts_with('.')).then(|| Attachment {
                    size: e.metadata().map_or(0, |m| m.len()),
                    modified_ms: modified_ms(&e.path()),
                    name,
                })
            })
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    /// Stores `bytes` as an attachment of job `id` under `name`, or under
    /// `name (2)` etc. when that name is taken. Returns the name used.
    pub fn add_attachment(&self, id: &str, name: &str, bytes: &[u8]) -> Result<String, String> {
        if !self.exists(id) {
            return Err(format!("no project with id {id}"));
        }
        let name = name.trim();
        self.attachment_path(id, name)?;
        let (stem, ext) = match name.rfind('.') {
            Some(i) if i > 0 => (&name[..i], &name[i..]),
            _ => (name, ""),
        };
        let mut used = name.to_string();
        let mut n = 2;
        while self.attachment_path(id, &used)?.exists() {
            used = format!("{stem} ({n}){ext}");
            n += 1;
        }
        let path = self.attachment_path(id, &used)?;
        fs::create_dir_all(path.parent().expect("has a parent")).map_err(|e| e.to_string())?;
        fs::write(&path, bytes).map_err(|e| e.to_string())?;
        Ok(used)
    }

    /// Copies a file from disk into job `id`'s attachments.
    pub fn attach_file(&self, id: &str, src: &Path) -> Result<String, String> {
        let bytes = fs::read(src).map_err(|e| format!("{}: {e}", src.display()))?;
        let name = src
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| format!("{}: not a file", src.display()))?;
        self.add_attachment(id, name, &bytes)
    }

    pub fn remove_attachment(&self, id: &str, name: &str) -> Result<(), String> {
        fs::remove_file(self.attachment_path(id, name)?).map_err(|e| e.to_string())
    }

    /// Copies all attachments of job `from` to job `to`.
    pub fn copy_attachments(&self, from: &str, to: &str) -> Result<(), String> {
        for a in self.attachments(from) {
            let bytes =
                fs::read(self.attachment_path(from, &a.name)?).map_err(|e| e.to_string())?;
            self.add_attachment(to, &a.name, &bytes)?;
        }
        Ok(())
    }

    pub fn set_live(&self, status: &LiveStatus) {
        if fs::create_dir_all(self.activity_dir()).is_ok() {
            if let Ok(text) = serde_json::to_string(status) {
                let _ = write_atomic(&self.activity_dir().join(LIVE_FILE), &text);
            }
        }
    }

    pub fn live(&self) -> Option<LiveStatus> {
        let text = fs::read_to_string(self.activity_dir().join(LIVE_FILE)).ok()?;
        serde_json::from_str(&text).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::examples;

    fn temp_store(name: &str) -> Store {
        let dir = std::env::temp_dir().join(format!("tectora-test-{name}-{}", now_ms()));
        Store::new(dir).unwrap()
    }

    #[test]
    fn slug_handles_czech_names() {
        assert_eq!(slug("Rodinný dům Říčany"), "rodinny-dum-ricany");
        assert_eq!(slug("  "), "navrh");
        let store = Store {
            dir: PathBuf::from("/nonexistent-tectora-dir"),
        };
        assert_eq!(store.new_id("Con"), "con-navrh");
        assert!(
            store
                .path(&store.new_id(&"dlouhy nazev ".repeat(40)))
                .is_ok()
        );
    }

    #[test]
    fn rejects_path_traversal() {
        let store = Store {
            dir: PathBuf::from("/tmp/x"),
        };
        assert!(store.path("../etc/passwd").is_err());
        assert!(store.path("ok-id").is_ok());
        assert!(store.path("rodinny-dum-2").is_ok());
        assert!(store.path("Dům Říčany").is_ok());
        for bad in [
            "",
            ".hidden",
            "a/b",
            "a\\b",
            "C:evil",
            "x:stream",
            "CON",
            "nul",
            "com1",
            "trailing.",
            "trailing ",
            "a\nb",
            "q?",
            "a*b",
        ] {
            assert!(store.path(bad).is_err(), "{bad:?} must be rejected");
        }
        assert!(store.path(&"x".repeat(MAX_ID_LEN + 1)).is_err());
    }

    #[test]
    fn records_activity_and_reverts() {
        let store = temp_store("activity");
        let a = examples::find("hip").unwrap().project();
        let id = store
            .save_as(None, &a, "claude", "Created hip roof")
            .unwrap();
        let mut b = a.clone();
        b.parts[0].edges[0].pitch_deg = 45.0;
        store
            .save_as(Some(id.clone()), &b, "claude", "Steeper front")
            .unwrap();

        let log = store.activity(&id);
        assert_eq!(log.len(), 2);
        assert!(log[0].before.is_none());
        assert_eq!(log[1].before.as_ref(), Some(&a));
        assert_eq!(
            store.summarize(&id).unwrap().last_activity.unwrap().summary,
            "Steeper front"
        );

        assert_eq!(store.revert_last(&id, USER_ACTOR).unwrap(), a);
        assert_eq!(store.load(&id).unwrap(), a);

        // Two agent steps undone together.
        store
            .save_as(Some(id.clone()), &b, "claude", "Step 1")
            .unwrap();
        let first = store.activity(&id).last().unwrap().ts_ms;
        let mut c = b.clone();
        c.parts[0].edges[1].overhang = 0.9;
        std::thread::sleep(std::time::Duration::from_millis(2));
        store
            .save_as(Some(id.clone()), &c, "claude", "Step 2")
            .unwrap();
        assert_eq!(store.revert_to(&id, first, USER_ACTOR).unwrap(), a);
        assert_eq!(store.load(&id).unwrap(), a);
        store.delete(&id).unwrap();
        assert!(store.activity(&id).is_empty());
        let _ = fs::remove_dir_all(&store.dir);
    }
}
