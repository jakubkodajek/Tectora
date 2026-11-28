//! Release gate: exit 0 only when `TECTORA_REPORT_URL` and
//! `TECTORA_REPORT_KEY` pass the rules the app applies (`Config::checked`,
//! no loopback). Prints which of the two is wrong, never a value.

use tectora_relay::Config;

fn main() {
    let url = std::env::var("TECTORA_REPORT_URL").unwrap_or_default();
    let key = std::env::var("TECTORA_REPORT_KEY").unwrap_or_default();
    if Config::checked(&url, &key, false).is_some() {
        println!("report relay configuration: ok");
        return;
    }
    let url_ok = Config::checked(&url, "k".repeat(16).as_str(), false).is_some();
    if url_ok {
        eprintln!("report relay configuration: the key is not valid (16 to 128 letters, digits, - and _)");
    } else {
        eprintln!("report relay configuration: the address is not a Web app /exec address");
    }
    std::process::exit(1);
}
