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

/// Writes job `id` of `store` to `dest` as a bundle.
pub fn export(store: &Store, id: &str, dest: &Path) -> Result<Manifest, FileError> {
    let project = store.load(id)?;
    let attachments = store.attachments(id);
    let manifest = Manifest {
        format: FORMAT.into(),
        format_version: FORMAT_VERSION,
        name: project.name.clone(),
        engine_version: crate::snapshot::ENGINE_VERSION.into(),
        exported_ms: now_ms(),
        files: attachments.iter().map(|a| a.name.clone()).collect(),
    };
    let file = fs::File::create(dest).map_err(|e| FileError::io(dest, e))?;
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
    // The preview runs the roof kernel: a job out of range or one the
    // kernel cannot calculate is still exported, without preview (ERR-008).
    let svg = project
        .check_limits()
        .ok()
        .and_then(|()| crate::model::guarded(|| preview_svg(&project)).ok());
    if let Some(svg) = svg {
        put("preview.svg", svg.as_bytes())?;
    }
    for a in &attachments {
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
pub fn import(store: &Store, src: &Path) -> Result<String, FileError> {
    let file = fs::File::open(src).map_err(|e| FileError::io(src, e))?;
    let mut zip = zip::ZipArchive::new(file).map_err(zip_err)?;
    if let Some(m) = read_entry(&mut zip, "manifest.json", MAX_TEXT_ENTRY)? {
        let m: Manifest = serde_json::from_slice(&m).map_err(zip_err)?;
        if m.format != FORMAT {
            return Err(zip_err("not a Tectora job"));
        }
        if m.format_version > FORMAT_VERSION {
            return Err(FileError::new(
                "newer_format",
                format!(
                    "job bundle: made by a newer Tectora (bundle format {}, this version reads up to {FORMAT_VERSION})",
                    m.format_version
                ),
            ));
        }
    }
    let text = read_entry(&mut zip, "project.tectora", MAX_TEXT_ENTRY)?
        .ok_or_else(|| zip_err("no project in the file"))?;
    let project = parse_project(
        &String::from_utf8(text).map_err(zip_err)?,
        "job bundle: project.tectora",
    )?;
    let history = read_entry(&mut zip, "history.jsonl", MAX_TEXT_ENTRY)?;
    let names: Vec<String> = zip
        .file_names()
        .filter_map(|n| n.ok())
        .filter_map(|n| n.strip_prefix("files/").map(str::to_string))
        .filter(|n| !n.is_empty() && !n.contains('/'))
        .collect();
    // Checked before anything is written, so a refused bundle leaves no
    // half-imported job behind.
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
        if let Some(history) = history {
            let path = store.activity_path(&id)?;
            let dir = store.activity_dir();
            fs::create_dir_all(&dir).map_err(|e| FileError::io(&dir, e))?;
            fs::write(&path, history).map_err(|e| FileError::io(&path, e))?;
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
    let meta = fs::metadata(path).map_err(|e| FileError::io(path, e))?;
    if !meta.is_file() {
        return Err(FileError::new(
            "not_a_file",
            format!("{}: not a regular file", path.display()),
        ));
    }
    if meta.len() > MAX_TEXT_ENTRY {
        return Err(FileError::new(
            "too_large",
            format!(
                "{}: larger than {} MB",
                path.display(),
                MAX_TEXT_ENTRY >> 20
            ),
        ));
    }
    let text = fs::read_to_string(path).map_err(|e| FileError::io(path, e))?;
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

    #[test]
    fn preview_shows_outline_and_roof_lines() {
        let svg = preview_svg(&crate::examples::find("hip").unwrap().project());
        assert!(svg.contains("<polygon"));
        assert!(svg.contains("<line"));
    }
}
