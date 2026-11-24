//! Job bundles (`.tectorajob`): one ZIP file holding a whole job to hand
//! over or archive — the project, its attached files (drawings, photos),
//! its change history and a plan preview.
//!
//! ```text
//! manifest.json     format, versions, export time, file list
//! project.tectora   the project (same JSON as in the library)
//! history.jsonl     the change log, if any
//! preview.svg       plan view: outlines and roof lines
//! files/<name>      attachments
//! ```

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use zip::write::SimpleFileOptions;

use crate::model::Project;
use crate::store::{FileError, Store, now_ms, outlines, roof_lines};

pub const EXTENSION: &str = "tectorajob";
const FORMAT: &str = "tectora-job";
const FORMAT_VERSION: u32 = 1;
/// Largest attached file read from a bundle, bytes.
const MAX_ENTRY: u64 = crate::store::MAX_ATTACHMENT;
/// Largest project or change log in a bundle, bytes.
const MAX_TEXT_ENTRY: u64 = 16 * 1024 * 1024;
/// Most attached files a bundle may carry.
const MAX_FILES: usize = 200;
/// Largest total size of the attached files of one bundle, bytes. Together
/// with the limits above this keeps a small ZIP from filling the disk
/// (an entry can inflate about 1000:1).
const MAX_FILES_TOTAL: u64 = 512 * 1024 * 1024;
/// Largest bundle file opened: the attached files, the project, its log
/// and the rest of the ZIP at their limits.
const MAX_BUNDLE: u64 = MAX_FILES_TOTAL + 4 * MAX_TEXT_ENTRY;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub format: String,
    pub format_version: u32,
    pub name: String,
    pub engine_version: String,
    pub exported_ms: u64,
    pub files: Vec<String>,
}

/// A bundle that is damaged or not a job (`unreadable`).
fn zip_err(e: impl std::fmt::Display) -> FileError {
    FileError::new("unreadable", format!("job bundle: {e}"))
}

fn too_large(what: impl std::fmt::Display) -> FileError {
    FileError::new("too_large", format!("job bundle: {what}"))
}

/// A project file's text as a project: `newer_format` when a newer Tectora
/// wrote it, `unreadable` when it is not a project, `out_of_range` or
/// `too_many` when it holds what no real roof has (ARCH-002). Its texts are
/// cleaned first (`fallback` names a job without a visible name), so a
/// text that can be cleaned does not refuse the job (ARCH-201, ARCH-303).
fn parse_project(text: &str, what: &str, fallback: &str) -> Result<Project, FileError> {
    if let Some(version) = crate::store::newer_format(text) {
        return Err(FileError::new(
            "newer_format",
            format!(
                "{what}: saved by a newer Tectora (format {version}, this version reads up to {})",
                crate::model::SCHEMA_VERSION
            ),
        ));
    }
    let mut project = Project::from_json(text)
        .map_err(|e| FileError::new("unreadable", format!("{what}: {e}")))?;
    crate::edit::clean_received(&mut project, fallback);
    project
        .check_input()
        .map_err(|e| FileError::new(e.code, format!("{what}: {e}")))?;
    Ok(project)
}

/// Folder of the library where the MCP server writes bundles.
pub const EXPORTS_DIR: &str = "exports";

/// A new file `<library>/exports/<name>.tectorajob` (with " (2)" etc. when
/// the name is taken): the only place an AI client may write a bundle, so it
/// can never replace a file of the user's. `name` must be a plain file name.
pub fn export_path(store: &Store, name: &str) -> Result<std::path::PathBuf, FileError> {
    export_path_with(store, name, EXTENSION)
}

/// Like [`export_path`] for another file type in the exports folder, e.g.
/// the plan sheet as `pdf` or `svg`.
pub fn export_path_with(
    store: &Store,
    name: &str,
    ext: &str,
) -> Result<std::path::PathBuf, FileError> {
    // Trailing spaces and dots, and the extension typed with them, are no
    // part of the name, as `with_extension` takes them for the CLI:
    // Windows drops them, so "a." would name another file there than on
    // macOS (HUNT-204 item 1).
    let mut name = name.trim().trim_end_matches(blank_or_dot);
    let tail = format!(".{ext}");
    if name.len() >= tail.len()
        && name.is_char_boundary(name.len() - tail.len())
        && name[name.len() - tail.len()..].eq_ignore_ascii_case(&tail)
    {
        name = name[..name.len() - tail.len()].trim_end_matches(blank_or_dot);
    }
    if name.is_empty() {
        return Err(FileError::new(
            "invalid_name",
            "the file name is empty (only spaces and dots)",
        ));
    }
    let bad = name.len() > 100 || !crate::store::valid_file_name(&format!("{name}.{ext}"));
    if bad {
        return Err(FileError::new(
            "invalid_name",
            format!("`{name}` is not a plain file name"),
        ));
    }
    let dir = store.dir.join(EXPORTS_DIR);
    fs::create_dir_all(&store.dir).map_err(|e| FileError::io(&store.dir, e))?;
    // A link in place of the folder is refused: nothing is exported to
    // where it points (audit 2026-11-22 ARCH-603).
    crate::store::own_folder(&dir).map_err(|e| FileError::io(&dir, e))?;
    for n in 1..1000 {
        let file = if n == 1 {
            format!("{name}.{ext}")
        } else {
            format!("{name} ({n}).{ext}")
        };
        let path = dir.join(file);
        // Anything there takes the name, also a link to nowhere, which
        // `exists` calls free (ARCH-603).
        if fs::symlink_metadata(&path).is_err() {
            return Ok(path);
        }
    }
    Err(FileError::new(
        "invalid_name",
        format!("too many exports named `{name}`"),
    ))
}

/// Writes `bytes` as a new file `<name>.<ext>` in the library's exports
/// folder ([`export_path_with`]) and returns its path. The file is created
/// only where nothing is (`create_new`), so a link put at the name between
/// the pick and the write is never written through (ARCH-603).
pub fn write_export(
    store: &Store,
    name: &str,
    ext: &str,
    bytes: &[u8],
) -> Result<std::path::PathBuf, FileError> {
    for _ in 0..8 {
        let path = export_path_with(store, name, ext)?;
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut f) => {
                let written = f.write_all(bytes).and_then(|()| f.sync_all());
                if let Err(e) = written {
                    drop(f);
                    let _ = fs::remove_file(&path);
                    return Err(FileError::io(&path, e));
                }
                return Ok(path);
            }
            // Taken meanwhile: the next free name.
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(FileError::io(&path, e)),
        }
    }
    Err(FileError::new(
        "invalid_name",
        format!("`{name}.{ext}`: the exports folder keeps changing"),
    ))
}

/// Writes job `id` of `store` to `dest` as a bundle. Nothing is written
/// to `dest` unless the whole bundle was: it is built beside it and moved
/// there at the end, so a failed export neither leaves a bundle that lacks
/// the attached files nor replaces the file that was there (audit
/// 2026-11-15 HUNTIO-006).
///
/// `dest` must end in `.tectorajob` and lie outside the library's own
/// folders, so an export can never write over a job file or into the
/// files, change logs or trash of the library (HUNTIO-002); see
/// [`with_extension`] for a path typed without the ending. A job with an
/// attached file of a type import refuses is not exported either
/// (`not_attachable`, HUNTIO-001).
pub fn export(store: &Store, id: &str, dest: &Path) -> Result<Manifest, FileError> {
    check_dest(store, dest)?;
    let project = store.load(id)?;
    // Never files from where a link in the library points (ARCH-602).
    let attachments = store.try_attachments(id)?;
    if let Some(bad) = attachments.iter().find(|a| !Store::can_attach(&a.name)) {
        return Err(FileError::new(
            "not_attachable",
            format!(
                "job bundle: the attached file `{}` is not a file type a bundle carries; remove it from the job and export again",
                bad.name
            ),
        ));
    }
    // A bundle import refuses is not written: the job would be lost to
    // whoever receives it (ARCH-003).
    if attachments.len() > MAX_FILES {
        return Err(FileError::new(
            "too_many",
            format!(
                "job bundle: too many attached files ({}, at most {MAX_FILES})",
                attachments.len()
            ),
        ));
    }
    let total: u64 = attachments.iter().map(|a| a.size).sum();
    if total > MAX_FILES_TOTAL {
        return Err(too_large(format!(
            "the attached files are too large ({} MB, at most {} MB)",
            total >> 20,
            MAX_FILES_TOTAL >> 20
        )));
    }
    let part = dest.with_file_name(format!(
        ".{}.{}.part",
        dest.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("bundle"),
        std::process::id()
    ));
    let written = write_bundle(store, id, &project, &attachments, &part, dest);
    let done = written.and_then(|m| {
        fs::rename(&part, dest)
            .map_err(|e| FileError::io(dest, e))
            .map(|()| m)
    });
    if done.is_err() {
        let _ = fs::remove_file(&part);
    }
    done
}

/// Characters a file name does not end in: Windows drops trailing spaces
/// and dots, and a trailing blank of another kind (U+3000) is trimmed as
/// MCP trims it, so the CLI and MCP name one file alike (HUNT-305 item 2).
fn blank_or_dot(c: char) -> bool {
    c.is_whitespace() || c == '.'
}

/// `path` with `.tectorajob` added when it does not end in it (in any
/// case): `job.tectora` becomes `job.tectora.tectorajob`, never the job
/// file itself. For the CLI and the app's save dialog.
pub fn with_extension(path: &Path) -> PathBuf {
    // Trailing spaces and dots are no part of a name on Windows, which
    // drops them (round 2 ARCH-106): "a.tectorajob " is "a.tectorajob".
    let trimmed = path
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.trim_end_matches(blank_or_dot))
        .filter(|n| !n.is_empty());
    let path = &match trimmed {
        Some(n) => path.with_file_name(n),
        None => path.to_path_buf(),
    };
    let has = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case(EXTENSION));
    if has {
        return path.to_path_buf();
    }
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{EXTENSION}"));
    path.with_file_name(name)
}

/// The rules of [`export`] for its destination.
fn check_dest(store: &Store, dest: &Path) -> Result<(), FileError> {
    let name = dest.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let has_ext = dest
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case(EXTENSION));
    // A name of only spaces and dots before the extension is no name on
    // Windows, which drops them (ARCH-205 item 2).
    let stem = name
        .get(..name.len().saturating_sub(EXTENSION.len() + 1))
        .unwrap_or("")
        .trim_matches([' ', '.']);
    if !has_ext || stem.is_empty() || !crate::store::valid_file_name(name) {
        return Err(FileError::new(
            "invalid_name",
            format!(
                "{}: a job bundle must be a plain file name ending in .{EXTENSION}",
                dest.display()
            ),
        ));
    }
    // Compared as the folders really are (links, `..`, the case of a
    // macOS or Windows file system): the folder of `dest` exists when the
    // export can write there at all.
    let Some(parent) = dest.parent() else {
        return Ok(());
    };
    let parent = if parent.as_os_str().is_empty() {
        Path::new(".")
    } else {
        parent
    };
    let (Ok(at), Ok(lib)) = (parent.canonicalize(), store.dir.canonicalize()) else {
        return Ok(());
    };
    let inside = [
        crate::store::FILES_DIR,
        crate::store::ACTIVITY_DIR,
        crate::store::TRASH_DIR,
    ]
    .iter()
    .any(|d| at.starts_with(lib.join(d)));
    if inside {
        return Err(FileError::new(
            "invalid_name",
            format!(
                "{}: a job bundle cannot be written into the library's own folders; choose another folder",
                dest.display()
            ),
        ));
    }
    Ok(())
}

