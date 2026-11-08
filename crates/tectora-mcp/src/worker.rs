//! Drawing tools run in a child process with a memory and time limit.
//!
//! The PDF reader has no limit on how far a stream inflates or how often a
//! form draws other forms. A small hostile or broken PDF could otherwise
//! abort the whole MCP server (out of memory, which no panic guard catches)
//! or block it for good (ARCH-027, ARCH-028). So each drawing call runs in
//! a fresh copy of the same executable, started with
//! [`WORKER_ENV`] set to the library folder: it caps its own heap with
//! [`CappedAlloc`], answers one call on stdout and exits. The server kills
//! it after [`timeout`] and reports a plain error; the session goes on.

use std::alloc::{GlobalAlloc, Layout, System};
use std::io::{BufRead, Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tectora_core::store::Store;

use crate::{Server, ToolResult};

/// Set (to the library folder) in the environment of a drawing worker.
pub const WORKER_ENV: &str = "TECTORA_DRAWING_WORKER";
/// The client name, passed on so the worker's live status names it.
const CLIENT_ENV: &str = "TECTORA_DRAWING_CLIENT";
/// Overrides the time limit, seconds (tests).
const TIMEOUT_ENV: &str = "TECTORA_DRAWING_TIMEOUT_S";
/// Overrides the memory limit, MiB (tests).
const MEMORY_ENV: &str = "TECTORA_DRAWING_MEMORY_MB";

/// Heap a drawing call may use. A dense A0 plot with a 4096 px render
/// needs a few hundred MB.
const MEMORY_LIMIT: usize = 1536 * 1024 * 1024;
/// Wall-clock time a drawing call may take.
const TIMEOUT: Duration = Duration::from_secs(90);

/// The tools that open a PDF (`list_drawings` only lists file names).
pub fn opens_pdf(name: &str) -> bool {
    matches!(
        name,
        "drawing_info"
            | "view_drawing"
            | "drawing_lines"
            | "measure_drawing"
            | "drawing_dimensions"
            | "trace_outline"
    )
}

static LIMIT: AtomicUsize = AtomicUsize::new(usize::MAX);
static USED: AtomicUsize = AtomicUsize::new(0);

/// The system allocator with an optional cap on the bytes in use. The cap is
/// off (`usize::MAX`) except in a drawing worker; an allocation over it
/// fails, and Rust then aborts the worker, not the server.
pub struct CappedAlloc;

fn reserve(size: usize) -> bool {
    let limit = LIMIT.load(Ordering::Relaxed);
    if limit == usize::MAX {
        USED.fetch_add(size, Ordering::Relaxed);
        return true;
    }
    let mut used = USED.load(Ordering::Relaxed);
    loop {
        let Some(next) = used.checked_add(size).filter(|n| *n <= limit) else {
            return false;
        };
        match USED.compare_exchange_weak(used, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return true,
            Err(now) => used = now,
        }
    }
}

fn release(size: usize) {
    USED.fetch_sub(size, Ordering::Relaxed);
}

// SAFETY: every call is forwarded to `System` with the same layout and
// pointer; the counter only decides whether to forward an allocation.
unsafe impl GlobalAlloc for CappedAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if !reserve(layout.size()) {
            return std::ptr::null_mut();
        }
        // SAFETY: same contract as the caller's.
        let p = unsafe { System.alloc(layout) };
        if p.is_null() {
            release(layout.size());
        }
        p
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if !reserve(layout.size()) {
            return std::ptr::null_mut();
        }
        // SAFETY: same contract as the caller's.
        let p = unsafe { System.alloc_zeroed(layout) };
        if p.is_null() {
            release(layout.size());
        }
        p
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` was allocated by `System` with `layout`.
        unsafe { System.dealloc(ptr, layout) };
        release(layout.size());
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let old = layout.size();
        if new_size > old && !reserve(new_size - old) {
            return std::ptr::null_mut();
        }
        // SAFETY: `ptr` was allocated by `System` with `layout`.
        let p = unsafe { System.realloc(ptr, layout, new_size) };
        if p.is_null() {
            if new_size > old {
                release(new_size - old);
            }
        } else if new_size < old {
            release(old - new_size);
        }
        p
    }
}

#[global_allocator]
static GLOBAL: CappedAlloc = CappedAlloc;

fn env_number(key: &str) -> Option<u64> {
    std::env::var(key).ok()?.trim().parse().ok()
}

fn timeout() -> Duration {
    env_number(TIMEOUT_ENV).map_or(TIMEOUT, Duration::from_secs)
}

/// Runs as a drawing worker when [`WORKER_ENV`] is set: answers the one call
/// on stdin and returns true. False: not a worker, serve MCP as usual.
pub fn run_if_worker() -> bool {
    let Some(dir) = std::env::var_os(WORKER_ENV) else {
        return false;
    };
    let limit = env_number(MEMORY_ENV).map_or(MEMORY_LIMIT, |mb| (mb as usize) << 20);
    LIMIT.store(limit, Ordering::Relaxed);
    let answer = match worker_call(PathBuf::from(dir)) {
        Ok(v) => json!({ "ok": v }),
        Err(e) => json!({ "err": e }),
    };
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{answer}");
    let _ = out.flush();
    true
}

fn worker_call(dir: PathBuf) -> ToolResult {
    let mut line = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    let call: Value = serde_json::from_str(&line).map_err(|e| e.to_string())?;
    let name = call.get("name").and_then(Value::as_str).unwrap_or("");
    if !opens_pdf(name) {
        return Err(format!("not a drawing tool: `{name}`"));
    }
    let mut server = Server::new(Store::new(dir).map_err(|e| e.to_string())?);
    if let Ok(client) = std::env::var(CLIENT_ENV) {
        server.client = client;
    }
    let args = call.get("args").cloned().unwrap_or(json!({}));
    tectora_core::model::guarded(|| {
        server.call_drawing(name, &args).unwrap_or(Ok(Value::Null))
    })?
}

impl Server {
    /// A drawing tool in a worker process (see the module docs).
    pub(crate) fn call_drawing_isolated(&self, name: &str, args: &Value) -> Option<ToolResult> {
        if !opens_pdf(name) || cfg!(test) || std::env::var_os(WORKER_ENV).is_some() {
            return self.call_drawing(name, args);
        }
        Some(self.run_worker(name, args))
    }

    fn run_worker(&self, name: &str, args: &Value) -> ToolResult {
        let exe =
            std::env::current_exe().map_err(|e| format!("cannot start the drawing reader: {e}"))?;
        let mut child = Command::new(exe)
            .arg("mcp")
            .env(WORKER_ENV, &self.store.dir)
            .env(CLIENT_ENV, &self.client)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("cannot start the drawing reader: {e}"))?;
        let request = json!({ "name": name, "args": args }).to_string();
        if let Some(mut stdin) = child.stdin.take() {
            let _ = writeln!(stdin, "{request}");
        }
        let mut stdout = child
            .stdout
            .take()
            .ok_or("cannot read the drawing reader")?;
        let reader = std::thread::spawn(move || {
            let mut text = String::new();
            let _ = stdout.read_to_string(&mut text);
            text
        });
        let limit = timeout();
        let start = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if start.elapsed() < limit => {
                    std::thread::sleep(Duration::from_millis(20))
                }
                Ok(None) | Err(_) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!(
                        "reading the drawing took longer than {} s and was stopped; the PDF is too complex or damaged. Try another export of the drawing (for example a PDF printed from the CAD program, or an image)",
                        limit.as_secs()
                    ));
                }
            }
        };
        let text = reader.join().unwrap_or_default();
        let answer: Option<Value> = text
            .lines()
            .last()
            .and_then(|l| serde_json::from_str(l).ok());
        match answer {
            Some(mut a) if a.get("ok").is_some() => Ok(a["ok"].take()),
            Some(a) if a.get("err").is_some() => Err(a["err"]
                .as_str()
                .unwrap_or("the drawing could not be read")
                .to_string()),
            _ => Err(format!(
                "reading the drawing needed more memory than allowed or failed ({status}) and was stopped; the PDF is too complex or damaged. Try another export of the drawing"
            )),
        }
    }
}
