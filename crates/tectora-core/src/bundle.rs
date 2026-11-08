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
use std::path::Path;

use serde::{Deserialize, Serialize};
use zip::write::SimpleFileOptions;

use crate::model::Project;
use crate::store::{Store, now_ms, outlines, roof_lines};

pub const EXTENSION: &str = "tectorajob";
const FORMAT: &str = "tectora-job";
const FORMAT_VERSION: u32 = 1;
/// Largest attached file read from a bundle, bytes.
const MAX_ENTRY: u64 = 256 * 1024 * 1024;
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

fn zip_err(e: impl std::fmt::Display) -> String {
    format!("job bundle: {e}")
}

/// Folder of the library where the MCP server writes bundles.
pub const EXPORTS_DIR: &str = "exports";

/// A new file `<library>/exports/<name>.tectorajob` (with " (2)" etc. when
/// the name is taken): the only place an AI client may write a bundle, so it
/// can never replace a file of the user's. `name` must be a plain file name.
pub fn export_path(store: &Store, name: &str) -> Result<std::path::PathBuf, String> {
    let bad = name.is_empty()
        || name.len() > 100
        || name.starts_with('.')
        || name.ends_with('.')
        || name.ends_with(' ')
        || name.chars().any(|c| {
            c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
        });
    if bad {
        return Err(format!("`{name}` is not a plain file name"));
    }
    let dir = store.dir.join(EXPORTS_DIR);
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for n in 1..1000 {
        let file = if n == 1 {
            format!("{name}.{EXTENSION}")
        } else {
            format!("{name} ({n}).{EXTENSION}")
        };
        let path = dir.join(file);
        if !path.exists() {
            return Ok(path);
        }
    }
    Err(format!("too many exports named `{name}`"))
}

/// Writes job `id` of `store` to `dest` as a bundle.
pub fn export(store: &Store, id: &str, dest: &Path) -> Result<Manifest, String> {
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
    let file = fs::File::create(dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut put = |name: &str, bytes: &[u8]| -> Result<(), String> {
        zip.start_file(name, opts).map_err(zip_err)?;
        zip.write_all(bytes).map_err(zip_err)
    };
    put(
        "manifest.json",
        serde_json::to_string_pretty(&manifest)
            .map_err(zip_err)?
            .as_bytes(),
    )?;
    put(
        "project.tectora",
        project.to_json().map_err(zip_err)?.as_bytes(),
    )?;
    if let Ok(history) = fs::read(store.activity_path(id)?) {
        put("history.jsonl", &history)?;
    }
    put("preview.svg", preview_svg(&project).as_bytes())?;
    for a in &attachments {
        let bytes = fs::read(store.attachment_path(id, &a.name)?).map_err(|e| e.to_string())?;
        put(&format!("files/{}", a.name), &bytes)?;
    }
    zip.finish().map_err(zip_err)?;
    Ok(manifest)
}

/// Reads entry `name`, at most `max` bytes (its declared size is checked
/// first, the bytes actually inflated as they are read).
fn read_entry<R: Read + std::io::Seek>(
    zip: &mut zip::ZipArchive<R>,
    name: &str,
    max: u64,
) -> Result<Option<Vec<u8>>, String> {
    let mut f = match zip.by_name(name) {
        Ok(f) => f,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(e) => return Err(zip_err(e)),
    };
    if f.size() > max {
        return Err(zip_err(format!("{name} is too large")));
    }
    let mut buf = Vec::new();
    f.by_ref()
        .take(max + 1)
        .read_to_end(&mut buf)
        .map_err(zip_err)?;
    if buf.len() as u64 > max {
        return Err(zip_err(format!("{name} is too large")));
    }
    Ok(Some(buf))
}

/// Adds the job in bundle `src` to `store` as a new job; returns its id.
pub fn import(store: &Store, src: &Path) -> Result<String, String> {
    let file = fs::File::open(src).map_err(|e| format!("{}: {e}", src.display()))?;
    let mut zip = zip::ZipArchive::new(file).map_err(zip_err)?;
    if let Some(m) = read_entry(&mut zip, "manifest.json", MAX_TEXT_ENTRY)? {
        let m: Manifest = serde_json::from_slice(&m).map_err(zip_err)?;
        if m.format != FORMAT {
            return Err(zip_err("not a Tectora job"));
        }
        if m.format_version > FORMAT_VERSION {
            return Err(zip_err("made by a newer Tectora"));
        }
    }
    let text = read_entry(&mut zip, "project.tectora", MAX_TEXT_ENTRY)?
        .ok_or_else(|| zip_err("no project in the file"))?;
    let project =
        Project::from_json(&String::from_utf8(text).map_err(zip_err)?).map_err(zip_err)?;
    project.check_limits().map_err(zip_err)?;
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
        return Err(zip_err(format!(
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
        return Err(zip_err(format!(
            "the attached files are too large ({} MB, at most {} MB)",
            declared >> 20,
            MAX_FILES_TOTAL >> 20
        )));
    }
    let id = store.save(None, &project)?;
    let rest = (|| -> Result<(), String> {
        if let Some(history) = history {
            let path = store.activity_path(&id)?;
            fs::create_dir_all(store.activity_dir()).map_err(|e| e.to_string())?;
            fs::write(path, history).map_err(|e| e.to_string())?;
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
        let _ = store.delete(&id);
        return Err(e);
    }
    Ok(id)
}

/// The library job for a file the user opened (double-click, drag and drop):
/// a `.tectora` file in the library opens as it is, one from elsewhere and a
/// `.tectorajob` bundle are added to the library as a new job.
pub fn open_path(store: &Store, path: &Path) -> Result<String, String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext == EXTENSION {
        return import(store, path);
    }
    if ext != crate::store::EXTENSION {
        return Err(format!("{}: not a Tectora file", path.display()));
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
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let project = Project::from_json(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(store.save(None, &project)?)
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
        assert!(e.contains("too large"), "{e}");
        let many = bundle_with(&dir, &[("x.pdf", 0, MAX_FILES + 1)]);
        let e = import(&store, &many).unwrap_err();
        assert!(e.contains("too many"), "{e}");
        assert!(store.list().is_empty());
        let ok = bundle_with(&dir, &[("x.pdf", 1, 2)]);
        let id = import(&store, &ok).unwrap();
        assert_eq!(store.attachments(&id).len(), 2);
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