fn write_bundle(
    store: &Store,
    id: &str,
    project: &Project,
    attachments: &[crate::store::Attachment],
    part: &Path,
    dest: &Path,
) -> Result<Manifest, FileError> {
    let manifest = Manifest {
        format: FORMAT.into(),
        format_version: FORMAT_VERSION,
        name: project.name.clone(),
        engine_version: crate::snapshot::ENGINE_VERSION.into(),
        exported_ms: now_ms(),
        files: attachments.iter().map(|a| a.name.clone()).collect(),
    };
    // A file left by an earlier export of this process goes; a link put
    // at the name is removed itself, never written through (ARCH-603).
    if fs::symlink_metadata(part).is_ok_and(|m| !m.is_dir()) {
        let _ = fs::remove_file(part);
    }
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(part)
        .map_err(|e| FileError::io(dest, e))?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    // Writing fails for file system reasons (disk full, folder read-only).
    let write_err = |e: zip::result::ZipError| match e {
        zip::result::ZipError::Io(e) => FileError::io(dest, e),
        e => FileError::new("io", format!("{}: {e}", dest.display())),
    };
    let mut put = |name: &str, bytes: &[u8]| -> Result<(), FileError> {
        zip.start_file(name, opts).map_err(write_err)?;
        zip.write_all(bytes).map_err(|e| FileError::io(dest, e))
    };
    let json_err = |e: serde_json::Error| FileError::new("internal", e.to_string());
    put(
        "manifest.json",
        serde_json::to_string_pretty(&manifest)
            .map_err(json_err)?
            .as_bytes(),
    )?;
    // A bundle import refuses is not written (HUNT-101): a job file over
    // what import reads stops the export, a change log over it goes in
    // with its newest entries only.
    // The app names the job's own size by this detail's wording (errors.ts,
    // ARCH-205): keep "the job is too large to hand over" when editing it.
    let text = project.to_json().map_err(json_err)?;
    if text.len() as u64 > MAX_TEXT_ENTRY {
        return Err(too_large(format!(
            "the job is too large to hand over ({} MB, at most {} MB); shorten its notes",
            text.len() >> 20,
            MAX_TEXT_ENTRY >> 20
        )));
    }
    put("project.tectora", text.as_bytes())?;
    // Bounded, and never blocked by a pipe in its place (ARCH-301).
    if let Ok(history) = crate::store::read_newest_lines(&store.activity_path(id)?, MAX_TEXT_ENTRY)
    {
        let newest = newest_lines(&history, MAX_TEXT_ENTRY as usize);
        if !newest.is_empty() {
            put("history.jsonl", newest)?;
        }
    }
    // The preview runs the roof kernel: a job out of range, one the kernel
    // cannot calculate or one too complex to calculate (`work`) is still
    // exported, without preview (ERR-008).
    let svg = project
        .check_limits()
        .ok()
        .and_then(|()| crate::work::calculate(|| preview_svg(project)).ok());
    if let Some(svg) = svg {
        put("preview.svg", svg.as_bytes())?;
    }
    for a in attachments {
        let path = store.attachment_path(id, &a.name)?;
        let bytes = crate::store::read_regular(&path, MAX_ENTRY)?;
        put(&format!("files/{}", a.name), &bytes)?;
    }
    zip.finish().map_err(write_err)?;
    Ok(manifest)
}

/// The newest whole lines of the change log `bytes` that together take at
/// most `max` bytes (all of it when it fits).
fn newest_lines(bytes: &[u8], max: usize) -> &[u8] {
    if bytes.len() <= max {
        return bytes;
    }
    let from = bytes.len() - max;
    // A line starts after a line break at `from - 1` or later.
    match bytes[from - 1..].iter().position(|&b| b == b'\n') {
        Some(at) => &bytes[from + at..],
        None => &[],
    }
}

/// Reads entry `name`, at most `max` bytes (its declared size is checked
/// first, the bytes actually inflated as they are read).
fn read_entry<R: Read + std::io::Seek>(
    zip: &mut zip::ZipArchive<R>,
    name: &str,
    max: u64,
) -> Result<Option<Vec<u8>>, FileError> {
    let mut f = match zip.by_name(name) {
        Ok(f) => f,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(e) => return Err(zip_err(e)),
    };
    if f.size() > max {
        return Err(too_large(format!("{name} is too large")));
    }
    let mut buf = Vec::new();
    f.by_ref()
        .take(max + 1)
        .read_to_end(&mut buf)
        .map_err(zip_err)?;
    if buf.len() as u64 > max {
        return Err(too_large(format!("{name} is too large")));
    }
    Ok(Some(buf))
}

/// Adds the job in bundle `src` to `store` as a new job; returns its id.
/// A named pipe or device called `*.tectorajob` is refused before it is
/// opened: opening it blocked the MCP server for good (ARCH-002).
pub fn import(store: &Store, src: &Path) -> Result<String, FileError> {
    let file = crate::store::open_regular(src, MAX_BUNDLE)?;
    let mut zip = zip::ZipArchive::new(file).map_err(zip_err)?;
    let manifest = read_entry(&mut zip, "manifest.json", MAX_TEXT_ENTRY)?;
    let mut manifest_value = None;
    if let Some(m) = &manifest {
        // The format and its version first: a newer Tectora may have
        // changed the rest of the manifest (ERR-007).
        let v: serde_json::Value =
            serde_json::from_slice(m).map_err(|_| zip_err("the manifest is not JSON"))?;
        if v.get("format").and_then(serde_json::Value::as_str) != Some(FORMAT) {
            return Err(zip_err("not a Tectora job"));
        }
        // Read as a project's `schema_version` is: `2.0` and `1e30` are
        // newer versions, anything else not a number is a plain refusal,
        // never a raw parser text (HUNT-406).
        let version = v
            .get("format_version")
            .map(|n| {
                crate::store::whole_version(n)
                    .ok_or_else(|| zip_err("the bundle format version is not a whole number"))
            })
            .transpose()?;
        if let Some(version) = version.filter(|&n| n > u64::from(FORMAT_VERSION)) {
            let shown = if version <= u64::from(u32::MAX) {
                format!("bundle format {version}")
            } else {
                "a later bundle format".to_string()
            };
            return Err(FileError::new(
                "newer_format",
                format!(
                    "job bundle: made by a newer Tectora ({shown}, this version reads up to {FORMAT_VERSION})"
                ),
            ));
        }
        manifest_value = Some((v, version));
    }
    let text = read_entry(&mut zip, "project.tectora", MAX_TEXT_ENTRY)?
        .ok_or_else(|| zip_err("no project in the file"))?;
    // A project from a newer Tectora is `newer_format` whatever the
    // manifest says.
    // The texts keep the rules of a write: no control or invisible
    // characters, nothing too long (ARCH-201).
    let project = parse_project(
        &crate::store::job_text(text, "job bundle: project.tectora")?,
        "job bundle: project.tectora",
        &file_stem(src),
    )?;
    if let Some((mut v, version)) = manifest_value {
        // `1.0` is version 1.
        if let Some(n) = version {
            v["format_version"] = n.into();
        }
        serde_json::from_value::<Manifest>(v).map_err(|_| {
            zip_err("the manifest is damaged (a field is missing or of another type)")
        })?;
    }
    // A change log over the limit (bundles exported before HUNT-101) is
    // left out: the job and its files matter more than its history.
    let history = match read_entry(&mut zip, "history.jsonl", MAX_TEXT_ENTRY) {
        Err(e) if e.code == "too_large" => None,
        h => h?,
    };
    let names: Vec<String> = zip
        .file_names()
        .filter_map(|n| n.ok())
        .filter_map(|n| n.strip_prefix("files/").map(str::to_string))
        .filter(|n| !n.is_empty() && !n.contains('/'))
        .collect();
    // Checked before anything is written, so a refused bundle leaves no
    // half-imported job behind. A file Tectora would not attach (a program,
    // a script) is not taken from a bundle either: extracted here it carries
    // no "downloaded" mark, so the system would run it without a warning
    // when it is double-clicked in the job's folder (HUNTIO-014).
    if let Some(bad) = names
        .iter()
        .find(|n| crate::store::valid_file_name(n) && !Store::can_attach(n))
    {
        return Err(FileError::new(
            "unsupported_type",
            format!("job bundle: `{bad}` is not a file type Tectora attaches"),
        ));
    }
    if names.len() > MAX_FILES {
        return Err(too_large(format!(
            "too many attached files ({}, at most {MAX_FILES})",
            names.len()
        )));
    }
    let mut declared = 0u64;
    for name in &names {
        let f = zip.by_name(&format!("files/{name}")).map_err(zip_err)?;
        declared = declared.saturating_add(f.size());
    }
    if declared > MAX_FILES_TOTAL {
        return Err(too_large(format!(
            "the attached files are too large ({} MB, at most {} MB)",
            declared >> 20,
            MAX_FILES_TOTAL >> 20
        )));
    }
    let id = store.save(None, &project)?;
    let rest = (|| -> Result<(), FileError> {
        if let Some((history, last)) = history.as_deref().and_then(received_history) {
            let path = store.activity_path(&id)?;
            let dir = store.activity_dir();
            crate::store::own_folder(&dir).map_err(|e| FileError::io(&dir, e))?;
            // A new file, never through a link left at its path, which
            // would put the sender's history outside the library (ARCH-501).
            let mut f = crate::store::open_regular_for_write(
                &path,
                fs::OpenOptions::new().write(true).create_new(true),
            )
            .map_err(|e| FileError::io(&path, e))?;
            f.write_all(history.as_bytes())
                .and_then(|()| crate::store::sync_file(&f))
                .map_err(|e| FileError::io(&path, e))?;
            // The sender's changes are history, not an AI change waiting
            // for this user's review.
            store.mark_reviewed(&id, last)?;
        }
        let mut total = 0u64;
        for name in names {
            // The declared sizes can lie: the bytes read count too.
            let max = MAX_ENTRY.min(MAX_FILES_TOTAL - total);
            if let Some(bytes) = read_entry(&mut zip, &format!("files/{name}"), max)? {
                total += bytes.len() as u64;
                store.add_attachment(&id, &name, &bytes)?;
            }
        }
        Ok(())
    })();
    if let Err(e) = rest {
        // No half-imported job: the job, its log and its files go again.
        let _ = store.purge(&id);
        return Err(e);
    }
    Ok(id)
}

