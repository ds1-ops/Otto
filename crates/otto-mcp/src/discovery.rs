//! The running app advertises its MCP endpoint in a small JSON file so the
//! `otto mcp` stdio bridge can find it: `~/.otto/mcp.json`.
//!
//! Not under `%APPDATA%`, where it used to live. Claude desktop is a packaged (MSIX) app,
//! and a packaged process writing into AppData gets a private copy inside the package
//! instead - `…\Packages\Claude_*\LocalCache\Roaming\Otto\mcp.json`. From then on the
//! bridge, which runs as Claude's child, reads that copy, while the Otto window writes the
//! real file: the bridge keeps offering a token the window stopped using and every call
//! comes back 401, with the live window sitting right there. The home directory is not
//! redirected, so both sides see one file.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Discovery {
    pub url: String,
    pub token: String,
    pub pid: u32,
}

pub fn discovery_path() -> Option<PathBuf> {
    dirs::home_dir().map(|d| d.join(".otto").join("mcp.json"))
}

/// Where the file used to live, still read so an older Otto window is found.
fn legacy_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("Otto").join("mcp.json"))
}

impl Discovery {
    pub fn read() -> Option<Self> {
        let at = |p: Option<PathBuf>| serde_json::from_str(&std::fs::read_to_string(p?).ok()?).ok();
        at(discovery_path()).or_else(|| at(legacy_path()))
    }

    pub fn write(&self) -> std::io::Result<()> {
        let path = discovery_path().ok_or_else(|| std::io::Error::other("no home directory"))?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self).unwrap_or_default())?;
        // Nothing may fall back to the old file once this one exists: a packaged Claude
        // has its own copy of it and would go on reading that one for ever.
        if let Some(p) = legacy_path() {
            let _ = std::fs::remove_file(p);
        }
        Ok(())
    }

    /// Removes the file if it still points at this process.
    pub fn remove_if_ours(&self) {
        if Discovery::read().as_ref() == Some(self) {
            for p in [discovery_path(), legacy_path()].into_iter().flatten() {
                let _ = std::fs::remove_file(p);
            }
        }
    }
}

/// A 128-bit random token (hex) from the OS-seeded hasher keys.
pub fn random_token() -> String {
    let mut out = String::new();
    for i in 0..2u64 {
        let mut h = RandomState::new().build_hasher();
        h.write_u64(i);
        h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos());
        h.write_u32(std::process::id());
        out.push_str(&format!("{:016x}", h.finish()));
    }
    out
}
