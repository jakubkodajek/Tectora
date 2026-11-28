//! Relay client against a mock relay (plan-delivery D9, ADR 0008). The mock
//! is a std `TcpListener` that answers a scripted list of replies and records
//! every request; Apps Script's `POST -> 302 -> GET` flow is the normal case.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::Engine;
use serde_json::{Value, json};
use tectora_relay::{Config, SendError, Timeouts, send_with};

const KEY: &str = "Key0123456789abcdefXYZ_-";
const ZIP_BYTES: &[u8] = b"PK\x03\x04 pretend zip \x00\x01\x02\xff";

#[derive(Debug, Clone)]
struct Seen {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

enum Reply {
    Answer {
        status: u16,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
    },
    /// Read the request, then never answer (until the mock is dropped).
    Hang,
}

fn redirect(to: &str) -> Reply {
    Reply::Answer {
        status: 302,
        headers: vec![("Location".into(), to.into())],
        body: b"Moved".to_vec(),
    }
}

fn answer(status: u16, body: impl AsRef<[u8]>) -> Reply {
    Reply::Answer {
        status,
        headers: vec![("Content-Type".into(), "application/json".into())],
        body: body.as_ref().to_vec(),
    }
}

struct Mock {
    addr: SocketAddr,
    seen: Arc<Mutex<Vec<Seen>>>,
    connections: Arc<AtomicUsize>,
    _release: Sender<()>,
}

impl Mock {
    fn start(replies: Vec<Reply>) -> Mock {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let connections = Arc::new(AtomicUsize::new(0));
        let (release, hold): (Sender<()>, Receiver<()>) = channel();
        let (s2, c2) = (seen.clone(), connections.clone());
        std::thread::spawn(move || {
            let mut replies = replies.into_iter();
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                c2.fetch_add(1, Ordering::SeqCst);
                let Some(req) = read_request(&mut stream) else {
                    continue;
                };
                s2.lock().unwrap().push(req);
                match replies.next() {
                    Some(Reply::Answer {
                        status,
                        headers,
                        body,
                    }) => {
                        let mut head = format!(
                            "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n",
                            body.len()
                        );
                        for (k, v) in headers {
                            head.push_str(&format!("{k}: {v}\r\n"));
                        }
                        head.push_str("\r\n");
                        let _ = stream.write_all(head.as_bytes());
                        let _ = stream.write_all(&body);
                    }
                    Some(Reply::Hang) => {
                        // Returns (an error) as soon as the test drops the mock.
                        let _ = hold.recv();
                        break;
                    }
                    None => {}
                }
            }
        });
        Mock {
            addr,
            seen,
            connections,
            _release: release,
        }
    }

    fn url(&self) -> String {
        format!("http://{}/macros/s/T/exec", self.addr)
    }

    fn base(&self) -> String {
        format!("http://{}", self.addr)
    }

    fn requests(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }
}

fn read_request(stream: &mut TcpStream) -> Option<Seen> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
    let mut lines = head.split("\r\n");
    let mut first = lines.next()?.split(' ');
    let method = first.next()?.to_string();
    let path = first.next()?.to_string();
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect();
    let len: usize = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);
    while buf.len() < head_end + len {
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    Some(Seen {
        method,
        path,
        headers,
        body: buf[head_end..].to_vec(),
    })
}

fn zip_file(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tectora-relay-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("tectora-report-20261128-143012-app.zip");
    std::fs::write(&p, ZIP_BYTES).unwrap();
    p
}

const SHORT: Timeouts = Timeouts {
    connect: Duration::from_secs(5),
    total: Duration::from_secs(10),
};

fn upload(url: &str, tag: &str, t: Timeouts) -> Result<tectora_relay::Receipt, SendError> {
    let cfg = Config::checked(url, KEY, true).expect("a loopback http config is accepted");
    send_with(
        &cfg,
        "0123456789abcdef0123456789abcdef",
        "tectora-report-20261128-143012-app.zip",
        &json!({ "description": "d", "source": "app" }),
        &zip_file(tag),
        t,
    )
}