#[cfg(test)]
thread_local! {
    /// Received history entries diffed by this thread, so a test checks
    /// the work done, not the time it took (TEST-506 item 2).
    static HISTORY_DIFFS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The change log of a received bundle as it is written into the library,
/// with the time of its last entry. Entries that do not read as a change,
/// or whose versions are outside the limits a job file is held to, are
/// left out: a revert writes such a version into the job unchecked
/// (HUNTIO-004). Times ahead of this computer's clock are moved back, the
/// order kept, so the next change is later than all of them (HUNTIO-005).
fn received_history(bytes: &[u8]) -> Option<(String, u64)> {
    let ok = |p: &Option<Project>| p.as_ref().is_none_or(|p| p.check_limits().is_ok());
    // Only the newest entries a change log keeps, read from the end before
    // anything is cleaned or diffed: a bundle of 20 000 entries put them
    // all into the library, which froze the job's AI panel (ARCH-503).
    let mut entries: Vec<crate::store::ActivityEntry> = bytes
        .rsplit(|&b| b == b'\n')
        .filter_map(|l| serde_json::from_slice(l).ok())
        .take(crate::store::ACTIVITY_KEEP)
        .collect();
    entries.reverse();
    // Who and why as a write keeps them, and the versions a revert puts
    // back with the texts of a received job (ARCH-201). What changed is
    // worked out again from the cleaned versions, as a save does: the
    // sender's list could hold any value (a null crashed the app's change
    // list) or a change that never happened (HUNT-301).
    for e in &mut entries {
        e.summary = crate::edit::clean_line(&e.summary, crate::edit::MAX_SUMMARY_CHARS);
        e.actor = crate::edit::clean_line(&e.actor, crate::edit::MAX_ACTOR_CHARS);
        e.reason = e
            .reason
            .as_deref()
            .map(|r| crate::edit::clean_line(r, crate::edit::MAX_SUMMARY_CHARS));
        // An entry that says Tectora itself wrote it was written by the
        // sender: it is kept as history under another name and never
        // shown or trusted as this engine's recalculation (audit
        // 2026-11-22 ARCH-802).
        // Also one that already reads as received (any spelling), so the
        // app knows it by its one name (audit 2026-11-23 ARCH-901).
        if crate::store::reserved_actor(&e.actor).is_some_and(|r| r != crate::store::USER_ACTOR) {
            e.actor = crate::store::RECEIVED_ENGINE_ACTOR.to_string();
        }
        for p in [&mut e.before, &mut e.after].into_iter().flatten() {
            let name = p.name.clone();
            crate::edit::clean_received(p, &name);
        }
    }
    // Versions out of the limits are dropped before their changes are
    // worked out: diffing an unbounded version took minutes (HUNT-401).
    entries.retain(|e| ok(&e.before) && ok(&e.after));
    for e in &mut entries {
        e.changes = match (&e.before, &e.after) {
            (Some(b), Some(a)) => {
                #[cfg(test)]
                HISTORY_DIFFS.with(|n| n.set(n.get() + 1));
                crate::diff::diff(b, a)
            }
            _ => Vec::new(),
        };
    }
    if entries.is_empty() {
        return None;
    }
    let last = order_times(&mut entries, now_ms());
    // Within the bytes a log is read to, as a save trims it; every entry
    // is the sender's, none waits for this user's review.
    let text = crate::store::trim_log(&entries, entries.len(), crate::store::MAX_ACTIVITY_FILE);
    Some((text, last))
}

/// Gives the received `entries` (file order) times this computer can
/// follow, and returns the newest. Times ahead of `now` are moved back by
/// how far the newest is ahead; then each entry is at least 1 ms later
/// than the one before, so no two collapse into one time (an entry dated
/// far ahead moved the others to 0, and a revert then named the wrong
/// one: audit 2026-11-24 HUNT-1602). A revert's and a put-back's link is
/// rewritten to the new time of the entry it named (the newest earlier
/// one of that time, as `revert_range` finds it), or moved like the
/// times when it names no entry (it dangles and is read by time, audit
/// 2026-11-23 HUNT-1501, DANGLING). Nothing is dropped.
fn order_times(entries: &mut [crate::store::ActivityEntry], now: u64) -> u64 {
    let newest = entries.iter().map(|e| e.ts_ms).max().unwrap_or(0);
    let shift = newest.saturating_sub(now);
    let mut new_time: std::collections::HashMap<u64, u64> = std::collections::HashMap::new();
    let mut prev: Option<u64> = None;
    for e in entries.iter_mut() {
        let old = e.ts_ms;
        let shifted = old.saturating_sub(shift);
        let ts = prev.map_or(shifted, |p| shifted.max(p.saturating_add(1)));
        for link in [&mut e.undid_from, &mut e.restores].into_iter().flatten() {
            *link = new_time
                .get(link)
                .copied()
                .unwrap_or_else(|| link.saturating_sub(shift));
        }
        e.ts_ms = ts;
        new_time.insert(old, ts);
        prev = Some(ts);
    }
    prev.unwrap_or(0)
}

/// The library job for a file the user opened (double-click, drag and drop):
/// a `.tectora` file in the library opens as it is, one from elsewhere and a
/// `.tectorajob` bundle are added to the library as a new job.
pub fn open_path(store: &Store, path: &Path) -> Result<String, FileError> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext == EXTENSION {
        return import(store, path);
    }
    if ext != crate::store::EXTENSION {
        return Err(FileError::new(
            "unsupported_type",
            format!("{}: not a Tectora file", path.display()),
        ));
    }
    let same_dir = |a: &Path, b: &Path| match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    };
    if let (Some(parent), Some(stem)) = (path.parent(), path.file_stem().and_then(|s| s.to_str()))
        && same_dir(parent, &store.dir)
        && store.exists(stem)
    {
        return Ok(stem.to_string());
    }
    let text = crate::store::job_text(
        crate::store::read_regular(path, MAX_TEXT_ENTRY)?,
        &path.display().to_string(),
    )?;
    let project = parse_project(&text, &path.display().to_string(), &file_stem(path))?;
    Ok(store.save(None, &project)?)
}

/// The file name of `path` without its extension, for a job name.
fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// What opening several files (double-click, start-up arguments) gave.
#[derive(Debug, Default, PartialEq)]
pub struct Opened {
    /// The job of the last file that opened, to show.
    pub last: Option<String>,
    /// Each file that did not open, with why: the code the app translates
    /// and the English detail.
    pub failed: Vec<(PathBuf, FileError)>,
}

/// [`open_path`] for each file; a file that fails does not stop the others,
/// and its error is kept for the user (not only a log line).
pub fn open_paths(store: &Store, paths: impl IntoIterator<Item = PathBuf>) -> Opened {
    let mut out = Opened::default();
    for p in paths {
        // A panic is this file's failure, told to the user like any other:
        // the app opens files on a thread of its own, which a panic ended
        // without a word.
        let opened = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            #[cfg(test)]
            if file_stem(&p) == OPEN_PANICS {
                panic!("a test file whose import panics");
            }
            open_path(store, &p)
        }))
        .unwrap_or_else(|e| {
            Err(FileError::new(
                "internal",
                format!(
                    "{}: opening the file failed: {}",
                    p.display(),
                    crate::store::panic_message(&*e)
                ),
            ))
        });
        match opened {
            Ok(id) => out.last = Some(id),
            Err(e) => out.failed.push((p, e)),
        }
    }
    out
}

/// The name of a test file whose import panics.
#[cfg(test)]
const OPEN_PANICS: &str = "test-open-panics";

