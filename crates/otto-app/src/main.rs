//! Otto: a Prism-style plotting app.
//!
//! `otto [file]`         opens the app (optionally a .otto project or a data file)
//! `otto mcp`            MCP server on stdio for Claude (bridges to the running app, else headless)
//! `otto mcp --headless` always headless
// Otto is a windowed app: it never owns a console, in any build. Set OTTO_LOG to write the
// log to a file instead (OTTO_LOG=path, or OTTO_LOG=1 for otto.log beside the executable).
#![windows_subsystem = "windows"]

use std::path::PathBuf;

/// Where tracing writes: a file when OTTO_LOG asks for one, else nowhere (the GUI has no
/// console to write to).
fn log_file() -> Option<std::fs::File> {
    let want = std::env::var("OTTO_LOG").ok().filter(|v| !v.is_empty())?;
    let path = match want.as_str() {
        "1" | "true" | "yes" => std::env::current_exe().ok()?.with_file_name("otto.log"),
        other => PathBuf::from(other),
    };
    std::fs::File::create(path).ok()
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("mcp") {
        // stdout belongs to the MCP protocol; log to stderr only.
        tracing_subscriber::fmt()
            .with_writer(std::io::stderr)
            .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()))
            .init();
        let headless = args.iter().any(|a| a == "--headless");
        let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
        return rt.block_on(otto_mcp::run_stdio(headless));
    }

    if let Some(file) = log_file() {
        tracing_subscriber::fmt()
            .with_writer(file)
            .with_ansi(false)
            .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()))
            .init();
    }
    otto_app::run_gui(args.first().map(PathBuf::from))?;
    Ok(())
}