/// The normal Apps Script flow: POST, 302, GET, JSON.
fn through_302(body: impl AsRef<[u8]>, tag: &str) -> Result<tectora_relay::Receipt, SendError> {
    let m = Mock::start(vec![redirect("/macros/echo?k=1"), answer(200, body)]);
    upload(&m.url(), tag, SHORT)
}

// ---------------------------------------------------------------- D9a

#[test]
fn a_post_answered_with_302_is_followed_with_a_get_and_gives_the_receipt() {
    let m = Mock::start(vec![
        redirect("/macros/echo?k=1"),
        answer(
            200,
            r#"{"ok":true,"file_id":"F","issue_url":"https://github.com/o/r/issues/7"}"#,
        ),
    ]);
    let r = upload(&m.url(), "a", SHORT).expect("receipt");
    assert_eq!(r.file_id, "F");
    assert_eq!(
        r.issue_url.as_deref(),
        Some("https://github.com/o/r/issues/7")
    );

    let seen = m.requests();
    assert_eq!(seen.len(), 2, "one POST and one GET: {seen:?}");
    let (post, get) = (&seen[0], &seen[1]);
    assert_eq!(post.method, "POST");
    assert_eq!(post.path, "/macros/s/T/exec");
    assert!(
        post.headers
            .iter()
            .any(|(k, v)| k.eq_ignore_ascii_case("content-type")
                && v.starts_with("application/json")),
        "{:?}",
        post.headers
    );
    let body: Value = serde_json::from_slice(&post.body).expect("the body is JSON");
    assert_eq!(body["v"], 1);
    assert_eq!(body["key"], KEY);
    assert_eq!(body["id"], "0123456789abcdef0123456789abcdef");
    assert_eq!(body["file_name"], "tectora-report-20261128-143012-app.zip");
    assert_eq!(body["size"], ZIP_BYTES.len());
    assert_eq!(body["meta"]["source"], "app");
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(body["zip_b64"].as_str().expect("zip_b64"))
        .unwrap();
    assert_eq!(decoded, ZIP_BYTES);

    assert_eq!(get.method, "GET");
    assert_eq!(get.path, "/macros/echo?k=1");
    assert!(get.body.is_empty(), "the GET carries no body");
    assert!(!get.path.contains(KEY));
    for (k, v) in &get.headers {
        assert!(!v.contains(KEY), "the key leaked into header {k}");
    }
}

// ---------------------------------------------------------------- D9b