/// Plan preview: wall outlines and roof lines, y up, 1 unit = 1 m.
pub fn preview_svg(project: &Project) -> String {
    let outlines = outlines(project);
    let lines = roof_lines(project);
    let pts = outlines.iter().flatten().chain(lines.iter().flatten());
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for p in pts {
        x0 = x0.min(p[0]);
        y0 = y0.min(p[1]);
        x1 = x1.max(p[0]);
        y1 = y1.max(p[1]);
    }
    if x0 > x1 {
        (x0, y0, x1, y1) = (0.0, 0.0, 1.0, 1.0);
    }
    let pad = 0.05 * (x1 - x0).max(y1 - y0).max(1.0);
    let f = |v: f64| format!("{:.3}", v);
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{} {} {} {}\">\n<g transform=\"scale(1,-1)\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\">\n",
        f(x0 - pad),
        f(-y1 - pad),
        f(x1 - x0 + 2.0 * pad),
        f(y1 - y0 + 2.0 * pad)
    );
    let w = f(0.004 * (x1 - x0).max(y1 - y0).max(1.0));
    for o in &outlines {
        let d: Vec<String> = o
            .iter()
            .map(|p| format!("{},{}", f(p[0]), f(p[1])))
            .collect();
        svg += &format!(
            "<polygon points=\"{}\" stroke=\"#5c5b59\" stroke-width=\"{w}\"/>\n",
            d.join(" ")
        );
    }
    for [a, b] in &lines {
        svg += &format!(
            "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"#a8352d\" stroke-width=\"{w}\"/>\n",
            f(a[0]),
            f(a[1]),
            f(b[0]),
            f(b[1])
        );
    }
    svg + "</g>\n</svg>\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-6 audit (arch "To check" 40): a file whose import panics is
    /// reported like any file that does not open, and the others still
    /// open; the app's open thread used to end without a word.
    #[test]
    fn a_file_whose_import_panics_is_reported_and_the_others_open() {
        let dir = std::env::temp_dir().join(format!("tectora-open-panic-{}", now_ms()));
        let store = Store::new(dir.join("lib")).unwrap();
        let good = dir.join("good.tectora");
        fs::write(
            &good,
            crate::examples::find("hip")
                .unwrap()
                .project()
                .to_json()
                .unwrap(),
        )
        .unwrap();
        let bad = dir.join(format!("{OPEN_PANICS}.tectora"));
        fs::write(&bad, "{}").unwrap();
        let r = open_paths(&store, [bad.clone(), good]);
        assert!(r.last.is_some());
        assert_eq!(r.failed.len(), 1);
        assert_eq!(r.failed[0].0, bad);
        assert_eq!(r.failed[0].1.code, "internal");
        assert!(
            r.failed[0].1.detail.contains("failed"),
            "{}",
            r.failed[0].1.detail
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn files_that_do_not_open_are_reported_and_the_others_open() {
        let dir = std::env::temp_dir().join(format!("tectora-open-{}", now_ms()));
        let store = Store::new(dir.join("lib")).unwrap();
        let good = dir.join("good.tectora");
        let p = crate::examples::find("hip").unwrap().project();
        fs::write(&good, p.to_json().unwrap()).unwrap();
        let broken = dir.join("broken.tectora");
        fs::write(&broken, "{ not json").unwrap();
        let other = dir.join("notes.txt");
        fs::write(&other, "x").unwrap();
        let r = open_paths(&store, [broken.clone(), good, other.clone()]);
        assert!(r.last.is_some());
        let failed: Vec<&PathBuf> = r.failed.iter().map(|f| &f.0).collect();
        assert_eq!(failed, [&broken, &other]);
        assert!(r.failed[1].1.detail.contains("not a Tectora file"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_job_survives_export_and_import_with_files_and_history() {
        let dir = std::env::temp_dir().join(format!("tectora-bundle-{}", now_ms()));
        let store = Store::new(dir.join("lib")).unwrap();
        let mut p = crate::examples::find("hip").unwrap().project();
        let id = store.save(None, &p).unwrap();
        p.parts[0].edges[0].pitch_deg = 35.0;
        store
            .save_as(Some(id.clone()), &p, "AI", "steeper")
            .unwrap();
        store
            .add_attachment(&id, "drawing.pdf", b"%PDF-1.4 test")
            .unwrap();
        assert_eq!(
            store.add_attachment(&id, "drawing.pdf", b"x").unwrap(),
            "drawing (2).pdf"
        );

        let file = dir.join("job.tectorajob");
        let m = export(&store, &id, &file).unwrap();
        assert_eq!(m.files, vec!["drawing (2).pdf", "drawing.pdf"]);

        let other = Store::new(dir.join("other")).unwrap();
        let new_id = import(&other, &file).unwrap();
        assert_eq!(other.load(&new_id).unwrap(), store.load(&id).unwrap());
        assert_eq!(other.attachments(&new_id).len(), 2);
        assert_eq!(other.activity(&new_id).len(), store.activity(&id).len());
        // The sender's AI change is history for the receiver, not a review.
        assert!(other.pending_review(&new_id).is_none());
        let _ = fs::remove_dir_all(dir);
    }

    /// Audit 2026-11-21 HUNT-101: a change log over the size import reads
    /// (50 changes of a big job) is trimmed to its newest entries on export,
    /// so the bundle imports; a job import could not read is not exported;
    /// a bundle written before the fix imports without its log.
    #[test]
    fn a_big_change_log_is_trimmed_so_the_bundle_still_imports() {
        let dir = std::env::temp_dir().join(format!("tectora-bundle-hist-{}", now_ms()));
        let store = Store::new(dir.join("lib")).unwrap();
        let p = crate::examples::find("hip").unwrap().project();
        let id = store.save(None, &p).unwrap();
        let mut big = p.clone();
        big.notes = "n".repeat(512 * 1024);
        let line = |ts: u64| {
            serde_json::to_string(&crate::store::ActivityEntry {
                ts_ms: ts,
                actor: "Claude".into(),
                summary: format!("change {ts}"),
                before: Some(big.clone()),
                changes: Vec::new(),
                after: Some(big.clone()),
                undid_from: None,
                restores: None,
                reason: None,
            })
            .unwrap()
                + "\n"
        };
        let t0 = now_ms() - 1_000_000;
        let history: String = (0..20).map(|i| line(t0 + i)).collect();
        assert!(history.len() as u64 > MAX_TEXT_ENTRY);
        fs::create_dir_all(store.activity_dir()).unwrap();
        fs::write(store.activity_path(&id).unwrap(), &history).unwrap();

        let file = dir.join("job.tectorajob");
        export(&store, &id, &file).unwrap();
        let other = Store::new(dir.join("other")).unwrap();
        let new_id = import(&other, &file).unwrap();
        assert_eq!(other.load(&new_id).unwrap(), store.load(&id).unwrap());
        let log = other.activity(&new_id);
        assert!(!log.is_empty() && log.len() < 20, "{}", log.len());
        assert_eq!(log.last().unwrap().summary, format!("change {}", t0 + 19));

        // A bundle from before: its log over the limit is left out, the job
        // still comes in.
        let old = dir.join("old.tectorajob");
        let mut zip = zip::ZipWriter::new(fs::File::create(&old).unwrap());
        let opts = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .large_file(true);
        zip.start_file("project.tectora", opts).unwrap();
        zip.write_all(p.to_json().unwrap().as_bytes()).unwrap();
        zip.start_file("history.jsonl", opts).unwrap();
        zip.write_all(history.as_bytes()).unwrap();
        zip.finish().unwrap();
        let old_id = import(&other, &old).unwrap();
        assert!(other.activity(&old_id).is_empty());

        // A job whose own file is over what import reads is not exported.
        let mut huge = p.clone();
        huge.notes = "n".repeat(MAX_TEXT_ENTRY as usize + 1);
        let huge_id = store.save(None, &huge).unwrap();
        let err = export(&store, &huge_id, &dir.join("huge.tectorajob")).unwrap_err();
        assert_eq!(err.code, "too_large");
        assert!(!dir.join("huge.tectorajob").exists());
        let _ = fs::remove_dir_all(dir);
    }

    /// Audit 2026-11-15: ARCH-003 (a bundle import refuses is not written),
    /// HUNTIO-006 (a failed export leaves the old file and no half bundle).
    #[test]
    fn export_refuses_what_import_would_refuse_and_keeps_the_old_file() {
        let dir = std::env::temp_dir().join(format!("tectora-bundle-lim-{}", now_ms()));
        let store = Store::new(dir.join("lib")).unwrap();
        let p = crate::examples::find("hip").unwrap().project();
        let id = store.save(None, &p).unwrap();
        for i in 0..=MAX_FILES {
            store
                .add_attachment(&id, &format!("n{i}.txt"), b"x")
                .unwrap();
        }
        let file = dir.join("job.tectorajob");
        fs::write(&file, b"older bundle").unwrap();
        assert_eq!(export(&store, &id, &file).unwrap_err().code, "too_many");
        assert_eq!(fs::read(&file).unwrap(), b"older bundle");
        // A file that cannot be read stops the export the same way.
        // A file whose name Tectora refuses: on macOS and Linux what Finder
        // writes for a "/"; on Windows a ":" would make an NTFS stream, so a
        // name over 255 UTF-8 bytes (NTFS counts 200 UTF-16 units).
        let small = store.save(None, &p).unwrap();
        store.add_attachment(&small, "plan.pdf", b"%PDF").unwrap();
        let files = store.files_dir(&small).unwrap();
        let refused = if cfg!(windows) {
            format!("{}.pdf", "ž".repeat(200))
        } else {
            "Plan 1:2.pdf".to_string()
        };
        assert!(!crate::store::valid_file_name(&refused));
        fs::write(files.join(&refused), b"%PDF").unwrap();
        assert!(export(&store, &small, &file).is_err());
        assert_eq!(fs::read(&file).unwrap(), b"older bundle");
        let left: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".part"))
            .collect();
        assert!(left.is_empty(), "{left:?}");
        let _ = fs::remove_dir_all(dir);
    }

    /// HUNTIO-004, HUNTIO-014: a received bundle's log cannot put a version
    /// outside the limits into the job, and it cannot bring a program.
    #[test]
    fn a_received_bundle_cannot_bring_a_bad_history_or_a_program() {
        let dir = std::env::temp_dir().join(format!("tectora-bundle-hostile-{}", now_ms()));
        fs::create_dir_all(&dir).unwrap();
        let p = crate::examples::find("gable").unwrap().project();
        let mut bad = p.clone();
        bad.parts[0].edges[0].start.x = 1e300;
        let entry = |before: &Project, ts: u64| {
            serde_json::to_string(&crate::store::ActivityEntry {
                ts_ms: ts,
                actor: "Claude".into(),
                summary: "x".into(),
                before: Some(before.clone()),
                changes: Vec::new(),
                after: Some(p.clone()),
                undid_from: None,
                restores: None,
                reason: None,
            })
            .unwrap()
        };
        let write = |name: &str, files: &[&str], history: &str| {
            let path = dir.join(name);
            let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
            let opts = SimpleFileOptions::default();
            zip.start_file("project.tectora", opts).unwrap();
            zip.write_all(p.to_json().unwrap().as_bytes()).unwrap();
            zip.start_file("history.jsonl", opts).unwrap();
            zip.write_all(history.as_bytes()).unwrap();
            for f in files {
                zip.start_file(format!("files/{f}"), opts).unwrap();
                zip.write_all(b"MZ").unwrap();
            }
            zip.finish().unwrap();
            path
        };
        let store = Store::new(dir.join("lib")).unwrap();
        let ahead = now_ms() + 86_400_000;
        let history = entry(&bad, ahead) + "\n" + &entry(&p, ahead + 1) + "\n";
        let id = import(&store, &write("h.tectorajob", &[], &history)).unwrap();
        let log = store.activity(&id);
        assert_eq!(log.len(), 1, "the entry with the bad version is left out");
        assert!(log[0].ts_ms <= now_ms());
        assert!(store.pending_review(&id).is_none());
        for name in ["invoice.pdf.exe", "run.command"] {
            let err = import(&store, &write("x.tectorajob", &[name], "")).unwrap_err();
            assert_eq!(err.code, "unsupported_type", "{name}");
        }
        let _ = fs::remove_dir_all(dir);
    }

    /// Audit 2026-11-21 ARCH-201, TEST-206: a received job keeps the text
    /// rules of a write: control and bidirectional characters are dropped,
    /// over-long texts and the change log's summaries and actors are cut,
    /// and the job still opens. The same for a `.tectora` from outside.
    #[test]
    fn a_received_job_and_its_log_keep_the_text_rules() {
        let dir = std::env::temp_dir().join(format!("tectora-bundle-text-{}", now_ms()));
        fs::create_dir_all(&dir).unwrap();
        let mut p = crate::examples::find("gable").unwrap().project();
        p.name = "Evil\u{202e}raboof".into();
        p.job.number = "N\u{0}\u{202e}1".into();
        p.job.customer = "A\u{202e}B".into();
        p.job.address = "X\u{7}Y".into();
        p.notes = "line 1\nline\t2\u{202e}".into();
        p.parts[0].name = "Wing\u{202e}\u{0}".into();
        p.material_names = vec!["m".repeat(5000)];
        // TEST-306 R4 / R5, HUNT-301: hand-drawn line and face texts.
        p.manual_lines.push(crate::model::ManualLine {
            a: [0.0, 0.0, 0.0],
            b: [1.0, 0.0, 0.0],
            code: "\u{202e}T\u{7}".to_string() + &"X".repeat(20_000),
            length: None,
            label: "F\u{202e}\u{0}".into(),
        });
        p.manual_faces.push(crate::model::ManualFace {
            points: vec![
                crate::geom::Point2::new(20.0, 0.0),
                crate::geom::Point2::new(24.0, 0.0),
                crate::geom::Point2::new(24.0, 4.0),
            ],
            pitch_deg: 30.0,
            label: "F\u{202e}\u{0}".into(),
        });
        // TEST-303: the version before the change is cleaned too.
        let mut older = p.clone();
        older.notes = "older".into();
        let entry = serde_json::to_string(&crate::store::ActivityEntry {
            ts_ms: 1,
            actor: "x".repeat(10_000),
            summary: "S".repeat(100_000) + "\u{202e}",
            before: Some(older),
            changes: Vec::new(),
            after: Some(p.clone()),
            undid_from: None,
            restores: None,
            reason: None,
        })
        .unwrap();
        let path = dir.join("evil.tectorajob");
        let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
        let opts = SimpleFileOptions::default();
        zip.start_file("project.tectora", opts).unwrap();
        zip.write_all(p.to_json().unwrap().as_bytes()).unwrap();
        zip.start_file("history.jsonl", opts).unwrap();
        zip.write_all(entry.as_bytes()).unwrap();
        zip.finish().unwrap();
        let store = Store::new(dir.join("lib")).unwrap();
        let check = |got: &Project| {
            assert_eq!(got.name, "Evilraboof");
            assert_eq!(got.job.number, "N1");
            assert_eq!(got.job.customer, "AB");
            assert_eq!(got.job.address, "XY");
            assert_eq!(got.notes, "line 1\nline\t2");
            assert_eq!(got.parts[0].name, "Wing");
            assert_eq!(
                got.material_names[0].chars().count(),
                crate::edit::MAX_LABEL_CHARS
            );
            assert_eq!(got.manual_lines[0].label, "F");
            assert_eq!(got.manual_faces[0].label, "F");
            assert_eq!(got.manual_lines[0].code, "M");
            assert!(crate::edit::check_texts_change(None, got).is_ok());
        };
        let id = import(&store, &path).unwrap();
        check(&store.load(&id).unwrap());
        let log = store.activity(&id);
        assert_eq!(log.len(), 1);
        assert_eq!(
            log[0].summary.chars().count(),
            crate::edit::MAX_SUMMARY_CHARS
        );
        assert!(!log[0].summary.contains('\u{202e}'));
        assert_eq!(log[0].actor.chars().count(), crate::edit::MAX_ACTOR_CHARS);
        check(log[0].after.as_ref().unwrap());
        let as_after = |p: &Project| {
            assert_eq!(p.notes, "older");
            let mut b = p.clone();
            b.notes = "line 1\nline\t2".into();
            b
        };
        check(&as_after(log[0].before.as_ref().unwrap()));
        // A revert of the received change writes the cleaned version back.
        check(&as_after(&store.revert_last(&id, "user", None).unwrap()));
        // A `.tectora` opened from elsewhere; a name of nothing visible
        // takes the file's.
        let outside = dir.join("Garage.tectora");
        let mut q = p.clone();
        q.name = "\u{202e}\u{0}".into();
        fs::write(&outside, q.to_json().unwrap()).unwrap();
        let id = open_path(&store, &outside).unwrap();
        let got = store.load(&id).unwrap();
        assert_eq!(got.name, "Garage");
        assert_eq!(got.job.customer, "AB");
        let _ = fs::remove_dir_all(dir);
    }

    /// A bundle of `project` with `history` as its change log.
    fn bundle_with_log(dir: &Path, name: &str, project: &str, history: &str) -> PathBuf {
        let path = dir.join(name);
        let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
        let opts =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("project.tectora", opts).unwrap();
        zip.write_all(project.as_bytes()).unwrap();
        if !history.is_empty() {
            zip.start_file("history.jsonl", opts).unwrap();
            zip.write_all(history.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
        path
    }

    /// Audit 2026-11-22 ARCH-303, HUNT-301: a received job's stored result
    /// is bounded (200 000 line codes froze the editor and made every MCP
    /// read 23 MB) and its texts are cleaned; the change log's `changes`
    /// are what its cleaned versions differ in, not what the sender wrote
    /// (a null value crashed the app's change list, bidi text spoofed it).
    /// Audit 2026-11-23 ERR-1202, ARCH-1201: a bundle whose unknown keys
    /// nest deeper than the library takes is refused as `too_many`, with
    /// the reason, and adds no job; a key named `__proto__` comes in
    /// without it (ARCH-1202).
    #[test]
    fn a_bundle_with_too_deep_unknown_keys_is_refused() {
        let dir = std::env::temp_dir().join(format!("tectora-bundle-deep-{}", now_ms()));
        fs::create_dir_all(&dir).unwrap();
        let store = Store::new(dir.join("lib")).unwrap();
        let p = crate::examples::find("gable").unwrap().project();
        let mut v = serde_json::to_value(&p).unwrap();
        let mut deep = serde_json::json!([]);
        for _ in 0..crate::model::MAX_EXTRA_DEPTH {
            deep = serde_json::Value::Array(vec![deep]);
        }
        v["crm"] = deep;
        let path = bundle_with_log(&dir, "deep.tectorajob", &v.to_string(), "");
        let e = import(&store, &path).unwrap_err();
        assert_eq!(e.code, "too_many", "{e}");
        assert!(e.to_string().contains("nested too deep: 33 levels"), "{e}");
        assert!(store.list().is_empty());
        let mut v = serde_json::to_value(&p).unwrap();
        v["__proto__"] = serde_json::json!({ "flat_roof": true });
        let path = bundle_with_log(&dir, "proto.tectorajob", &v.to_string(), "");
        let id = import(&store, &path).unwrap();
        let got = store.load(&id).unwrap();
        assert!(got.extra.is_empty() && !got.flat_roof, "{:?}", got.extra);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_received_stored_result_and_change_list_are_bounded_and_cleaned() {
        let dir = std::env::temp_dir().join(format!("tectora-bundle-snap-{}", now_ms()));
        fs::create_dir_all(&dir).unwrap();
        let store = Store::new(dir.join("lib")).unwrap();
        let mut p = crate::examples::find("gable").unwrap().project();
        p.set_status(crate::model::JobStatus::Offer);
        let good = p.result_snapshot.clone().unwrap();
        // Too many line codes: refused with a message saying so.
        let mut big = p.clone();
        let snap = big.result_snapshot.as_mut().unwrap();
        for i in 0..200_000 {
            snap.lines_by_code.insert(format!("C{i:06}"), 1.0);
        }
        let path = bundle_with_log(&dir, "big.tectorajob", &big.to_json().unwrap(), "");
        let e = import(&store, &path).unwrap_err();
        assert!(e.to_string().contains("stored result"), "{e}");
        // A code that is no line code: refused as well.
        let mut odd = p.clone();
        odd.result_snapshot
            .as_mut()
            .unwrap()
            .lines_by_code
            .insert("\u{202e}EVIL\u{0}".to_string() + &"k".repeat(5000), 1.0);
        let path = bundle_with_log(&dir, "odd.tectorajob", &odd.to_json().unwrap(), "");
        let e = import(&store, &path).unwrap_err();
        assert!(e.to_string().contains("stored result"), "{e}");
        // The engine version is a text: cleaned and cut.
        let mut ver = p.clone();
        ver.result_snapshot.as_mut().unwrap().engine_version =
            "\u{202e}9.9.9\u{7}".to_string() + &"v".repeat(20_000);
        // The log's changes as a hostile sender wrote them.
        let mut renamed = ver.clone();
        renamed.name = "Renamed".into();
        let mut entry = serde_json::to_value(crate::store::ActivityEntry {
            ts_ms: 1,
            actor: "AI".into(),
            summary: "Renamed".into(),
            before: Some(ver.clone()),
            changes: Vec::new(),
            after: Some(renamed.clone()),
            undid_from: None,
            restores: None,
            reason: None,
        })
        .unwrap();
        entry["changes"] = serde_json::json!([
            { "kind": "modified", "field": "vertex", "part": 0, "edge": 1, "before": null, "after": null },
            { "kind": "modified", "field": "name", "before": "a", "after": "Faktura \u{202e}fdp.exe" },
            { "kind": "modified", "field": "job.customer", "before": "", "after": "x".repeat(100_000) },
        ]);
        let path = bundle_with_log(
            &dir,
            "ver.tectorajob",
            &renamed.to_json().unwrap(),
            &(entry.to_string() + "\n"),
        );
        let id = import(&store, &path).unwrap();
        let got = store.load(&id).unwrap();
        let v = &got.result_snapshot.as_ref().unwrap().engine_version;
        assert!(v.starts_with("9.9.9v"), "{v:?}");
        assert!(v.chars().count() <= crate::snapshot::MAX_ENGINE_VERSION_CHARS);
        assert_eq!(
            got.result_snapshot.as_ref().unwrap().roof_area,
            good.roof_area
        );
        let log = store.activity(&id);
        assert_eq!(log.len(), 1);
        let fields: Vec<&str> = log[0].changes.iter().map(|c| c.field.as_str()).collect();
        assert_eq!(fields, ["name"], "{:?}", log[0].changes);
        assert_eq!(log[0].changes[0].after, serde_json::json!("Renamed"));
        let _ = fs::remove_dir_all(dir);
    }

    /// ARCH-303: a job in the library whose stored result is over the bound
    /// does not open (MCP read it as 23 MB) and says why.
    #[test]
    fn a_library_job_with_an_oversized_stored_result_says_why_it_does_not_open() {
        let dir = std::env::temp_dir().join(format!("tectora-bundle-snaplib-{}", now_ms()));
        let store = Store::new(dir.clone()).unwrap();
        let mut p = crate::examples::find("gable").unwrap().project();
        p.set_status(crate::model::JobStatus::Offer);
        let snap = p.result_snapshot.as_mut().unwrap();
        for i in 0..1000 {
            snap.lines_by_code.insert(format!("C{i:03}"), 1.0);
        }
        fs::write(dir.join("big.tectora"), p.to_json().unwrap()).unwrap();
        let e = store.load("big").unwrap_err();
        assert_eq!(e.code(), "unreadable");
        assert!(e.to_string().contains("stored result"), "{e}");
        assert!(p.check_limits().unwrap_err().contains("stored result"));
        let _ = fs::remove_dir_all(dir);
    }

    /// A bundle with the gable example and these attached files: (name, size
    /// in MiB of spaces, count of such files).
    fn bundle_with(dir: &Path, files: &[(&str, usize, usize)]) -> std::path::PathBuf {
        let path = dir.join("big.tectorajob");
        let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
        let opts = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .large_file(true);
        let p = crate::examples::find("gable").unwrap().project();
        zip.start_file("project.tectora", opts).unwrap();
        zip.write_all(p.to_json().unwrap().as_bytes()).unwrap();
        let chunk = vec![b' '; 1 << 20];
        for (name, mib, count) in files {
            for i in 0..*count {
                zip.start_file(format!("files/{i}-{name}"), opts).unwrap();
                for _ in 0..*mib {
                    zip.write_all(&chunk).unwrap();
                }
            }
        }
        zip.finish().unwrap();
        path
    }

    #[test]
    fn a_bundle_over_the_size_or_count_limits_is_refused_and_leaves_nothing() {
        // ARCH-004: a small ZIP must not fill the disk.
        let dir = std::env::temp_dir().join(format!("tectora-bomb-{}", now_ms()));
        fs::create_dir_all(&dir).unwrap();
        let store = Store::new(dir.join("lib")).unwrap();
        let bomb = bundle_with(&dir, &[("plan.pdf", 200, 3)]);
        assert!(fs::metadata(&bomb).unwrap().len() < 4 << 20);
        let e = import(&store, &bomb).unwrap_err();
        assert!(
            e.detail.contains("too large") && e.code == "too_large",
            "{e}"
        );
        let many = bundle_with(&dir, &[("x.pdf", 0, MAX_FILES + 1)]);
        let e = import(&store, &many).unwrap_err();
        assert!(
            e.detail.contains("too many") && e.code == "too_large",
            "{e}"
        );
        assert!(store.list().is_empty());
        let ok = bundle_with(&dir, &[("x.pdf", 1, 2)]);
        let id = import(&store, &ok).unwrap();
        assert_eq!(store.attachments(&id).len(), 2);
        let _ = fs::remove_dir_all(dir);
    }

    /// A bundle with this manifest text and project text.
    fn bundle_of(dir: &Path, name: &str, manifest: &str, project: &str) -> PathBuf {
        let path = dir.join(name);
        let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
        let opts = SimpleFileOptions::default();
        zip.start_file("manifest.json", opts).unwrap();
        zip.write_all(manifest.as_bytes()).unwrap();
        zip.start_file("project.tectora", opts).unwrap();
        zip.write_all(project.as_bytes()).unwrap();
        zip.finish().unwrap();
        path
    }

    /// Audit 2026-11-22 HUNT-401: a received history version far out of
    /// the limits is dropped before its changes are worked out: a bundle of
    /// under a megabyte kept the import busy for minutes.
    #[test]
    fn a_history_version_out_of_the_limits_is_dropped_before_it_is_diffed() {
        let dir = std::env::temp_dir().join(format!("tectora-diffbomb-{}", now_ms()));
        fs::create_dir_all(&dir).unwrap();
        let p = crate::examples::find("gable").unwrap().project();
        let windows = |n: usize, k: f64| -> Vec<crate::model::RoofWindow> {
            (0..n)
                .map(|i| crate::model::RoofWindow {
                    x: i as f64 * 0.001 + k,
                    y: 1.0,
                    width: 0.7,
                    height: 1.1,
                })
                .collect()
        };
        let (mut before, mut after) = (p.clone(), p.clone());
        before.windows = windows(20_000, 0.0);
        after.windows = windows(25_000, 0.5);
        let entry = serde_json::to_string(&crate::store::ActivityEntry {
            ts_ms: 1,
            actor: "x".into(),
            summary: "s".into(),
            before: Some(before),
            changes: Vec::new(),
            after: Some(after),
            undid_from: None,
            restores: None,
            reason: None,
        })
        .unwrap();
        let path = dir.join("bomb.tectorajob");
        let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
        let opts = SimpleFileOptions::default();
        zip.start_file("project.tectora", opts).unwrap();
        zip.write_all(p.to_json().unwrap().as_bytes()).unwrap();
        zip.start_file("history.jsonl", opts).unwrap();
        zip.write_all(entry.as_bytes()).unwrap();
        zip.finish().unwrap();
        let store = Store::new(dir.join("lib")).unwrap();
        let started = std::time::Instant::now();
        HISTORY_DIFFS.with(|n| n.set(0));
        let id = import(&store, &path).unwrap();
        // Dropped before its changes were worked out, whatever the speed of
        // the machine (TEST-506 item 2).
        assert_eq!(HISTORY_DIFFS.with(|n| n.get()), 0, "the version was diffed");
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "the import took {:?}",
            started.elapsed()
        );
        assert!(store.activity(&id).is_empty());
        let _ = fs::remove_dir_all(dir);
    }

    /// Audit 2026-11-22 ARCH-503: a received history of more entries than
    /// a change log keeps is cut to the newest ones before it is cleaned
    /// and diffed: 20 000 entries froze the job's AI panel and made every
    /// MCP `get_history` megabytes.
    #[test]
    fn a_received_history_keeps_only_the_newest_entries_a_log_keeps() {
        let dir = std::env::temp_dir().join(format!("tectora-manyhist-{}", now_ms()));
        fs::create_dir_all(&dir).unwrap();
        let p = crate::examples::find("gable").unwrap().project();
        let history: String = (1..=1000u64)
            .map(|i| {
                let mut after = p.clone();
                after.notes = format!("note {i}");
                serde_json::to_string(&crate::store::ActivityEntry {
                    ts_ms: i,
                    actor: "x".into(),
                    summary: format!("change {i}"),
                    before: Some(p.clone()),
                    changes: Vec::new(),
                    after: Some(after),
                    undid_from: None,
                    restores: None,
                    reason: None,
                })
                .unwrap()
                    + "\n"
            })
            .collect();
        let path = bundle_with_log(&dir, "many.tectorajob", &p.to_json().unwrap(), &history);
        let store = Store::new(dir.join("lib")).unwrap();
        HISTORY_DIFFS.with(|n| n.set(0));
        let id = import(&store, &path).unwrap();
        assert_eq!(
            HISTORY_DIFFS.with(|n| n.get()),
            crate::store::ACTIVITY_KEEP,
            "only the kept entries are diffed"
        );
        let log = store.activity(&id);
        assert_eq!(log.len(), crate::store::ACTIVITY_KEEP);
        assert_eq!(log[0].summary, "change 951");
        assert_eq!(log.last().unwrap().summary, "change 1000");
        assert!(store.pending_review(&id).is_none(), "received, not pending");
        let _ = fs::remove_dir_all(dir);
    }

    /// Audit 2026-11-22 ARCH-501: a link left at the log path of the id a
    /// received job would get (pointing outside the library) is neither
    /// written through nor taken over: the job gets another id, and the
    /// link's target is never created.
    #[cfg(unix)]
    #[test]
    fn a_link_at_a_new_jobs_log_path_is_not_written_through_by_an_import() {
        let dir = std::env::temp_dir().join(format!("tectora-importlink-{}", now_ms()));
        fs::create_dir_all(&dir).unwrap();
        let mut p = crate::examples::find("gable").unwrap().project();
        p.name = "Odd".into();
        let entry = serde_json::to_string(&crate::store::ActivityEntry {
            ts_ms: 1,
            actor: "x".into(),
            summary: "House".into(),
            before: None,
            changes: Vec::new(),
            after: Some(p.clone()),
            undid_from: None,
            restores: None,
            reason: None,
        })
        .unwrap();
        let path = bundle_with_log(
            &dir,
            "odd.tectorajob",
            &p.to_json().unwrap(),
            &(entry + "\n"),
        );
        let store = Store::new(dir.join("lib")).unwrap();
        fs::create_dir_all(store.activity_dir()).unwrap();
        let target = dir.join("outside").join("created-by-import.jsonl");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&target, store.activity_dir().join("odd.jsonl")).unwrap();
        let id = import(&store, &path).unwrap();
        assert_ne!(id, "odd", "the id with a leftover link is taken");
        assert!(
            !target.exists(),
            "the history was written outside the library"
        );
        assert_eq!(store.activity(&id).len(), 1);
        let _ = fs::remove_dir_all(dir);
    }

    /// Audit 2026-11-23 ARCH-1401 (verifier's note): a received change log
    /// keeps the sender's `undid_from`, `restores` and `user` actor, so its
    /// reverts and put-backs keep their kinds; everything received is
    /// reviewed on import, so none of them waits for the user's review.
    #[test]
    fn a_received_log_keeps_its_reverts_and_put_backs() {
        use crate::store::{EntryKind, USER_ACTOR};
        let dir = std::env::temp_dir().join(format!("tectora-bundle-kinds-{}", now_ms()));
        let store = Store::new(dir.join("lib")).unwrap();
        let mut p = crate::examples::find("gable").unwrap().project();
        p.job.status = crate::model::JobStatus::Offer;
        p.result_snapshot = Some(crate::snapshot::take(&p));
        let id = store.save(None, &p).unwrap();
        let s = p.result_snapshot.as_mut().unwrap();
        s.engine_version = "0.0.1".into();
        s.roof_area += 2.0;
        fs::write(
            store.dir.join(format!("{id}.tectora")),
            p.to_json().unwrap(),
        )
        .unwrap();
        let origins = store.activity_dir().join(format!("{id}.result.json"));
        let text = fs::read_to_string(&origins)
            .unwrap()
            .replace(crate::snapshot::ENGINE_VERSION, "0.0.1");
        fs::write(&origins, text).unwrap();
        let done = store.refresh_entry(&id).unwrap().expect("recalculated");
        let mut q = store.load(&id).unwrap();
        q.notes = "AI note".into();
        store
            .save_as(Some(id.clone()), &q, "claude", "Note")
            .unwrap();
        let noted = store.activity(&id).last().unwrap().ts_ms;
        store.revert_last(&id, USER_ACTOR, None).unwrap();
        store
            .restore_offer_result(&id, done.entry_ts_ms, USER_ACTOR, None)
            .unwrap();
        let sent = store.activity(&id);
        let (revert, put_back) = (sent[sent.len() - 2].clone(), sent[sent.len() - 1].clone());
        assert_eq!(revert.undid_from, Some(noted));

        let bundle = dir.join("offer.tectorajob");
        export(&store, &id, &bundle).unwrap();
        let other = Store::new(dir.join("other")).unwrap();
        let got = import(&other, &bundle).unwrap();
        let log = other.activity(&got);
        let r = log
            .iter()
            .find(|e| e.ts_ms == revert.ts_ms)
            .expect("the revert");
        assert_eq!(r.undid_from, Some(noted));
        assert_eq!(r.actor, USER_ACTOR);
        assert_eq!(r.kind(), Some(EntryKind::Revert));
        let b = log
            .iter()
            .find(|e| e.ts_ms == put_back.ts_ms)
            .expect("the put-back");
        assert_eq!(b.restores, Some(done.entry_ts_ms));
        assert_eq!(b.actor, USER_ACTOR);
        assert_eq!(b.kind(), Some(EntryKind::Restore));
        let items = other.activity_items(&got);
        assert_eq!(items[0].kind, Some(EntryKind::Restore));
        assert_eq!(items[1].kind, Some(EntryKind::Revert));
        assert!(other.pending_review(&got).is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    /// Rewrites every line of `id`'s change log with `edit`, as a sender
    /// whose clock runs ahead (or another program) could have written it.
    fn edit_log(store: &Store, id: &str, edit: impl Fn(usize, &mut crate::store::ActivityEntry)) {
        let path = store.activity_path(id).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        let lines: Vec<String> = text
            .lines()
            .filter(|l| !l.is_empty())
            .enumerate()
            .map(|(k, l)| {
                let mut e: crate::store::ActivityEntry = serde_json::from_str(l).unwrap();
                edit(k, &mut e);
                serde_json::to_string(&e).unwrap()
            })
            .collect();
        fs::write(&path, lines.join("\n") + "\n").unwrap();
    }

    /// A sent job: created by the user, two AI changes (notes, customer),
    /// the user's revert of the second.
    fn sent_with_a_revert(store: &Store) -> String {
        use crate::store::USER_ACTOR;
        let p = crate::examples::find("gable").unwrap().project();
        let id = store.save_as(None, &p, USER_ACTOR, "Created").unwrap();
        let mut q = store.load(&id).unwrap();
        q.notes = "A1".into();
        store.save_as(Some(id.clone()), &q, "claude", "A1").unwrap();
        q.job.customer = "A2".into();
        store.save_as(Some(id.clone()), &q, "claude", "A2").unwrap();
        store.revert_last(&id, USER_ACTOR, None).unwrap();
        id
    }

    /// Audit 2026-11-23 HUNT-1501: a log received from a sender whose
    /// clock runs a day ahead is moved back with its links. Every revert
    /// and put-back names an entry of the imported log, and "revert the
    /// latest change" undoes the change before the reverted one.
    #[test]
    fn a_received_log_from_a_clock_ahead_keeps_its_links() {
        use crate::store::USER_ACTOR;
        let dir = std::env::temp_dir().join(format!("tectora-bundle-ahead-{}", now_ms()));
        let store = Store::new(dir.join("lib")).unwrap();
        let id = sent_with_a_revert(&store);
        const DAY: u64 = 24 * 3600 * 1000;
        edit_log(&store, &id, |_, e| {
            e.ts_ms += DAY;
            for link in [&mut e.undid_from, &mut e.restores].into_iter().flatten() {
                *link += DAY;
            }
        });
        let bundle = dir.join("ahead.tectorajob");
        export(&store, &id, &bundle).unwrap();
        let other = Store::new(dir.join("other")).unwrap();
        let got = import(&other, &bundle).unwrap();
        let log = other.activity(&got);
        let links: Vec<u64> = log
            .iter()
            .flat_map(|e| e.undid_from.into_iter().chain(e.restores))
            .collect();
        assert!(!links.is_empty(), "the revert came along");
        for ts in links {
            assert!(
                log.iter().any(|e| e.ts_ms == ts),
                "{ts} names no entry of {log:#?}"
            );
        }
        let p = other.revert_last(&got, USER_ACTOR, None).unwrap();
        assert_eq!((p.notes.as_str(), p.job.customer.as_str()), ("", ""));
        let _ = fs::remove_dir_all(&dir);
    }

    /// Audit 2026-11-24 HUNT-1602 (design C): one entry dated far ahead
    /// (2100) moved every other entry to 0. Now every time is distinct
    /// and increasing, every link names the entry it named before, and
    /// "revert the latest change" twice walks two steps back without ever
    /// putting a change back.
    #[test]
    fn a_received_log_with_one_entry_far_ahead_keeps_distinct_times() {
        use crate::store::USER_ACTOR;
        let dir = std::env::temp_dir().join(format!("tectora-bundle-far-{}", now_ms()));
        let store = Store::new(dir.join("lib")).unwrap();
        let id = sent_with_a_revert(&store);
        // A3 after the revert of A2, dated 2100-01-01.
        let mut q = store.load(&id).unwrap();
        q.job.address = "A3".into();
        store.save_as(Some(id.clone()), &q, "claude", "A3").unwrap();
        let sent = store.activity(&id);
        edit_log(&store, &id, |k, e| {
            if k == sent.len() - 1 {
                e.ts_ms = 4_102_444_800_000;
            }
        });
        let named = |log: &[crate::store::ActivityEntry]| -> Vec<Option<String>> {
            log.iter()
                .map(|e| {
                    e.undid_from
                        .map(|ts| log.iter().rfind(|x| x.ts_ms == ts).unwrap().summary.clone())
                })
                .collect()
        };
        let before = named(&store.activity(&id));
        let bundle = dir.join("far.tectorajob");
        export(&store, &id, &bundle).unwrap();
        let other = Store::new(dir.join("other")).unwrap();
        let got = import(&other, &bundle).unwrap();
        let log = other.activity(&got);
        assert_eq!(log.len(), sent.len(), "nothing dropped");
        assert!(log.windows(2).all(|w| w[0].ts_ms < w[1].ts_ms), "{log:#?}");
        assert!(log.last().unwrap().ts_ms <= now_ms() + log.len() as u64);
        assert_eq!(named(&log), before, "links name the same entries");
        // A3 out, then A1 out (A2 stays out): never a change put back.
        let p = other.revert_last(&got, USER_ACTOR, None).unwrap();
        assert_eq!(
            (p.notes.as_str(), p.job.customer.as_str(), p.job.address.as_str()),
            ("A1", "", "")
        );
        let p = other.revert_last(&got, USER_ACTOR, None).unwrap();
        assert_eq!(
            (p.notes.as_str(), p.job.customer.as_str(), p.job.address.as_str()),
            ("", "", "")
        );
        let _ = fs::remove_dir_all(&dir);
    }

    /// Audit 2026-11-23 DANGLING (design B2): a received revert whose
    /// reverted version is outside the limits keeps its link, which then
    /// names no entry of the imported log; it is read by time, and the
    /// change before it can still be reverted.
    #[test]
    fn a_received_revert_whose_version_was_left_out_still_leaves_the_change_before_it() {
        use crate::store::USER_ACTOR;
        let dir = std::env::temp_dir().join(format!("tectora-bundle-dangling-{}", now_ms()));
        let store = Store::new(dir.join("lib")).unwrap();
        let id = sent_with_a_revert(&store);
        // The second AI change's version is out of the limits.
        edit_log(&store, &id, |k, e| {
            if k == 2 {
                e.after.as_mut().unwrap().parts[0].edges[1].overhang = 1e300;
            }
        });
        let bundle = dir.join("dangling.tectorajob");
        export(&store, &id, &bundle).unwrap();
        let other = Store::new(dir.join("other")).unwrap();
        let got = import(&other, &bundle).unwrap();
        let log = other.activity(&got);
        assert!(!log.iter().any(|e| e.summary == "A2"), "left out: {log:#?}");
        let revert = log.last().unwrap();
        assert!(revert.undid_from.is_some(), "kept as a revert");
        let p = other.revert_last(&got, USER_ACTOR, None).unwrap();
        assert_eq!(p.notes, "");
        let _ = fs::remove_dir_all(&dir);
    }

    /// Audit 2026-11-22 ARCH-802: a change-log entry that came with a
    /// bundle under the engine's name (a real one of the sender's Tectora,
    /// or a forged one) is history under another name: never shown as this
    /// engine's recalculation, and with nothing to put back.
    #[test]
    fn a_received_engine_entry_is_never_this_engines_recalculation() {
        use crate::store::{ENGINE_ACTOR, RECEIVED_ENGINE_ACTOR, USER_ACTOR};
        let dir = std::env::temp_dir().join(format!("tectora-bundle-engine-{}", now_ms()));
        let store = Store::new(dir.join("lib")).unwrap();
        // An offer an older engine stored, recalculated on open.
        let mut p = crate::examples::find("gable").unwrap().project();
        p.job.status = crate::model::JobStatus::Offer;
        p.result_snapshot = Some(crate::snapshot::take(&p));
        let id = store.save(None, &p).unwrap();
        let s = p.result_snapshot.as_mut().unwrap();
        s.engine_version = "0.0.1".into();
        s.roof_area += 2.0;
        fs::write(
            store.dir.join(format!("{id}.tectora")),
            p.to_json().unwrap(),
        )
        .unwrap();
        let origins = store.activity_dir().join(format!("{id}.result.json"));
        let text = fs::read_to_string(&origins)
            .unwrap()
            .replace(crate::snapshot::ENGINE_VERSION, "0.0.1");
        fs::write(&origins, text).unwrap();
        let done = store.refresh_entry(&id).unwrap().expect("recalculated");
        assert!(store.recalculation(&id).is_some());

        let bundle = dir.join("offer.tectorajob");
        export(&store, &id, &bundle).unwrap();
        let other = Store::new(dir.join("other")).unwrap();
        let got = import(&other, &bundle).unwrap();
        let log = other.activity(&got);
        assert!(!log.is_empty());
        assert!(log.iter().all(|e| e.actor != ENGINE_ACTOR), "{log:?}");
        let entry = log.iter().find(|e| e.ts_ms == done.entry_ts_ms).unwrap();
        assert_eq!(entry.actor, RECEIVED_ENGINE_ACTOR);
        assert!(
            other
                .activity_items(&got)
                .iter()
                .all(|a| a.refreshed.is_none())
        );
        assert_eq!(other.recalculation(&got), None);
        let e = other
            .restore_offer_result(&got, done.entry_ts_ms, USER_ACTOR, None)
            .unwrap_err();
        assert_eq!(e.code(), "revert_stale");
        // In any spelling of the name.
        let line = |actor: &str| {
            serde_json::to_string(&crate::store::ActivityEntry {
                ts_ms: 5,
                actor: actor.into(),
                summary: "x".into(),
                changes: Vec::new(),
                before: None,
                after: None,
                undid_from: None,
                restores: None,
                reason: None,
            })
            .unwrap()
        };
        for actor in [
            "tectora",
            "Tectora",
            " TECTORA ",
            "tectora (received)",
            " Tectora (Received) ",
        ] {
            let (text, _) = received_history(line(actor).as_bytes()).unwrap();
            let e: crate::store::ActivityEntry = serde_json::from_str(text.trim()).unwrap();
            assert_eq!(e.actor, RECEIVED_ENGINE_ACTOR, "{actor:?}");
        }
        // The sender's own entries stay the user's, in any spelling as the
        // one name (audit 2026-11-23 TEST-1006).
        for actor in ["user", " User "] {
            let (text, _) = received_history(line(actor).as_bytes()).unwrap();
            let e: crate::store::ActivityEntry = serde_json::from_str(text.trim()).unwrap();
            assert_eq!(e.actor.trim().to_ascii_lowercase(), USER_ACTOR, "{actor:?}");
            assert_ne!(e.actor, RECEIVED_ENGINE_ACTOR, "{actor:?}");
        }
        // A received reason is held to what a write keeps: one line,
        // no control or bidi characters, at most the summary cap.
        let raw = format!(
            "a\u{202e}b\n{}",
            "x".repeat(crate::edit::MAX_SUMMARY_CHARS + 10)
        );
        let mut with_reason: serde_json::Value = serde_json::from_str(&line("claude")).unwrap();
        with_reason["reason"] = serde_json::json!(raw);
        let (text, _) = received_history(with_reason.to_string().as_bytes()).unwrap();
        let e: crate::store::ActivityEntry = serde_json::from_str(text.trim()).unwrap();
        let r = e.reason.expect("the reason is kept");
        assert!(r.starts_with('a') && r.contains('b'), "{r:?}");
        assert!(
            !r.chars()
                .any(|c| c.is_control() || ('\u{202a}'..='\u{202e}').contains(&c)),
            "{r:?}"
        );
        assert!(
            r.chars().count() <= crate::edit::MAX_SUMMARY_CHARS,
            "{}",
            r.len()
        );
        let _ = fs::remove_dir_all(dir);
    }

    /// Audit 2026-11-22 HUNT-406: the bundle format version is read as a
    /// project's schema version is: `2.0` and `1e30` are a newer Tectora,
    /// `1.0` is this one, and a string is a plain refusal, never a raw
    /// parser text.
    #[test]
    fn a_bundle_format_version_written_as_a_float_or_a_string_is_understood() {
        let dir = std::env::temp_dir().join(format!("tectora-bundle-version-{}", now_ms()));
        fs::create_dir_all(&dir).unwrap();
        let store = Store::new(dir.join("lib")).unwrap();
        let project = crate::examples::find("gable")
            .unwrap()
            .project()
            .to_json()
            .unwrap();
        let manifest = |v: &str| {
            format!(
                r#"{{"format": "tectora-job", "format_version": {v}, "name": "x", "engine_version": "1", "exported_ms": 1, "files": []}}"#
            )
        };
        for (i, (v, code)) in [
            ("2.0", "newer_format"),
            ("1e30", "newer_format"),
            ("\"2\"", "unreadable"),
            ("1.5", "unreadable"),
            ("-1", "unreadable"),
        ]
        .into_iter()
        .enumerate()
        {
            let b = bundle_of(&dir, &format!("v{i}.tectorajob"), &manifest(v), &project);
            let e = import(&store, &b).unwrap_err();
            assert_eq!(e.code, code, "{v}: {e}");
            assert!(!e.detail.contains("invalid type"), "{v}: {}", e.detail);
            assert!(!e.detail.contains("expected"), "{v}: {}", e.detail);
        }
        let b = bundle_of(&dir, "v1.tectorajob", &manifest("1.0"), &project);
        assert!(import(&store, &b).is_ok());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_bundle_from_a_newer_tectora_says_so_even_when_its_manifest_changed() {
        // ERR-007: a newer manifest that the current struct cannot parse
        // was called damaged.
        let dir = std::env::temp_dir().join(format!("tectora-newer-bundle-{}", now_ms()));
        fs::create_dir_all(&dir).unwrap();
        let store = Store::new(dir.join("lib")).unwrap();
        let p = crate::examples::find("gable").unwrap().project();
        let project = p.to_json().unwrap();
        let newer = bundle_of(
            &dir,
            "newer.tectorajob",
            r#"{"format": "tectora-job", "format_version": 2, "name": "x", "app": {"version": "9.0"}}"#,
            &project,
        );
        let e = import(&store, &newer).unwrap_err();
        assert_eq!(e.code, "newer_format", "{e}");
        // An older manifest that lacks a field, with a project from a newer
        // Tectora: the project's version is what counts.
        let mut v: serde_json::Value = serde_json::from_str(&project).unwrap();
        v["schema_version"] = 99.into();
        v["future"] = serde_json::json!({ "x": 1 });
        let short = r#"{"format": "tectora-job", "format_version": 1, "name": "x"}"#;
        let newer_schema = bundle_of(&dir, "newer-schema.tectorajob", short, &v.to_string());
        let e = import(&store, &newer_schema).unwrap_err();
        assert_eq!(e.code, "newer_format", "{e}");
        // The same manifest with a current project is damaged.
        let damaged = bundle_of(&dir, "damaged.tectorajob", short, &project);
        assert_eq!(import(&store, &damaged).unwrap_err().code, "unreadable");
        let not_json = bundle_of(&dir, "garbage.tectorajob", "{ x", &project);
        assert_eq!(import(&store, &not_json).unwrap_err().code, "unreadable");
        let other = bundle_of(
            &dir,
            "other.tectorajob",
            r#"{"format": "something-else", "format_version": 7}"#,
            &project,
        );
        let e = import(&store, &other).unwrap_err();
        assert!(
            e.code == "unreadable" && e.detail.contains("not a Tectora job"),
            "{e}"
        );
        assert!(store.list().is_empty());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_bundle_cannot_name_a_file_outside_the_library() {
        // ARCH-039: on Windows `files_dir.join("C:x.dll")` is `C:x.dll`.
        let dir = std::env::temp_dir().join(format!("tectora-names-{}", now_ms()));
        fs::create_dir_all(&dir).unwrap();
        let store = Store::new(dir.join("lib")).unwrap();
        for name in ["C:version.dll", "plan.pdf:hidden"] {
            let b = bundle_with(&dir, &[(name, 0, 1)]);
            let e = import(&store, &b).unwrap_err();
            assert!(e.detail.contains("invalid file name"), "{name}: {e}");
            assert!(store.list().is_empty(), "{name}: nothing kept");
        }
        for name in ["nul", "NUL.pdf", "com1.txt", "a:b", "..", "x/y"] {
            assert!(export_path(&store, name).is_err(), "{name}");
        }
        assert!(export_path(&store, "Novák střecha").is_ok());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_refused_bundle_leaves_no_lock_file_behind() {
        // ARCH-008: every bundle refused after the job was written left
        // `.activity/<id>.lock`.
        let dir = std::env::temp_dir().join(format!("tectora-refused-lock-{}", now_ms()));
        fs::create_dir_all(&dir).unwrap();
        let store = Store::new(dir.join("lib")).unwrap();
        // A good file, then one Windows reserves: refused after the job and
        // the first file were written.
        let b2 = bundle_with(&dir, &[("x.pdf", 0, 1)]);
        let mut zip = zip::ZipWriter::new_append(
            fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&b2)
                .unwrap(),
        )
        .unwrap();
        zip.start_file("files/NUL.pdf", SimpleFileOptions::default())
            .unwrap();
        zip.finish().unwrap();
        let e = import(&store, &b2).unwrap_err();
        assert!(e.detail.contains("invalid file name"), "{e}");
        let left: Vec<String> = fs::read_dir(store.activity_dir())
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| !n.starts_with('.'))
            .collect();
        assert!(left.is_empty(), "{left:?}");
        assert!(store.list().is_empty());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn one_bundle_imported_twice_at_once_gives_two_jobs() {
        // TEST-001 for imports.
        let dir = std::env::temp_dir().join(format!("tectora-import-race-{}", now_ms()));
        fs::create_dir_all(&dir).unwrap();
        let store = std::sync::Arc::new(Store::new(dir.join("lib")).unwrap());
        let b = bundle_with(&dir, &[("x.pdf", 0, 1)]);
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let ids: Vec<String> = (0..2)
            .map(|_| {
                let (s, b, bar) = (store.clone(), b.clone(), barrier.clone());
                std::thread::spawn(move || {
                    bar.wait();
                    import(&s, &b).unwrap()
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|h| h.join().unwrap())
            .collect();
        assert_ne!(ids[0], ids[1]);
        assert_eq!(store.list().len(), 2);
        for id in &ids {
            assert_eq!(store.attachments(id).len(), 1, "{id}");
        }
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_bundle_with_coordinates_out_of_range_is_refused() {
        let dir = std::env::temp_dir().join(format!("tectora-range-{}", now_ms()));
        let store = Store::new(dir.join("lib")).unwrap();
        let mut p = crate::examples::find("gable").unwrap().project();
        let id = store.save(None, &p).unwrap();
        p.parts[0].edges[0].start.x = 1e300;
        fs::write(store.path(&id).unwrap(), serde_json::to_string(&p).unwrap()).unwrap();
        let file = dir.join("far.tectorajob");
        export(&store, &id, &file).unwrap();
        let mut zip = zip::ZipArchive::new(fs::File::open(&file).unwrap()).unwrap();
        assert!(zip.by_name("project.tectora").is_ok());
        assert!(zip.by_name("preview.svg").is_err(), "no kernel run");
        let other = Store::new(dir.join("other")).unwrap();
        assert!(import(&other, &file).is_err());
        assert!(other.list().is_empty());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn opening_a_library_file_keeps_it_and_an_outside_one_is_added() {
        let dir = std::env::temp_dir().join(format!("tectora-open-{}", now_ms()));
        let store = Store::new(dir.join("lib")).unwrap();
        let p = crate::examples::find("gable").unwrap().project();
        let id = store.save(None, &p).unwrap();
        assert_eq!(open_path(&store, &store.path(&id).unwrap()).unwrap(), id);
        let outside = dir.join("from-mail.tectora");
        fs::write(&outside, p.to_json().unwrap()).unwrap();
        let new_id = open_path(&store, &outside).unwrap();
        assert_ne!(new_id, id);
        assert_eq!(store.load(&new_id).unwrap(), p);
        let _ = fs::remove_dir_all(dir);
    }

    /// Audit 2026-11-21 HUNTIO-002: an export over the job's own
    /// `.tectora` turned the job into a ZIP nothing could open.
    #[test]
    fn an_export_writes_only_a_tectorajob_outside_the_library_folders() {
        let dir = std::env::temp_dir().join(format!("tectora-dest-{}", now_ms()));
        let store = Store::new(dir.join("lib")).unwrap();
        let p = crate::examples::find("gable").unwrap().project();
        let id = store.save(None, &p).unwrap();
        let own = store.path(&id).unwrap();
        let before = fs::read(&own).unwrap();
        fs::create_dir_all(dir.join("out")).unwrap();
        for dest in [
            own.clone(),
            dir.join("out").join("noext"),
            dir.join("out").join("job.zip"),
            dir.join("out").join(".tectorajob"),
            store.files_dir(&id).unwrap().join("x.tectorajob"),
            store.activity_dir().join("x.tectorajob"),
            store.trash_dir().join("x.tectorajob"),
        ] {
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            let e = export(&store, &id, &dest).unwrap_err();
            assert_eq!(e.code, "invalid_name", "{}: {e}", dest.display());
        }
        assert_eq!(fs::read(&own).unwrap(), before, "the job file is kept");
        assert_eq!(store.load(&id).unwrap(), p);
        assert!(!dir.join("out").join("noext").exists());
        // A path typed without the ending gets it, also after `.tectora`.
        assert_eq!(
            with_extension(&own),
            own.with_file_name(format!("{id}.tectora.tectorajob"))
        );
        let upper = dir.join("out").join("Job.TECTORAJOB");
        assert_eq!(with_extension(&upper), upper);
        export(&store, &id, &with_extension(&own)).unwrap();
        export(&store, &id, &upper).unwrap();
        // The library's exports folder, where the MCP server writes.
        export(&store, &id, &export_path(&store, "x").unwrap()).unwrap();
        assert_eq!(fs::read(&own).unwrap(), before);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_name_typed_with_trailing_spaces_or_dots_gets_one_ending() {
        // Round 2 ARCH-106 item 2: "a b.tectorajob " became
        // "a b.tectorajob .tectorajob"; Windows drops trailing spaces and
        // dots from file names, so the name is taken without them.
        let dir = Path::new("out");
        for typed in ["a b.tectorajob ", "a b.tectorajob. ", "a b ", "a b"] {
            assert_eq!(
                with_extension(&dir.join(typed)),
                dir.join("a b.tectorajob"),
                "{typed:?}"
            );
        }
        assert_eq!(
            with_extension(&dir.join("a.TectoraJob  ")),
            dir.join("a.TectoraJob")
        );
    }

    /// Audit 2026-11-21 HUNT-204 item 1, ARCH-205 item 2: an export name
    /// with trailing dots gets one ending, as the CLI's, and a name of only
    /// spaces and dots is refused.
    #[test]
    fn export_names_lose_trailing_dots_and_blank_names_are_refused() {
        let dir = std::env::temp_dir().join(format!("tectora-export-dots-{}", now_ms()));
        let store = Store::new(dir.join("lib")).unwrap();
        let name = |n: &str| {
            export_path(&store, n).map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        };
        assert_eq!(name("a.").unwrap(), "a.tectorajob");
        assert_eq!(name("x.tectorajob..").unwrap(), "x.tectorajob");
        assert_eq!(name("x.TECTORAJOB").unwrap(), "x.tectorajob");
        assert_eq!(name("a b ").unwrap(), "a b.tectorajob");
        for blank in [" . .", "...", ".tectorajob", ""] {
            assert_eq!(name(blank).unwrap_err().code, "invalid_name", "{blank:?}");
        }
        let id = store
            .save(None, &crate::examples::find("hip").unwrap().project())
            .unwrap();
        let typed = with_extension(&dir.join(" . ."));
        assert_eq!(
            export(&store, &id, &typed).unwrap_err().code,
            "invalid_name"
        );
        assert!(!typed.exists());
        let _ = fs::remove_dir_all(dir);
    }

    /// Audit 2026-11-21 HUNTIO-001: a job holding a file import refuses
    /// (attached by an older Tectora) is not exported to a bundle nobody
    /// can import.
    #[test]
    fn a_job_with_a_file_import_refuses_is_not_exported() {
        let dir = std::env::temp_dir().join(format!("tectora-refused-type-{}", now_ms()));
        let store = Store::new(dir.join("lib")).unwrap();
        let p = crate::examples::find("gable").unwrap().project();
        let id = store.save(None, &p).unwrap();
        store.add_attachment(&id, "plan.pdf", b"%PDF").unwrap();
        let files = store.files_dir(&id).unwrap();
        fs::write(files.join("Zpráva od zákazníka.msg"), b"x").unwrap();
        let file = dir.join("job.tectorajob");
        let e = export(&store, &id, &file).unwrap_err();
        assert_eq!(e.code, "not_attachable", "{e}");
        assert!(e.detail.contains("Zpráva od zákazníka.msg"), "{e}");
        assert!(!file.exists());
        fs::remove_file(files.join("Zpráva od zákazníka.msg")).unwrap();
        export(&store, &id, &file).unwrap();
        let other = Store::new(dir.join("other")).unwrap();
        assert!(import(&other, &file).is_ok());
        let _ = fs::remove_dir_all(dir);
    }

    /// Audit 2026-11-21 ARCH-002: a named pipe called `*.tectorajob` (or
    /// `*.tectora`) blocked the import, and the MCP server with it.
    #[cfg(unix)]
    #[test]
    fn importing_a_named_pipe_is_refused_without_waiting() {
        let dir = std::env::temp_dir().join(format!("tectora-fifo-{}", now_ms()));
        let store = Store::new(dir.join("lib")).unwrap();
        let mut pipes = Vec::new();
        for name in ["pipe.tectorajob", "pipe.tectora"] {
            let pipe = dir.join(name);
            let made = std::process::Command::new("mkfifo")
                .arg(&pipe)
                .status()
                .is_ok_and(|s| s.success());
            if !made {
                eprintln!("mkfifo not available, skipped");
                return;
            }
            pipes.push(pipe);
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let lib = store.dir.clone();
        std::thread::spawn(move || {
            let store = Store::at(lib);
            let codes: Vec<_> = pipes
                .iter()
                .map(|p| open_path(&store, p).map_err(|e| e.code))
                .collect();
            let _ = tx.send(codes);
        });
        let codes = rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("refused without waiting for a writer");
        assert_eq!(codes, [Err("not_a_file"), Err("not_a_file")]);
        assert!(store.list().is_empty());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn preview_shows_outline_and_roof_lines() {
        let svg = preview_svg(&crate::examples::find("hip").unwrap().project());
        assert!(svg.contains("<polygon"));
        assert!(svg.contains("<line"));
    }
}
