//! Discover simulator resources once, before admitting jobs.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::process::Command;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Installation {
    pub schema: u32,
    pub version: String,
    pub revision: String,
    pub installed: bool,
    pub player_api: u32,
    pub web_root: PathBuf,
    pub python_enabled: bool,
    pub python: Option<PathBuf>,
    pub backend: Option<PathBuf>,
}

pub fn executable(path: &Path) -> Result<PathBuf> {
    let candidate = if path.components().count() > 1 {
        std::path::absolute(path)?
    } else {
        let search = std::env::var_os("PATH").context("PATH is not set")?;
        std::env::split_paths(&search)
            .map(|directory| directory.join(path))
            .find(|candidate| is_executable(candidate))
            .with_context(|| format!("Executable {} not found on PATH", path.display()))?
    };
    ensure!(
        is_executable(&candidate),
        "Not an executable file: {}",
        candidate.display()
    );
    Ok(std::path::absolute(candidate)?)
}
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

pub async fn discover(simulator: &Path) -> Result<Installation> {
    let output = tokio::time::timeout(
        Duration::from_secs(10),
        Command::new(simulator)
            .arg("--installation-info")
            .kill_on_drop(true)
            .output(),
    )
    .await
    .context("Simulator installation discovery timed out")??;
    ensure!(
        output.status.success(),
        "Simulator installation discovery failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    ensure!(
        output.stdout.len() <= 65536,
        "Simulator installation information is too large"
    );
    let info: Installation = serde_json::from_slice(&output.stdout)
        .context("Simulator must support --installation-info with JSON output")?;
    ensure!(
        info.schema == 1 && info.player_api == 1,
        "Unsupported simulator installation or player interface"
    );
    ensure!(
        info.web_root.is_absolute(),
        "Simulator player directory must be absolute"
    );
    if info.python_enabled {
        ensure!(
            info.python
                .as_ref()
                .is_some_and(|p| p.is_absolute() && p.is_file())
                && info
                    .backend
                    .as_ref()
                    .is_some_and(|p| p.is_absolute() && p.join("__init__.py").is_file()),
            "Simulator Python environment or backend is missing"
        );
    }
    Ok(info)
}

pub fn default_target() -> Result<PathBuf> {
    let executable = std::env::current_exe()?;
    let mut directory = executable
        .parent()
        .context("Studio executable has no parent")?;
    if directory.file_name().is_some_and(|name| name == "deps") {
        directory = directory
            .parent()
            .context("Test executable has no build directory")?;
    }
    let development = directory.join("presets/studio-ideal-v1.json");
    if development.is_file() {
        return Ok(development);
    }
    Ok(directory
        .parent()
        .context("Studio executable has no installation prefix")?
        .join("share/qsbit-studio/presets/studio-ideal-v1.json"))
}

pub fn default_work_dir() -> Result<PathBuf> {
    if let Some(cache) = std::env::var_os("XDG_CACHE_HOME").filter(|v| !v.is_empty()) {
        let cache = PathBuf::from(cache);
        ensure!(cache.is_absolute(), "XDG_CACHE_HOME must be absolute");
        return Ok(cache.join("qsbit-studio/runs"));
    }
    Ok(
        PathBuf::from(std::env::var_os("HOME").context("HOME is not set; pass --work-dir")?)
            .join(".cache/qsbit-studio/runs"),
    )
}
