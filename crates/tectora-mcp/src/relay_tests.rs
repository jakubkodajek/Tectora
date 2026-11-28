//! Bug reports over MCP are sent through the relay (plan-delivery D10, ADR
//! 0008). The server gets its relay `Config` and its outbox folder through
//! `Server::with_relay` / `Server::set_relay` (the test seam named in the
//! plan), never through the process environment. The relay is a std
//! `TcpListener` that plays Apps Script: POST -> 302 -> GET -> JSON.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use tectora_core::store::{Store, now_ms};
use tectora_relay::Config;

use super::Server;

const KEY: &str = "McpTestKey0123456789_-";

struct Relay {
    url: String,
    uploads: Arc<Mutex<Vec<Value>>>,
}

fn read_request(s: &mut TcpStream) -> Option<(String, Vec<u8>)> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    let end = loop {
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
        let n = s.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&buf[..end]).to_string();
    let method = head.split(' ').next()?.to_string();
    let len: usize = head
        .lines()
        .find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.eq_ignore_ascii_case("content-length")
                .then(|| v.trim().parse().ok())?
        })
        .unwrap_or(0);
    while buf.len() < end + len {
        let n = s.read(&mut chunk).ok()?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    Some((method, buf[end..].to_vec()))
}

fn start_relay() -> Relay {
    start_relay_answering(
        r#"{"ok":true,"file_id":"F1","issue_url":"https://github.com/o/r/issues/9"}"#,
    )
}

/// A relay whose answer to every upload is `answer` (the JSON of the GET).
fn start_relay_answering(answer: &'static str) -> Relay {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let uploads = Arc::new(Mutex::new(Vec::new()));
    let u2 = uploads.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let Some((method, body)) = read_request(&mut stream) else {
                continue;
            };
            let reply = if method == "POST" {
                if let Ok(v) = serde_json::from_slice::<Value>(&body) {
                    u2.lock().unwrap().push(v);
                }
                "HTTP/1.1 302 Found\r\nLocation: /macros/echo?k=1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string()
            } else {
                let b = answer;
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b}",
                    b.len()
                )
            };
            let _ = stream.write_all(reply.as_bytes());
        }
    });
    Relay {
        url: format!("http://{addr}/macros/s/T/exec"),
        uploads,
    }
}

fn config_for(url: &str) -> Config {
    Config::checked(url, KEY, true).expect("loopback config")
}

/// A URL nothing listens on.
fn dead_url() -> String {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    format!("http://127.0.0.1:{port}/macros/s/T/exec")
}

fn server_with(relay: Option<Config>) -> (Server, PathBuf) {
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "tectora-mcp-relay-{}-{}-{n}",
        now_ms(),
        std::process::id()
    ));
    let root = dir.join("reports");
    let mut s = Server::with_relay(Store::new(dir.join("lib")).unwrap(), &root, relay);
    s.client = "Claude".into();
    (s, root)
}

fn report(s: &mut Server, text: &str) -> Value {
    s.call_tool(
        "create_bug_report",
        &json!({ "description": text, "summary": "Report the problem" }),
    )
    .unwrap()
}

fn state_in_list(s: &mut Server, name: &str) -> String {
    let l = s.call_tool("list_bug_reports", &json!({})).unwrap();
    l["reports"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["file_name"] == name)
        .unwrap_or_else(|| panic!("{name} not listed: {l}"))["state"]
        .as_str()
        .unwrap()
        .to_string()
}

fn tool(name: &str) -> Value {
    super::tools()
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == name)
        .unwrap_or_else(|| panic!("no tool {name}"))
        .clone()
}

