//! A small rotating text log shared by the app, the MCP server and the CLI,
//! std only. It exists so a bug report can carry the last lines of what the
//! program did (`bugreport`). Every function here is best effort: a log
//! folder that cannot be created or written makes the calls no-ops, and
//! nothing in this module panics or blocks.
//!
//! The folder comes from `TECTORA_LOG_DIR`, else `<TECTORA_CONFIG_DIR>/logs`
//! when that is set, else the OS folder (macOS `~/Library/Logs/Tectora`,
//! Windows `%LOCALAPPDATA%\Tectora\logs`, Linux `$XDG_STATE_HOME/tectora`).
//! It is read at each call, so a test can point it elsewhere.

use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, Once};

/// Size at which `<role>.log` becomes `<role>.1.log`.
pub const MAX_LOG_BYTES: u64 = 512 * 1024;
/// Longest log line, characters.
pub const MAX_LINE_CHARS: usize = 2000;

struct State {
    role: Option<String>,
    /// The last panic line and how often it repeated without being written.
    last_panic: String,
    repeats: u32,
}

static STATE: Mutex<State> = Mutex::new(State {
    role: None,
    last_panic: String::new(),
    repeats: 0,
});

/// The folder the logs are written to.
pub fn log_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("TECTORA_LOG_DIR").filter(|v| !v.is_empty()) {
        return PathBuf::from(d);
    }
    if let Some(d) = std::env::var_os("TECTORA_CONFIG_DIR").filter(|v| !v.is_empty()) {
        return PathBuf::from(d).join("logs");
    }
    if cfg!(target_os = "macos") {
        return dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Library")
            .join("Logs")
            .join("Tectora");
    }
    if cfg!(windows) {
        return dirs::data_local_dir()
            .or_else(dirs::home_dir)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Tectora")
            .join("logs");
    }
    dirs::state_dir()
        .or_else(dirs::data_local_dir)
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("tectora")
}

fn valid_role(role: &str) -> bool {
    (1..=16).contains(&role.len()) && role.bytes().all(|b| b.is_ascii_lowercase())
}

/// Year, month, day, hour, minute, second of a Unix time in ms, UTC.
pub fn utc_parts(ms: u64) -> (i64, i64, i64, u64, u64, u64) {
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
    (year, month, day, rem / 3600, rem % 3600 / 60, rem % 60)
}

/// `YYYY-MM-DD HH:MM:SS` in UTC.
pub fn utc_iso(ms: u64) -> String {
    let (y, mo, d, h, mi, s) = utc_parts(ms);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}:{s:02}")
}

fn clean(msg: &str) -> String {
    let mut out = String::new();
    for c in msg.chars().take(MAX_LINE_CHARS * 2) {
        if out.chars().count() >= MAX_LINE_CHARS {
            break;
        }
        if c.is_control() {
            out.push(' ');
        } else if matches!(
            c,
            '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2028}' | '\u{2029}'
                | '\u{2066}'..='\u{2069}' | '\u{feff}'
        ) {
            // dropped
        } else {
            out.push(c);
        }
    }
    out
}

fn append(dir: &Path, role: &str, level: &str, msg: &str) {
    let text = format!(
        "{} {} {}\n",
        utc_iso(crate::store::now_ms()),
        clean(level),
        clean(msg)
    );
    if fs::create_dir_all(dir).is_err() {
        return;
    }
    let file = dir.join(format!("{role}.log"));
    // Never through a link or a folder put in its place.
    if let Ok(m) = fs::symlink_metadata(&file) {
        if !m.is_file() {
            return;
        }
        if m.len() >= MAX_LOG_BYTES {
            let old = dir.join(format!("{role}.1.log"));
            let _ = fs::remove_file(&old);
            let _ = fs::rename(&file, &old);
        }
    }
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(&file) {
        let _ = f.write_all(text.as_bytes());
    }
}

fn current_role(state: &State) -> Option<String> {
    state.role.clone().filter(|r| valid_role(r))
}

/// Names the log (`app` or `mcp`) and writes a start line with the version
/// and the system. Until this is called, [`line`] does nothing.
pub fn init(role: &str) {
    if !valid_role(role) {
        return;
    }
    let mut st = STATE.lock().unwrap_or_else(|e| e.into_inner());
    st.role = Some(role.to_string());
    st.last_panic.clear();
    st.repeats = 0;
    append(
        &log_dir(),
        role,
        "INFO",
        &format!(
            "start {role} {} on {} {}",
            env!("CARGO_PKG_VERSION"),
            std::env::consts::OS,
            std::env::consts::ARCH
        ),
    );
}

/// Appends one line (control characters stripped, at most
/// [`MAX_LINE_CHARS`]). Never fails.
pub fn line(level: &str, msg: &str) {
    let st = STATE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(role) = current_role(&st) {
        append(&log_dir(), &role, level, msg);
    }
}

/// Logs panics (message and location) from now on, then calls the hook that
/// was there. The same panic again and again is one line plus a count. The
/// hook never blocks and never panics: a busy log drops the line.
pub fn install_panic_hook() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let payload = info.payload();
                let msg = payload
                    .downcast_ref::<&str>()
                    .map(|s| (*s).to_string())
                    .or_else(|| payload.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "panic".into());
                let at = info
                    .location()
                    .map(|l| format!("{}:{}", l.file(), l.line()))
                    .unwrap_or_default();
                let text = clean(&format!("panic: {msg} at {at}"));
                if let Ok(mut st) = STATE.try_lock()
                    && let Some(role) = current_role(&st)
                {
                    let dir = log_dir();
                    if st.last_panic == text {
                        st.repeats = st.repeats.saturating_add(1);
                    } else {
                        if st.repeats > 0 {
                            let n = st.repeats;
                            append(
                                &dir,
                                &role,
                                "WARN",
                                &format!("previous panic repeated {n} times"),
                            );
                        }
                        st.last_panic = text.clone();
                        st.repeats = 0;
                        append(&dir, &role, "ERROR", &text);
                    }
                }
            }));
            previous(info);
        }));
    });
}

/// The end of `<role>.log`: whole lines only, at most `max_bytes` bytes and
/// `max_lines` lines. Empty when there is no log.
pub fn tail(role: &str, max_bytes: u64, max_lines: usize) -> String {
    if !valid_role(role) || max_bytes == 0 || max_lines == 0 {
        return String::new();
    }
    let path = log_dir().join(format!("{role}.log"));
    let Ok(meta) = fs::symlink_metadata(&path) else {
        return String::new();
    };
    if !meta.is_file() {
        return String::new();
    }
    let Ok(mut f) = fs::File::open(&path) else {
        return String::new();
    };
    let len = meta.len();
    let start = len.saturating_sub(max_bytes);
    if f.seek(SeekFrom::Start(start)).is_err() {
        return String::new();
    }
    let mut buf = Vec::new();
    if f.take(max_bytes).read_to_end(&mut buf).is_err() {
        return String::new();
    }
    let mut text = String::from_utf8_lossy(&buf).into_owned();
    if start > 0 {
        // The first line is probably cut.
        match text.find('\n') {
            Some(i) => text.drain(..=i),
            None => return String::new(),
        };
    }
    let lines: Vec<&str> = text.lines().collect();
    let from = lines.len().saturating_sub(max_lines);
    let mut out = lines[from..].join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    // Lossy conversion may have grown the text past the byte limit.
    while out.len() as u64 > max_bytes {
        match out.find('\n') {
            Some(i) => out.drain(..=i),
            None => return String::new(),
        };
    }
    out
}
