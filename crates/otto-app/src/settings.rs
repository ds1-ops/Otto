//! App preferences that persist between launches, in `%APPDATA%\Otto\settings.json`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

fn yes() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    /// Let Claude see and drive the app through MCP.
    #[serde(default = "yes")]
    pub mcp_enabled: bool,
    /// Where R is (an installation folder or Rscript.exe); `None` finds it automatically.
    /// The statistics engine reads this from the same file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r_path: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { mcp_enabled: true, r_path: None }
    }
}

/// The settings file next to the MCP discovery file.
pub fn default_path() -> Option<PathBuf> {
    otto_mcp::discovery_path().and_then(|p| p.parent().map(|d| d.join("settings.json")))
}

impl Settings {
    /// Reads the settings; a missing or unreadable file gives the defaults.
    pub fn load(path: Option<&Path>) -> Self {
        path.and_then(|p| std::fs::read_to_string(p).ok()).and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self).unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_defaults() {
        let dir = std::env::temp_dir().join(format!("otto-settings-{}", std::process::id()));
        let path = dir.join("settings.json");
        assert_eq!(Settings::load(Some(&path)), Settings::default());
        let off = Settings { mcp_enabled: false, r_path: Some("C:/R".into()) };
        off.save(&path).unwrap();
        assert_eq!(Settings::load(Some(&path)), off);
        std::fs::write(&path, "{}").unwrap();
        assert!(Settings::load(Some(&path)).mcp_enabled);
        let _ = std::fs::remove_dir_all(dir);
    }
}
