//! Compares two SOL files token by token, numbers to 1e-5.
//!
//! `mv_cmp A.SOL B.SOL`: prints `EQUAL (n tokens)` and exits 0, or prints
//! the first difference with the tokens around it and exits 1.

use std::process::ExitCode;

use tectora_core::modell;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        eprintln!("usage: mv_cmp A.SOL B.SOL");
        return ExitCode::from(2);
    }
    let read = |p: &str| match std::fs::read(p) {
        Ok(b) => Some(String::from_utf8_lossy(&b).into_owned()),
        Err(e) => {
            eprintln!("{p}: {e}");
            None
        }
    };
    let (Some(a), Some(b)) = (read(&args[0]), read(&args[1])) else {
        return ExitCode::from(2);
    };
    match modell::compare_sol(&a, &b, 1e-5) {
        Ok(n) => {
            println!("EQUAL ({n} tokens)");
            ExitCode::SUCCESS
        }
        Err(d) => {
            let ctx = |t: &str| {
                let v: Vec<&str> = t.split_whitespace().collect();
                let lo = d.token.saturating_sub(4);
                v[lo.min(v.len())..(d.token + 5).min(v.len())].join(" ")
            };
            println!(
                "DIFF at token {}: {} vs {}",
                d.token,
                d.a.as_deref().unwrap_or("<end>"),
                d.b.as_deref().unwrap_or("<end>")
            );
            println!("  A: {}", ctx(&a));
            println!("  B: {}", ctx(&b));
            ExitCode::from(1)
        }
    }
}