#[test]
fn the_five_bug_report_tools_have_the_documented_kinds() {
    for name in [
        "create_bug_report",
        "send_bug_reports",
        "discard_bug_report",
    ] {
        let t = tool(name);
        assert!(
            t["inputSchema"]["properties"].get("summary").is_some(),
            "{name}"
        );
        assert!(
            t["inputSchema"]["required"].to_string().contains("summary"),
            "{name}"
        );
        assert_eq!(t["annotations"]["readOnlyHint"], false, "{name}");
    }
    assert_eq!(
        tool("discard_bug_report")["annotations"]["destructiveHint"],
        true
    );
    assert!(
        tool("discard_bug_report")["inputSchema"]["required"]
            .to_string()
            .contains("name")
    );
    for name in ["create_bug_report", "send_bug_reports"] {
        assert_ne!(tool(name)["annotations"]["destructiveHint"], true, "{name}");
    }
    for name in ["list_bug_reports", "read_bug_report"] {
        let t = tool(name);
        assert!(
            t["inputSchema"]["properties"].get("summary").is_none(),
            "{name}"
        );
        assert_eq!(t["annotations"]["readOnlyHint"], true, "{name}");
    }
    assert!(
        tool("create_bug_report")["inputSchema"]["properties"]
            .get("path")
            .is_none()
    );
    assert!(
        tool("send_bug_reports")["inputSchema"]["properties"]
            .as_object()
            .unwrap()
            .keys()
            .all(|k| k == "summary")
    );
}

#[test]
fn the_tool_texts_say_the_report_is_sent_not_written_to_a_folder() {
    let create = tool("create_bug_report")["description"]
        .as_str()
        .unwrap()
        .to_lowercase();
    assert!(
        !create.contains("reports_dir") && !create.contains("reports folder"),
        "{create}"
    );
    assert!(
        create.contains("sent") || create.contains("send"),
        "{create}"
    );
    let settings = tool("update_settings");
    assert!(
        settings["inputSchema"]["properties"]
            .get("reports_dir")
            .is_none()
    );
    assert!(
        !settings["description"]
            .as_str()
            .unwrap()
            .contains("reports_dir")
    );
}

#[test]
fn a_report_is_sent_with_the_clients_name_and_reported_as_sent() {
    let relay = start_relay();
    let (mut s, root) = server_with(Some(config_for(&relay.url)));
    let r = report(&mut s, "Gable looks wrong");
    assert_eq!(r["state"], "sent", "{r}");
    let name = r["file_name"].as_str().unwrap();
    assert!(
        name.starts_with("tectora-report-") && name.ends_with(".zip"),
        "{name}"
    );
    assert!(r["size"].as_u64().unwrap() > 0);
    assert!(r.get("path").is_none(), "an AI never learns a path: {r}");
    assert!(!r.to_string().contains(KEY));
    assert!(!r.to_string().contains(&relay.url));

    let up = relay.uploads.lock().unwrap();
    assert_eq!(up.len(), 1, "exactly one upload");
    assert_eq!(up[0]["meta"]["source"], "mcp");
    assert_eq!(up[0]["meta"]["reporter"], "Claude");
    assert_eq!(up[0]["meta"]["description"], "Gable looks wrong");
    assert_eq!(up[0]["file_name"], name);
    drop(up);

    assert_eq!(state_in_list(&mut s, name), "sent");
    let read = s
        .call_tool("read_bug_report", &json!({ "name": name }))
        .unwrap();
    assert_eq!(read["state"], "sent");
    assert!(!read.to_string().contains(KEY));
    assert!(
        !root.join("outbox").join(name).exists(),
        "a sent ZIP is not kept"
    );
}

#[test]
fn with_the_relay_down_the_report_waits_and_a_later_send_delivers_it() {
    let (mut s, _root) = server_with(Some(config_for(&dead_url())));
    let r = report(&mut s, "offline report");
    assert_eq!(r["state"], "waiting", "{r}");
    let name = r["file_name"].as_str().unwrap().to_string();
    assert_eq!(state_in_list(&mut s, &name), "waiting");
    // Still down: nothing is sent and nothing is lost.
    let again = s
        .call_tool("send_bug_reports", &json!({ "summary": "Try again" }))
        .unwrap();
    assert_eq!(again["sent"], 0, "{again}");
    assert_eq!(again["waiting"], 1, "{again}");
    assert_eq!(state_in_list(&mut s, &name), "waiting");

    let relay = start_relay();
    s.set_relay(Some(config_for(&relay.url)));
    let sent = s
        .call_tool(
            "send_bug_reports",
            &json!({ "summary": "Send the waiting report" }),
        )
        .unwrap();
    assert_eq!(sent["sent"], 1, "{sent}");
    assert_eq!(sent["waiting"], 0, "{sent}");
    assert_eq!(state_in_list(&mut s, &name), "sent");
    let up = relay.uploads.lock().unwrap();
    assert_eq!(up.len(), 1);
    assert_eq!(up[0]["meta"]["source"], "mcp");
    assert_eq!(up[0]["meta"]["description"], "offline report");
}

