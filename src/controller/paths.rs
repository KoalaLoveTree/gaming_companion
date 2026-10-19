use std::path::{Path, PathBuf};

use crate::{Error, Result};

const APP_DIR: &str = "gaming-companion";

pub fn config_path() -> Result<PathBuf> {
    Ok(app_dir()?.join("config.toml"))
}

pub fn webview_data_dir() -> Result<PathBuf> {
    let dir = app_dir()?.join("webview");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Plugin folders, in order: an explicit env path, `./plugins`, plugins next to the
/// executable, then the plugins directory compiled into a dev build.
pub fn plugins_dir() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("GAMING_COMPANION_PLUGINS") {
        let path = PathBuf::from(path);
        if path.is_dir() {
            return Ok(path);
        }
        return Err(Error::new(format!(
            "GAMING_COMPANION_PLUGINS is not a directory: {}",
            path.display()
        )));
    }
    let candidates = [
        std::env::current_dir().ok().map(|dir| dir.join("plugins")),
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|dir| dir.join("plugins"))),
        Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins")),
    ];
    candidates
        .into_iter()
        .flatten()
        .find(|path| path.is_dir())
        .ok_or_else(|| Error::new("no plugins directory found"))
}

fn app_dir() -> Result<PathBuf> {
    let dir = dirs::config_dir()
        .ok_or_else(|| Error::new("could not find a user config directory"))?
        .join(APP_DIR);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn load_config(path: &Path) -> Result<crate::model::AppConfig> {
    if !path.exists() {
        return Ok(crate::model::AppConfig::default());
    }
    let text = std::fs::read_to_string(path)
        .map_err(|err| Error::new(format!("reading {}: {err}", path.display())))?;
    crate::model::AppConfig::parse(&text)
}

pub fn save_config(path: &Path, config: &crate::model::AppConfig) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = config.to_toml()?;
    std::fs::write(path, text)
        .map_err(|err| Error::new(format!("writing {}: {err}", path.display())))
}