#[test]
fn bad_key_is_permanent() {
    let e = through_302(r#"{"ok":false,"error":"bad_key"}"#, "b1").unwrap_err();
    assert_eq!(e.code(), "bad_key");
    assert!(e.permanent());
    assert!(matches!(e, SendError::Refused { .. }), "{e:?}");
}

#[test]
fn bad_request_and_too_large_are_permanent() {
    for code in ["bad_request", "too_large"] {
        let e = through_302(json!({ "ok": false, "error": code }).to_string(), "b1b").unwrap_err();
        assert_eq!(e.code(), code);
        assert!(e.permanent(), "{code}");
    }
}

#[test]
fn rate_limited_is_a_retry_with_the_wait_the_relay_names() {
    let e = through_302(
        r#"{"ok":false,"error":"rate_limited","retry_after_s":120}"#,
        "b2",
    )
    .unwrap_err();
    assert_eq!(e.code(), "rate_limited");
    assert!(!e.permanent());
    assert!(
        matches!(
            e,
            SendError::Refused {
                retry_after_s: Some(120),
                ..
            }
        ),
        "{e:?}"
    );
}

#[test]
fn a_wait_longer_than_a_day_is_capped_at_24_hours() {
    let e = through_302(
        r#"{"ok":false,"error":"rate_limited","retry_after_s":999999999}"#,
        "b2b",
    )
    .unwrap_err();
    assert!(
        matches!(
            e,
            SendError::Refused {
                retry_after_s: Some(86_400),
                ..
            }
        ),
        "{e:?}"
    );
}

#[test]
fn a_server_error_code_is_a_retry() {
    let e = through_302(r#"{"ok":false,"error":"server"}"#, "b2c").unwrap_err();
    assert_eq!(e.code(), "server");
    assert!(!e.permanent());
}

#[test]
fn answers_that_are_not_the_relays_json_are_bad_answers() {
    let long = format!(r#"{{"ok":true,"file_id":"{}"}}"#, "x".repeat(70 * 1024));
    let cases: Vec<(&str, Vec<u8>)> = vec![
        (
            "html",
            b"<!DOCTYPE html><html><body>Error: TypeError</body></html>".to_vec(),
        ),
        ("empty", Vec::new()),
        ("70 KB", long.into_bytes()),
        ("ok without file_id", br#"{"ok":true}"#.to_vec()),
        (
            "ok is not a boolean",
            br#"{"ok":"yes","file_id":"F"}"#.to_vec(),
        ),
        (
            "unknown error code",
            br#"{"ok":false,"error":"what"}"#.to_vec(),
        ),
        ("json array", b"[1,2]".to_vec()),
    ];
    for (name, body) in cases {
        let e = through_302(&body, "b3").expect_err(name);
        assert!(matches!(e, SendError::BadAnswer(_)), "{name}: {e:?}");
        assert!(
            !e.permanent(),
            "{name}: a bad answer is retried with backoff"
        );
    }
}

#[test]
fn an_issue_link_that_is_not_on_github_is_dropped() {
    for url in [
        "https://evil.example/o/r/issues/7",
        "http://github.com/o/r/issues/7",
        "javascript:alert(1)",
    ] {
        let body = json!({ "ok": true, "file_id": "F", "issue_url": url }).to_string();
        let r = through_302(body, "b4").expect("the report itself was stored");
        assert_eq!(r.file_id, "F");
        assert_eq!(r.issue_url, None, "{url}");
    }
    let long = format!("https://github.com/{}", "a".repeat(400));
    let body = json!({ "ok": true, "file_id": "F", "issue_url": long }).to_string();
    assert_eq!(
        through_302(body, "b4").unwrap().issue_url,
        None,
        "over 300 chars"
    );
    let body = json!({ "ok": true, "file_id": "F", "issue_url": null }).to_string();
    assert_eq!(through_302(body, "b4").unwrap().issue_url, None);
}

#[test]
fn http_server_errors_retry_and_other_client_errors_are_bad_answers() {
    for status in [500u16, 502, 429] {
        let m = Mock::start(vec![answer(status, "oops")]);
        let e = upload(&m.url(), "b5", SHORT).unwrap_err();
        assert!(!e.permanent(), "{status}: {e:?}");
    }
    let m = Mock::start(vec![answer(404, "no")]);
    let e = upload(&m.url(), "b5", SHORT).unwrap_err();
    assert!(matches!(e, SendError::BadAnswer(_)), "{e:?}");
}

#[test]
fn a_missing_zip_or_a_directory_is_an_io_error_and_nothing_is_sent() {
    let m = Mock::start(vec![answer(200, "{}")]);
    let cfg = Config::checked(&m.url(), KEY, true).unwrap();
    let dir = std::env::temp_dir().join(format!("tectora-relay-io-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for p in [dir.join("gone.zip"), dir.clone()] {
        let e = send_with(
            &cfg,
            "0123456789abcdef0123456789abcdef",
            "x.zip",
            &json!({}),
            &p,
            SHORT,
        )
        .unwrap_err();
        assert!(matches!(e, SendError::Io(_)), "{e:?}");
    }
    assert!(m.requests().is_empty());
}

#[test]
fn a_zip_over_the_upload_cap_is_too_large_and_nothing_is_sent() {
    let m = Mock::start(vec![answer(200, "{}")]);
    let cfg = Config::checked(&m.url(), KEY, true).unwrap();
    let dir = std::env::temp_dir().join(format!("tectora-relay-big-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let big = dir.join("big.zip");
    let f = std::fs::File::create(&big).unwrap();
    f.set_len(tectora_relay::MAX_UPLOAD + 1).unwrap();
    let e = send_with(
        &cfg,
        "0123456789abcdef0123456789abcdef",
        "x.zip",
        &json!({}),
        &big,
        SHORT,
    )
    .unwrap_err();
    assert_eq!(e, SendError::TooLarge);
    assert!(e.permanent());
    assert!(m.requests().is_empty());
}

// ---------------------------------------------------------------- D9c

#[test]
fn a_redirect_to_another_origin_is_not_followed() {
    let other = Mock::start(vec![answer(200, r#"{"ok":true,"file_id":"EVIL"}"#)]);
    let m = Mock::start(vec![redirect(&format!("{}/macros/echo", other.base()))]);
    let e = upload(&m.url(), "c1", SHORT).unwrap_err();
    assert_eq!(e.code(), "relay_auth", "{e:?}");
    assert!(matches!(e, SendError::Refused { .. }), "{e:?}");
    assert_eq!(m.requests().len(), 1, "only the POST");
    assert_eq!(
        other.connections.load(Ordering::SeqCst),
        0,
        "no connection to the other host"
    );
    assert!(other.requests().is_empty());
}

#[test]
fn a_redirect_to_a_login_page_or_to_https_elsewhere_is_not_followed() {
    for to in [
        "https://accounts.google.com/ServiceLogin?x=1",
        "https://example.org/macros/echo",
        "ftp://127.0.0.1/x",
    ] {
        let m = Mock::start(vec![redirect(to)]);
        let e = upload(&m.url(), "c2", SHORT).unwrap_err();
        assert!(
            matches!(e, SendError::Refused { .. } | SendError::BadAnswer(_)),
            "{to}: {e:?}"
        );
        assert_eq!(m.requests().len(), 1, "{to}: only the POST");
    }
}

#[test]
fn a_chain_of_six_redirects_is_not_followed_to_its_end() {
    let replies: Vec<Reply> = (1..=7)
        .map(|n| redirect(&format!("/macros/echo?hop={n}")))
        .chain(std::iter::once(answer(200, r#"{"ok":true,"file_id":"F"}"#)))
        .collect();
    let m = Mock::start(replies);
    let e = upload(&m.url(), "c3", SHORT).unwrap_err();
    assert!(
        matches!(e, SendError::Refused { .. } | SendError::BadAnswer(_)),
        "{e:?}"
    );
    assert!(
        m.requests().len() <= 6,
        "a POST and at most five hops, saw {}",
        m.requests().len()
    );
}

#[test]
fn the_key_is_not_replayed_to_the_redirect_target() {
    let m = Mock::start(vec![
        redirect("/macros/echo?k=1"),
        answer(200, r#"{"ok":true,"file_id":"F"}"#),
    ]);
    upload(&m.url(), "c4", SHORT).unwrap();
    let seen = m.requests();
    assert!(!String::from_utf8_lossy(&seen[1].body).contains(KEY));
    assert!(seen[1].body.is_empty());
}

// ---------------------------------------------------------------- D9d

#[test]
fn nothing_listening_is_offline() {
    let port = {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let e = upload(
        &format!("http://127.0.0.1:{port}/macros/s/T/exec"),
        "d1",
        SHORT,
    )
    .unwrap_err();
    assert!(matches!(e, SendError::Offline(_)), "{e:?}");
    assert_eq!(e.code(), "offline");
    assert!(!e.permanent());
}

#[test]
fn a_relay_that_never_answers_is_offline_after_the_timeout() {
    let m = Mock::start(vec![Reply::Hang]);
    let t = Timeouts {
        connect: Duration::from_millis(500),
        total: Duration::from_millis(800),
    };
    let e = upload(&m.url(), "d2", t).unwrap_err();
    assert!(matches!(e, SendError::Offline(_)), "{e:?}");
    assert_eq!(e.code(), "offline");
}

// ---------------------------------------------------------------- D9e

const REAL: &str = "https://script.google.com/macros/s/AKfycbxExample123/exec";

#[test]
fn checked_accepts_the_real_endpoint_shape() {
    let c = Config::checked(REAL, KEY, false).expect("accepted");
    assert_eq!(c.url, REAL);
    assert_eq!(c.key, KEY);
    assert!(Config::checked(REAL, &"k".repeat(16), false).is_some());
    assert!(Config::checked(REAL, &"k".repeat(128), false).is_some());
}

#[test]
fn checked_refuses_everything_else() {
    let bad_urls = [
        "http://script.google.com/macros/s/AKf/exec",
        "https://evil.example/macros/s/AKf/exec",
        "https://script.google.com.evil.example/macros/s/AKf/exec",
        "https://script.google.com/macros/s/AKf/dev",
        "https://script.google.com/macros/s//exec",
        "https://script.google.com/macros/s/AKf/exec/more",
        "https://user@script.google.com/macros/s/AKf/exec",
        "https://script.google.com/other/s/AKf/exec",
        "not a url",
        "",
        "http://127.0.0.1:8080/macros/s/T/exec",
    ];
    for u in bad_urls {
        assert!(Config::checked(u, KEY, false).is_none(), "{u}");
    }
    let bad_keys = [
        "",
        "short8chr",
        &"k".repeat(15),
        &"k".repeat(129),
        "has space in the key!!",
        "key\nwith-newline-abcdef",
        "ключ-ключ-ключ-ключ",
    ];
    for k in bad_keys {
        assert!(Config::checked(REAL, k, false).is_none(), "{k:?}");
    }
}

#[test]
fn loopback_http_is_accepted_only_with_the_flag() {
    let u = "http://127.0.0.1:4040/macros/s/T/exec";
    assert!(Config::checked(u, KEY, false).is_none());
    assert!(Config::checked(u, KEY, true).is_some());
    // The flag does not open the door to other plain-http hosts.
    assert!(Config::checked("http://example.org/macros/s/T/exec", KEY, true).is_none());
    assert!(Config::checked("http://script.google.com/macros/s/T/exec", KEY, true).is_none());
}

#[test]
fn a_release_build_never_reads_the_endpoint_from_the_environment() {
    let baked = Some((REAL, KEY));
    let env = Some(("http://127.0.0.1:9/macros/s/T/exec", "EnvKey0123456789abcd"));
    // Release: the compiled-in pair, or nothing; the environment is ignored.
    assert_eq!(Config::resolve(baked, env, false).unwrap().url, REAL);
    assert_eq!(Config::resolve(None, env, false), None);
    // Debug: the environment overrides, and loopback http is allowed there.
    let c = Config::resolve(baked, env, true).unwrap();
    assert_eq!(c.url, "http://127.0.0.1:9/macros/s/T/exec");
    // No pair at all: sending is off.
    assert_eq!(Config::resolve(None, None, true), None);
    assert_eq!(Config::resolve(None, None, false), None);
    // A compiled-in pair is validated like any other.
    assert_eq!(Config::resolve(Some(("http://x/", KEY)), None, false), None);
}

#[test]
fn baked_is_none_when_the_build_has_no_secrets() {
    // CI and developer builds compile without the two variables.
    if option_env!("TECTORA_REPORT_URL").is_none()
        && std::env::var("TECTORA_REPORT_URL").is_err()
        && std::env::var("TECTORA_REPORT_KEY").is_err()
    {
        assert_eq!(Config::baked(), None);
    }
}

// ---------------------------------------------------------------- D16

#[test]
fn error_texts_never_contain_the_key_or_the_address() {
    let m = Mock::start(vec![answer(
        200,
        format!(r#"{{"ok":false,"error":"x {KEY}"}}"#),
    )]);
    let url = m.url();
    for e in [
        upload(&url, "e1", SHORT).unwrap_err(),
        upload("http://127.0.0.1:1/macros/s/T/exec", "e2", SHORT).unwrap_err(),
    ] {
        for text in [format!("{e:?}"), e.code().to_string()] {
            assert!(!text.contains(KEY), "{text}");
            assert!(!text.contains("127.0.0.1"), "{text}");
        }
    }
}