#[test]
fn a_build_without_a_relay_keeps_the_report_local() {
    let (mut s, root) = server_with(None);
    let r = report(&mut s, "no relay here");
    assert_eq!(r["state"], "local", "{r}");
    let name = r["file_name"].as_str().unwrap();
    assert!(root.join("outbox").join(name).is_file());
    assert_eq!(state_in_list(&mut s, name), "local");
    let read = s
        .call_tool("read_bug_report", &json!({ "name": name }))
        .unwrap();
    assert_eq!(read["report"]["description"], "no relay here");
    let sent = s
        .call_tool("send_bug_reports", &json!({ "summary": "Send" }))
        .unwrap();
    assert_eq!(sent["sent"], 0);
}

#[test]
fn a_path_is_refused_and_a_refused_report_queues_nothing() {
    let (mut s, root) = server_with(None);
    let outside = root.join("elsewhere");
    report(&mut s, "ok"); // else every refusal below is vacuous
    let count = |d: &Path| std::fs::read_dir(d).map(|r| r.count()).unwrap_or(0);
    let before = count(&root.join("outbox"));
    assert!(before >= 1);
    for args in [
        json!({ "description": "x", "summary": "s", "path": outside }),
        json!({ "description": "x", "summary": "s", "dir": outside }),
        json!({ "description": "x", "summary": "s", "folder": outside }),
        json!({ "summary": "s" }),
        json!({ "description": "x" }),
        json!({ "description": "x".repeat(5001), "summary": "s" }),
        json!({ "description": "x", "summary": "s", "id": "../escape" }),
    ] {
        assert!(s.call_tool("create_bug_report", &args).is_err(), "{args}");
    }
    assert!(!outside.exists());
    assert_eq!(count(&root.join("outbox")), before);
}

#[test]
fn only_plain_report_names_are_read_and_a_queued_report_is_discarded() {
    let (mut s, root) = server_with(None);
    // Two jobs give two file names (the name carries the job's slug).
    let mut about = |name: &str, text: &str| {
        let made = s
            .call_tool(
                "create_standard_roof",
                &json!({ "name": name, "type": "gable", "length": 12, "width": 8, "pitch_deg": 40, "summary": "start" }),
            )
            .unwrap();
        s.call_tool(
            "create_bug_report",
            &json!({ "description": text, "id": made["id"], "summary": "Report the problem" }),
        )
        .unwrap()
    };
    let a = about("Alpha", "first");
    let b = about("Beta", "second");
    let (na, nb) = (
        a["file_name"].as_str().unwrap(),
        b["file_name"].as_str().unwrap(),
    );
    for bad in [
        "../x.zip",
        "/etc/passwd",
        "a/b.zip",
        "notes.zip",
        "tectora-report-20260101-000000-x.txt",
        "",
        "..",
    ] {
        assert!(
            s.call_tool("read_bug_report", &json!({ "name": bad }))
                .is_err(),
            "read {bad:?}"
        );
        assert!(
            s.call_tool(
                "discard_bug_report",
                &json!({ "name": bad, "summary": "s" })
            )
            .is_err(),
            "discard {bad:?}"
        );
    }
    let r = s
        .call_tool(
            "discard_bug_report",
            &json!({ "name": na, "summary": "Drop the first report" }),
        )
        .unwrap();
    assert_eq!(r["discarded"], na);
    assert!(!root.join("outbox").join(na).exists());
    assert_ne!(na, nb);
    assert!(
        root.join("outbox").join(nb).is_file(),
        "only the named report went"
    );
    // Unknown, or already gone.
    assert!(
        s.call_tool(
            "discard_bug_report",
            &json!({ "name": na, "summary": "again" })
        )
        .is_err()
    );
    assert!(
        s.call_tool(
            "discard_bug_report",
            &json!({ "name": "tectora-report-20260101-000000-missing.zip", "summary": "s" })
        )
        .is_err()
    );
}

