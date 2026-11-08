//! A hostile or broken PDF must not crash or block the MCP server
//! (ARCH-027, ARCH-028): drawing tools run in a worker process with a memory
//! and a time limit, and the session answers the next call.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

use flate2::Compression;
use flate2::write::ZlibEncoder;
use serde_json::{Value, json};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tectora-limits-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("lib")).unwrap();
    std::fs::create_dir_all(dir.join("config")).unwrap();
    dir
}

/// A PDF from numbered objects (object 1 is the catalog).
fn pdf(objects: &[Vec<u8>]) -> Vec<u8> {
    let mut p = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objects.iter().enumerate() {
        offsets.push(p.len());
        p.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        p.extend_from_slice(o);
        p.extend_from_slice(b"\nendobj\n");
    }
    let xref = p.len();
    p.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for o in offsets {
        p.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    p.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    p
}

fn stream(dict: &str, data: &[u8]) -> Vec<u8> {
    let mut s = format!("<< {dict} /Length {} >>\nstream\n", data.len()).into_bytes();
    s.extend_from_slice(data);
    s.extend_from_slice(b"\nendstream");
    s
}

fn page_objects(contents: Vec<u8>, resources: &str) -> Vec<Vec<u8>> {
    vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources << {resources} >> /Contents 4 0 R >>")
            .into_bytes(),
        contents,
    ]
}

/// Forms that each draw the next one twice: 2^depth line draws.
fn nested_forms(depth: usize) -> Vec<u8> {
    let mut o = page_objects(stream("", b"/F Do /F Do"), "/XObject << /F 5 0 R >>");
    for i in 0..depth {
        let n = 5 + i;
        o.push(if i + 1 < depth {
            stream(
                &format!("/Type /XObject /Subtype /Form /BBox [0 0 200 200] /Resources << /XObject << /F {} 0 R >> >>", n + 1),
                b"/F Do /F Do",
            )
        } else {
            stream("/Type /XObject /Subtype /Form /BBox [0 0 200 200]", b"1 1 m 50 50 l S")
        });
    }
    pdf(&o)
}

/// A content stream of `mib` MiB of spaces, deflated (about 1000:1).
fn inflate_bomb(mib: usize) -> Vec<u8> {
    let mut z = ZlibEncoder::new(Vec::new(), Compression::best());
    let chunk = vec![b' '; 1 << 20];
    for _ in 0..mib {
        z.write_all(&chunk).unwrap();
    }
    let data = z.finish().unwrap();
    pdf(&page_objects(stream("/Filter /FlateDecode", &data), ""))
}

fn plain_drawing() -> Vec<u8> {
    pdf(&page_objects(
        stream("", b"10 10 m 500 10 l 500 800 l S"),
        "",
    ))
}

struct Mcp {
    child: Child,
    stdin: ChildStdin,
    out: BufReader<ChildStdout>,
    next: u64,
}

impl Mcp {
    fn start(dir: &Path, envs: &[(&str, &str)]) -> Mcp {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_tectora"));
        cmd.args(["mcp", "--dir"])
            .arg(dir.join("lib"))
            .env("TECTORA_CONFIG_DIR", dir.join("config"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        for (k, v) in envs {
            cmd.env(k, v);
        }
        let mut child = cmd.spawn().unwrap();
        let stdin = child.stdin.take().unwrap();
        let out = BufReader::new(child.stdout.take().unwrap());
        let mut m = Mcp {
            child,
            stdin,
            out,
            next: 1,
        };
        m.request(
            "initialize",
            json!({ "protocolVersion": "2025-06-18", "clientInfo": { "name": "test" } }),
        );
        m
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        writeln!(
            self.stdin,
            "{}",
            json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
        )
        .unwrap();
        self.stdin.flush().unwrap();
        let mut line = String::new();
        self.out.read_line(&mut line).unwrap();
        let v: Value = serde_json::from_str(&line)
            .unwrap_or_else(|_| panic!("no answer to {method}: {line:?}"));
        assert_eq!(v["id"], id);
        v["result"].clone()
    }

    fn tool(&mut self, name: &str, args: Value) -> (bool, String) {
        let r = self.request("tools/call", json!({ "name": name, "arguments": args }));
        (
            r["isError"] == true,
            r["content"][0]["text"].as_str().unwrap_or("").to_string(),
        )
    }
}

impl Drop for Mcp {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn a_normal_drawing_is_read_through_the_worker() {
    let dir = scratch("plain");
    let path = dir.join("plan.pdf");
    std::fs::write(&path, plain_drawing()).unwrap();
    let mut mcp = Mcp::start(&dir, &[]);
    let (err, text) = mcp.tool("drawing_info", json!({ "path": path }));
    assert!(!err, "{text}");
    assert!(text.contains("\"pages\""), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn nested_forms_are_stopped_by_the_time_limit_and_the_session_goes_on() {
    let dir = scratch("nested");
    let path = dir.join("nested.pdf");
    std::fs::write(&path, nested_forms(40)).unwrap();
    let mut mcp = Mcp::start(&dir, &[("TECTORA_DRAWING_TIMEOUT_S", "3")]);
    let start = Instant::now();
    let (err, text) = mcp.tool("drawing_info", json!({ "path": path }));
    assert!(err, "{text}");
    assert!(text.contains("longer than 3 s"), "{text}");
    assert!(start.elapsed() < Duration::from_secs(20));
    let (err, text) = mcp.tool("list_projects", json!({}));
    assert!(!err, "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_inflate_bomb_is_stopped_by_the_memory_limit_and_the_session_goes_on() {
    let dir = scratch("bomb");
    let path = dir.join("bomb.pdf");
    std::fs::write(&path, inflate_bomb(256)).unwrap();
    let mut mcp = Mcp::start(&dir, &[("TECTORA_DRAWING_MEMORY_MB", "64")]);
    let (err, text) = mcp.tool("drawing_info", json!({ "path": path }));
    assert!(err, "{text}");
    assert!(text.contains("more memory than allowed"), "{text}");
    let (err, text) = mcp.tool("list_projects", json!({}));
    assert!(!err, "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}
