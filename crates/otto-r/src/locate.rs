//! Finds Rscript: a path set in Otto's settings, R's own registry entry, or the newest
//! `C:\Program Files\R\R-x.y.z`.

use std::path::{Path, PathBuf};

/// Otto's settings file (shared with the app), `%APPDATA%\Otto\settings.json`.
pub fn settings_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("Otto").join("settings.json"))
}

/// The `r_path` setting: an R installation folder or an Rscript executable.
pub fn configured() -> Option<PathBuf> {
    let text = std::fs::read_to_string(settings_path()?).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let p = v.get("r_path")?.as_str()?.trim();
    (!p.is_empty()).then(|| PathBuf::from(p))
}

fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// Rscript inside an R installation folder (or the path itself if it is Rscript).
pub fn rscript_in(path: &Path) -> Option<PathBuf> {
    if path.is_file() {
        return Some(path.to_path_buf());
    }
    [path.join("bin").join(exe("Rscript")), path.join(exe("Rscript"))].into_iter().find(|p| p.is_file())
}

#[cfg(windows)]
fn registry_install_path() -> Option<PathBuf> {
    use std::os::windows::process::CommandExt;
    for key in [r"HKLM\SOFTWARE\R-core\R", r"HKCU\SOFTWARE\R-core\R"] {
        let out = std::process::Command::new("reg")
            .args(["query", key, "/v", "InstallPath"])
            .creation_flags(crate::CREATE_NO_WINDOW)
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&out.stdout);
        if let Some(line) = text.lines().find(|l| l.contains("InstallPath")) {
            if let Some(i) = line.find("REG_SZ") {
                let p = line[i + "REG_SZ".len()..].trim();
                if !p.is_empty() {
                    return Some(PathBuf::from(p));
                }
            }
        }
    }
    None
}

#[cfg(not(windows))]
fn registry_install_path() -> Option<PathBuf> {
    None
}

/// Version numbers of an "R-4.5.1" folder name, for picking the newest.
fn version_of(name: &str) -> Option<Vec<u32>> {
    let v = name.strip_prefix("R-")?;
    v.split('.').map(|p| p.parse().ok()).collect()
}

fn newest_in_program_files() -> Option<PathBuf> {
    let mut roots = Vec::new();
    for var in ["ProgramW6432", "ProgramFiles", "LOCALAPPDATA"] {
        if let Ok(dir) = std::env::var(var) {
            roots.push(PathBuf::from(&dir).join("R"));
            roots.push(PathBuf::from(dir).join("Programs").join("R"));
        }
    }
    let mut best: Option<(Vec<u32>, PathBuf)> = None;
    for root in roots {
        let Ok(entries) = std::fs::read_dir(&root) else { continue };
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if let (Some(v), Some(rs)) = (version_of(&name), rscript_in(&e.path())) {
                if best.as_ref().is_none_or(|(bv, _)| v > *bv) {
                    best = Some((v, rs));
                }
            }
        }
    }
    best.map(|(_, p)| p)
}

/// Rscript to use, or `None` when R is not installed (or not where Otto looks).
pub fn rscript() -> Option<PathBuf> {
    if let Some(p) = configured() {
        // An explicit setting wins, even if it is wrong: the dialog shows it.
        return rscript_in(&p);
    }
    registry_install_path().and_then(|p| rscript_in(&p)).or_else(newest_in_program_files).or_else(|| {
        // On PATH (other systems, or a portable R).
        std::env::var_os("PATH").and_then(|paths| std::env::split_paths(&paths).find_map(|d| rscript_in(&d.join(exe("Rscript")))))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_order_numerically() {
        assert!(version_of("R-4.10.0") > version_of("R-4.9.3"));
        assert_eq!(version_of("R-4.5.1"), Some(vec![4, 5, 1]));
        assert_eq!(version_of("Rtools"), None);
    }
}
