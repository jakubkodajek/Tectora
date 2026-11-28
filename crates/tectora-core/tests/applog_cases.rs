//! Rotating app log (`tectora_core::applog`), plan `target/work/bug-report`,
//! acceptance criterion 8. Written before the module exists. The log folder
//! comes from `TECTORA_LOG_DIR`, read at each call; every test holds `ENV`
//! because they change that variable (this binary is the only place that does).

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use tectora_core::applog;
use tectora_core::store::now_ms;

static ENV: Mutex<()> = Mutex::new(());

/// Points the log at a fresh folder; the guard keeps other tests out.
fn fresh(name: &str) -> (std::sync::MutexGuard<'static, ()>, PathBuf) {
    let guard = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!(
        "tectora-applog-{name}-{}-{}",
        std::process::id(),
        now_ms()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    unsafe { std::env::set_var("TECTORA_LOG_DIR", &dir) };
    applog::init("app");
    (guard, dir)
}

#[test]
fn lines_are_appended_to_the_role_file_with_a_start_line() {
    let (_g, dir) = fresh("append");
    applog::line("INFO", "first-marker");
    applog::line("WARN", "second-marker");
    let text = fs::read_to_string(dir.join("app.log")).unwrap();
    let a = text.find("first-marker").unwrap();
    let b = text.find("second-marker").unwrap();
    assert!(a < b);
    assert!(text.contains("INFO") && text.contains("WARN"));
    // The start line carries the version.
    assert!(
        text.lines()
            .next()
            .unwrap()
            .contains(env!("CARGO_PKG_VERSION")),
        "{text}"
    );
    assert_eq!(applog::log_dir(), dir);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn the_log_folder_follows_the_config_folder_when_no_log_folder_is_set() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let cfg = std::env::temp_dir().join(format!("tectora-applog-cfg-{}", std::process::id()));
    unsafe {
        std::env::remove_var("TECTORA_LOG_DIR");
        std::env::set_var("TECTORA_CONFIG_DIR", &cfg);
    }
    assert_eq!(applog::log_dir(), cfg.join("logs"));
    unsafe { std::env::set_var("TECTORA_LOG_DIR", cfg.join("own")) };
    assert_eq!(applog::log_dir(), cfg.join("own"));
    unsafe {
        std::env::remove_var("TECTORA_LOG_DIR");
        std::env::remove_var("TECTORA_CONFIG_DIR");
    }
}

#[test]
fn a_line_is_one_short_line_without_control_characters() {
    let (_g, dir) = fresh("sanitize");
    applog::line("INFO", "a\nb\r\u{1b}[31mc\u{0}d\u{202e}e");
    applog::line("INFO", &format!("long-{}", "x".repeat(5000)));
    let text = fs::read_to_string(dir.join("app.log")).unwrap();
    assert!(!text.contains('\u{1b}') && !text.contains('\r') && !text.contains('\0'));
    assert!(!text.contains('\u{202e}'));
    // "a\nb" did not become two log lines.
    assert!(
        !text.lines().any(|l| l == "b" || l.starts_with("b")),
        "{text}"
    );
    let longest = text
        .lines()
        .map(|l| l.chars().filter(|c| *c == 'x').count())
        .max()
        .unwrap();
    assert!((1..=2000).contains(&longest), "{longest}");
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn the_file_rotates_at_the_size_limit_keeping_one_old_file() {
    let (_g, dir) = fresh("rotate");
    let filler = "r".repeat(1500);
    for i in 0..1200 {
        applog::line("INFO", &format!("rotate-{i} {filler}"));
    }
    let now = fs::metadata(dir.join("app.log")).unwrap().len();
    assert!(now <= 600 * 1024, "app.log is {now} bytes");
    assert!(
        dir.join("app.1.log").is_file(),
        "{:?}",
        fs::read_dir(&dir).unwrap().count()
    );
    assert!(!dir.join("app.2.log").exists());
    // The newest line is in the current file.
    assert!(
        fs::read_to_string(dir.join("app.log"))
            .unwrap()
            .contains("rotate-1199 ")
    );
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn tail_returns_whole_lines_within_the_asked_bytes_and_lines() {
    let (_g, dir) = fresh("tail");
    for i in 0..100 {
        applog::line("INFO", &format!("tail-line-{i:03}"));
    }
    let t = applog::tail("app", 300, 400);
    assert!(t.len() <= 300, "{}", t.len());
    assert!(t.trim_end().ends_with("tail-line-099"), "{t}");
    for l in t.lines() {
        assert!(
            l.contains("tail-line-") && l.ends_with(|c: char| c.is_ascii_digit()),
            "cut line {l:?}"
        );
        assert!(l.len() > 15);
    }
    let five = applog::tail("app", 1 << 20, 5);
    assert_eq!(five.lines().count(), 5);
    assert!(five.trim_end().ends_with("tail-line-099"));
    assert_eq!(applog::tail("mcp", 1 << 20, 5), "");
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn a_log_folder_that_cannot_be_created_makes_every_call_a_no_op() {
    let (_g, dir) = fresh("nolog");
    let blocker = dir.join("file");
    fs::write(&blocker, "x").unwrap();
    unsafe { std::env::set_var("TECTORA_LOG_DIR", blocker.join("sub")) };
    applog::init("app");
    applog::line("INFO", "nobody-reads-this");
    applog::install_panic_hook();
    let r = std::panic::catch_unwind(|| panic!("hook-with-no-log-folder"));
    assert!(r.is_err());
    assert_eq!(applog::tail("app", 1000, 10), "");
    assert!(blocker.is_file(), "the file is left alone");
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn the_panic_hook_logs_message_and_location_and_counts_repeats() {
    let (_g, dir) = fresh("hook");
    applog::install_panic_hook();
    let r = std::panic::catch_unwind(|| panic!("boom-marker-7"));
    assert!(r.is_err());
    let text = fs::read_to_string(dir.join("app.log")).unwrap();
    let line = text
        .lines()
        .find(|l| l.contains("boom-marker-7"))
        .expect(&text);
    assert!(line.contains("applog_cases.rs"), "location missing: {line}");
    // The same panic again and again is one line plus a count.
    for _ in 0..5 {
        let _ = std::panic::catch_unwind(|| panic!("repeat-marker-9"));
    }
    let text = fs::read_to_string(dir.join("app.log")).unwrap();
    let n = text
        .lines()
        .filter(|l| l.contains("repeat-marker-9"))
        .count();
    assert!((1..=2).contains(&n), "{n} lines: {text}");
    let _ = fs::remove_dir_all(dir);
}