#[test]
fn a_sent_report_cannot_be_discarded() {
    let relay = start_relay();
    let (mut s, _root) = server_with(Some(config_for(&relay.url)));
    let name = report(&mut s, "sent one")["file_name"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(state_in_list(&mut s, &name), "sent");
    assert!(
        s.call_tool(
            "discard_bug_report",
            &json!({ "name": name, "summary": "s" })
        )
        .is_err()
    );
    assert_eq!(
        state_in_list(&mut s, &name),
        "sent",
        "the log line is still there"
    );
}

#[test]
fn the_new_tools_refuse_bad_input() {
    let (mut s, _root) = server_with(None);
    for (t, args) in [
        ("discard_bug_report", json!({ "name": 5, "summary": "s" })),
        ("discard_bug_report", json!({ "name": "x.zip" })),
        ("discard_bug_report", json!({ "summary": "s" })),
        (
            "discard_bug_report",
            json!({ "name": "x.zip", "summary": "s", "extra": 1 }),
        ),
        ("send_bug_reports", json!({})),
        ("send_bug_reports", json!({ "summary": "s", "force": true })),
        ("send_bug_reports", json!({ "summary": 7 })),
        ("list_bug_reports", json!({ "limit": "many" })),
        ("list_bug_reports", json!({ "limit": -1 })),
        ("read_bug_report", json!({ "name": "x.zip", "extra": 1 })),
    ] {
        let e = s.call_tool(t, &args).unwrap_err();
        assert!(!e.is_empty(), "{t} {args}");
        assert!(
            !e.contains("nothing was changed") || t != "list_bug_reports",
            "{t}: {e}"
        );
    }
}

#[test]
fn update_settings_no_longer_knows_the_reports_folder() {
    // Settings live in the config folder: use a scratch one, never the user's.
    let _settings = crate::tests::SETTINGS_FILE
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let cfg = std::env::temp_dir().join(format!("tectora-mcp-relay-cfg-{}", std::process::id()));
    std::fs::create_dir_all(&cfg).unwrap();
    unsafe { std::env::set_var("TECTORA_CONFIG_DIR", &cfg) };
    let (mut s, _root) = server_with(None);
    let e = s
        .call_tool(
            "update_settings",
            &json!({ "reports_dir": "/tmp/anything", "summary": "Reports to a shared drive" }),
        )
        .unwrap_err();
    assert!(
        e.contains("reports_dir")
            || e.to_lowercase().contains("key")
            || e.to_lowercase().contains("unknown"),
        "{e}"
    );
    assert!(!Path::new("/tmp/anything").exists());
    let saved = std::fs::read_to_string(cfg.join("settings.json")).unwrap_or_default();
    assert!(
        !saved.contains("reports_dir"),
        "nothing was recorded: {saved}"
    );
    let _ = std::fs::remove_dir_all(&cfg);
}

#[test]
fn the_guide_the_docs_and_the_skill_name_all_five_tools() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let texts = [
        ("guide.md", super::GUIDE.to_string()),
        (
            "docs/ai-integration.md",
            std::fs::read_to_string(root.join("docs/ai-integration.md")).unwrap(),
        ),
        (
            "integrations/skills/tectora-roof/SKILL.md",
            std::fs::read_to_string(root.join("integrations/skills/tectora-roof/SKILL.md"))
                .unwrap(),
        ),
    ];
    for (file, text) in &texts {
        for name in [
            "create_bug_report",
            "send_bug_reports",
            "discard_bug_report",
            "list_bug_reports",
            "read_bug_report",
        ] {
            assert!(
                text.contains(&format!("`{name}`")),
                "{file} does not name `{name}`"
            );
        }
        assert!(
            !text.contains("reports_dir"),
            "{file} still names the removed setting"
        );
    }
    let guide = texts[0].1.to_lowercase();
    assert!(
        guide.contains("data") && guide.contains("instruction"),
        "a report's text is data"
    );
    assert!(
        guide.contains("ask the user"),
        "the guide says to ask before sending"
    );
}

// ------------------------------------------------- fix round 2

