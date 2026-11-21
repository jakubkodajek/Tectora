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
/// `too_many` when it holds what no real roof has (ARCH-002).
fn parse_project(text: &str, what: &str) -> Result<Project, FileError> {
    if let Some(version) = crate::store::newer_format(text) {
        return Err(FileError::new(
            "newer_format",
            format!(
                "{what}: saved by a newer Tectora (format {version}, this version reads up to {})",
                crate::model::SCHEMA_VERSION
            ),
        ));
    }
    let project = Project::from_json(text)
        .map_err(|e| FileError::new("unreadable", format!("{what}: {e}")))?;
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
    let bad = name.len() > 100 || !crate::store::valid_file_name(&format!("{name}.{ext}"));
    if bad {
        return Err(FileError::new(
            "invalid_name",
            format!("`{name}` is not a plain file name"),
        ));
    }
    let dir = store.dir.join(EXPORTS_DIR);
    fs::create_dir_all(&dir).map_err(|e| FileError::io(&dir, e))?;
    for n in 1..1000 {
        let file = if n == 1 {
            format!("{name}.{ext}")
        } else {
            format!("{name} ({n}).{ext}")
        };
        let path = dir.join(file);
        if !path.exists() {
            return Ok(path);
        }
    }
    Err(FileError::new(
        "invalid_name",
        format!("too many exports named `{name}`"),
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
    let attachments = store.attachments(id);
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

/// `path` with `.tectorajob` added when it does not end in it (in any
/// case): `job.tectora` becomes `job.tectora.tectorajob`, never the job
/// file itself. For the CLI and the app's save dialog.
pub fn with_extension(path: &Path) -> PathBuf {
    // Trailing spaces and dots are no part of a name on Windows, which
    // drops them (round 2 ARCH-106): "a.tectorajob " is "a.tectorajob".
    let trimmed = path
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.trim_end_matches([' ', '.']))
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
    let stem = name.len().saturating_sub(EXTENSION.len() + 1);
    if !has_ext || stem == 0 || !crate::store::valid_file_name(name) {
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
    let file = fs::File::create(part).map_err(|e| FileError::io(dest, e))?;
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
    put(
        "project.tectora",
        project.to_json().map_err(json_err)?.as_bytes(),
    )?;
    if let Ok(history) = fs::read(store.activity_path(id)?) {
        put("history.jsonl", &history)?;
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
        let bytes = fs::read(&path).map_err(|e| FileError::io(&path, e))?;
        put(&format!("files/{}", a.name), &bytes)?;
    }
    zip.finish().map_err(write_err)?;
    Ok(manifest)
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
    if let Some(m) = &manifest {
        // The format and its version first: a newer Tectora may have
        // changed the rest of the manifest (ERR-007).
        let v: serde_json::Value = serde_json::from_slice(m).map_err(zip_err)?;
        if v.get("format").and_then(serde_json::Value::as_str) != Some(FORMAT) {
            return Err(zip_err("not a Tectora job"));
        }
        if let Some(version) = v
            .get("format_version")
            .and_then(serde_json::Value::as_u64)
            .filter(|&n| n > u64::from(FORMAT_VERSION))
        {
            return Err(FileError::new(
                "newer_format",
                format!(
                    "job bundle: made by a newer Tectora (bundle format {version}, this version reads up to {FORMAT_VERSION})"
                ),
            ));
        }
    }
    let text = read_entry(&mut zip, "project.tectora", MAX_TEXT_ENTRY)?
        .ok_or_else(|| zip_err("no project in the file"))?;
    // A project from a newer Tectora is `newer_format` whatever the
    // manifest says.
    let project = parse_project(
        &String::from_utf8(text).map_err(zip_err)?,
        "job bundle: project.tectora",
    )?;
    if let Some(m) = &manifest {
        serde_json::from_slice::<Manifest>(m).map_err(zip_err)?;
    }
    let history = read_entry(&mut zip, "history.jsonl", MAX_TEXT_ENTRY)?;
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
            fs::create_dir_all(&dir).map_err(|e| FileError::io(&dir, e))?;
            fs::write(&path, history).map_err(|e| FileError::io(&path, e))?;
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

/// The change log of a received bundle as it is written into the library,
/// with the time of its last entry. Entries that do not read as a change,
/// or whose versions are outside the limits a job file is held to, are
/// left out: a revert writes such a version into the job unchecked
/// (HUNTIO-004). Times ahead of this computer's clock are moved back, the
/// order kept, so the next change is later than all of them (HUNTIO-005).
fn received_history(bytes: &[u8]) -> Option<(String, u64)> {
    let ok = |p: &Option<Project>| p.as_ref().is_none_or(|p| p.check_limits().is_ok());
    let mut entries: Vec<crate::store::ActivityEntry> = bytes
        .split(|&b| b == b'\n')
        .filter_map(|l| serde_json::from_slice(l).ok())
        .filter(|e: &crate::store::ActivityEntry| ok(&e.before) && ok(&e.after))
        .collect();
    let last = entries.iter().map(|e| e.ts_ms).max()?;
    let now = now_ms();
    if last > now {
        let shift = last - now;
        for e in &mut entries {
            e.ts_ms = e.ts_ms.saturating_sub(shift);
        }
    }
    let text: String = entries
        .iter()
        .filter_map(|e| serde_json::to_string(e).ok())
        .map(|l| l + "\n")
        .collect();
    Some((text, last.min(now)))
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
    let mut text = String::new();
    crate::store::open_regular(path, MAX_TEXT_ENTRY)?
        .take(MAX_TEXT_ENTRY)
        .read_to_string(&mut text)
        .map_err(|e| FileError::io(path, e))?;
    let project = parse_project(&text, &path.display().to_string())?;
    Ok(store.save(None, &project)?)
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
        match open_path(store, &p) {
            Ok(id) => out.last = Some(id),
            Err(e) => out.failed.push((p, e)),
        }
    }
    out
}

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
