// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // `tectora-app mcp` runs the MCP server for AI clients without a window,
    // so the installed app is all the user needs to connect Claude & co.
    if std::env::args().nth(1).as_deref() == Some("mcp") {
        // A calculation worker opens the library it is told (ARCH-001).
        if tectora_mcp::run_worker_if_requested() {
            return;
        }
        if let Err(e) = tectora_core::store::Store::open_default().and_then(tectora_mcp::run_stdio)
        {
            eprintln!("tectora mcp: {e}");
            std::process::exit(1);
        }
        return;
    }
    tectora_app::run();
}