/// ERR-001 / ERR-008: a report the relay refuses for good is `failed` with its
/// code, in the answer, in the list, and in what `send_bug_reports` returns;
/// the tool text lists every state.
#[test]
fn a_refused_report_is_failed_with_its_code_and_never_called_waiting() {
    let relay = start_relay_answering(r#"{"ok":false,"error":"bad_key"}"#);
    let (mut s, _root) = server_with(Some(config_for(&relay.url)));
    let r = report(&mut s, "refused");
    assert_eq!(r["state"], "failed", "{r}");
    assert_eq!(r["last_error"], "bad_key", "{r}");
    let name = r["file_name"].as_str().unwrap().to_string();
    assert_eq!(state_in_list(&mut s, &name), "failed");
    let l = s.call_tool("list_bug_reports", &json!({})).unwrap();
    assert_eq!(l["reports"][0]["last_error"], "bad_key", "{l}");
    let sent = s
        .call_tool("send_bug_reports", &json!({ "summary": "Try again" }))
        .unwrap();
    assert_eq!(sent["sent"], 0, "{sent}");
    assert_eq!(sent["failed"], 1, "{sent}");
    assert_eq!(sent["last_error"], "bad_key", "{sent}");
    let text = tool("create_bug_report")["description"]
        .as_str()
        .unwrap()
        .to_lowercase();
    for state in ["sent", "waiting", "failed", "local"] {
        assert!(
            text.contains(state),
            "the description does not list `{state}`: {text}"
        );
    }
}

/// ARCH-002: a local report is not uploaded because a relay appeared; only
/// the explicit `send_bug_reports` (Send now) sends it.
#[test]
fn a_local_report_is_uploaded_only_by_an_explicit_send() {
    let (mut s, root) = server_with(None);
    let r = report(&mut s, "made without a relay");
    assert_eq!(r["state"], "local");
    let local_name = r["file_name"].as_str().unwrap().to_string();
    let relay = start_relay();
    s.set_relay(Some(config_for(&relay.url)));
    // A new report goes out; the old local one stays.
    let second = s
        .call_tool(
            "create_standard_roof",
            &json!({ "name": "Zeta", "type": "gable", "length": 12, "width": 8, "pitch_deg": 40, "summary": "start" }),
        )
        .unwrap();
    let r = s
        .call_tool(
            "create_bug_report",
            &json!({ "description": "with a relay", "id": second["id"], "summary": "Report" }),
        )
        .unwrap();
    assert_eq!(r["state"], "sent", "{r}");
    assert_eq!(
        relay.uploads.lock().unwrap().len(),
        1,
        "the local report was not uploaded"
    );
    assert_eq!(state_in_list(&mut s, &local_name), "local");
    assert!(root.join("outbox").join(&local_name).is_file());
    // Send now.
    let sent = s
        .call_tool(
            "send_bug_reports",
            &json!({ "summary": "Send the waiting report" }),
        )
        .unwrap();
    assert_eq!(sent["sent"], 1, "{sent}");
    assert_eq!(relay.uploads.lock().unwrap().len(), 2);
    assert_eq!(state_in_list(&mut s, &local_name), "sent");
}

// ---- fix round 3 ----

/// ARCH-004: the name `create_bug_report` returns reads that report, also
/// for a second report of the same second and job.
#[test]
fn the_name_a_report_is_created_with_reads_that_report() {
    let relay = start_relay();
    let (mut s, _root) = server_with(Some(config_for(&relay.url)));
    let first = report(&mut s, "FIRST-ZQX");
    let second = report(&mut s, "SECOND-ZQX");
    let (a, b) = (
        first["file_name"].as_str().unwrap(),
        second["file_name"].as_str().unwrap(),
    );
    assert_ne!(a, b, "two reports, one name");
    let read_a = s
        .call_tool("read_bug_report", &json!({ "name": a }))
        .unwrap();
    let read_b = s
        .call_tool("read_bug_report", &json!({ "name": b }))
        .unwrap();
    assert_eq!(read_a["sent"]["description"], "FIRST-ZQX", "{read_a}");
    assert_eq!(read_b["sent"]["description"], "SECOND-ZQX", "{read_b}");
}

/// ARCH-T4: Send now over MCP also uploads reports that were kept on this
/// computer; the tool text says so.
#[test]
fn send_bug_reports_says_it_also_uploads_local_reports() {
    let text = tool("send_bug_reports")["description"]
        .as_str()
        .unwrap()
        .to_lowercase();
    assert!(text.contains("local"), "{text}");
}
