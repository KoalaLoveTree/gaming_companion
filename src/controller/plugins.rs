use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;

use crate::model::{Plugin, Resource, ResourceKind};
use crate::{Error, Result};

const MAX_TEXT_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug)]
pub struct LoadOutcome {
    pub plugins: Vec<Plugin>,
    pub errors: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct PluginFile {
    id: String,
    name: String,
    entry: String,
    #[serde(default)]
    resource: Vec<Resource>,
}

pub fn load_plugins(dir: &Path) -> Result<LoadOutcome> {
    let mut plugins = Vec::new();
    let mut errors = Vec::new();
    let mut entries = fs::read_dir(dir)
        .map_err(|err| Error::new(format!("reading {}: {err}", dir.display())))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let manifest = path.join("plugin.toml");
        if !manifest.is_file() {
            continue;
        }
        match load_plugin(&path) {
            Ok(plugin) => {
                if plugins.iter().any(|loaded: &Plugin| loaded.id == plugin.id) {
                    errors.push(format!(
                        "{}: duplicate plugin id '{}'",
                        path.display(),
                        plugin.id
                    ));
                } else {
                    plugins.push(plugin);
                }
            }
            Err(err) => errors.push(format!("{}: {err}", path.display())),
        }
    }

    plugins.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(LoadOutcome { plugins, errors })
}

pub fn load_plugin(dir: &Path) -> Result<Plugin> {
    let root = dir
        .canonicalize()
        .map_err(|err| Error::new(format!("plugin directory: {err}")))?;
    let manifest = root.join("plugin.toml");
    let text = read_limited(&manifest)?;
    let file: PluginFile =
        toml::from_str(&text).map_err(|err| Error::new(format!("plugin.toml: {err}")))?;
    validate_id(&file.id)?;
    if file.name.trim().is_empty() {
        return Err(Error::new("plugin name is empty"));
    }
    if file.resource.is_empty() {
        return Err(Error::new("plugin has no resources"));
    }
    let mut seen = Vec::new();
    for resource in &file.resource {
        validate_id(&resource.id)?;
        if seen.contains(&resource.id) {
            return Err(Error::new(format!(
                "duplicate resource id '{}'",
                resource.id
            )));
        }
        seen.push(resource.id.clone());
        check_resource(&root, resource)?;
    }
    if !seen.iter().any(|id| id == &file.entry) {
        return Err(Error::new(format!(
            "entry '{}' does not match a resource",
            file.entry
        )));
    }
    Ok(Plugin {
        id: file.id,
        name: file.name,
        entry: file.entry,
        root,
        resources: file.resource,
    })
}

pub fn choose<'a>(
    plugins: &'a [Plugin],
    config: &crate::model::AppConfig,
) -> Result<(&'a Plugin, &'a Resource)> {
    if plugins.is_empty() {
        return Err(Error::new("no plugins found"));
    }
    let plugin = match plugins
        .iter()
        .find(|plugin| plugin.id == config.active_plugin)
    {
        Some(plugin) => plugin,
        None => {
            let fallback = &plugins[0];
            eprintln!(
                "gaming_companion: saved plugin '{}' is not installed, opening '{}'",
                config.active_plugin, fallback.id
            );
            fallback
        }
    };
    let wanted = config
        .active_resource
        .as_deref()
        .unwrap_or(plugin.entry.as_str());
    if let Some(resource) = plugin.resource(wanted) {
        return Ok((plugin, resource));
    }
    eprintln!(
        "gaming_companion: resource '{wanted}' is missing from '{}', opening '{}'",
        plugin.id, plugin.entry
    );
    plugin
        .resource(&plugin.entry)
        .map(|resource| (plugin, resource))
        .ok_or_else(|| Error::new(format!("plugin '{}' has no entry resource", plugin.id)))
}

fn check_resource(root: &Path, resource: &Resource) -> Result<()> {
    match resource.kind {
        ResourceKind::Url => {
            if !is_remote_url(&resource.src) {
                return Err(Error::new(format!(
                    "resource '{}' url must start with http:// or https://",
                    resource.id
                )));
            }
        }
        ResourceKind::Html | ResourceKind::Markdown => {
            resolve_inside(root, &resource.src)?;
        }
    }
    for inject in &resource.inject {
        let path = resolve_inside(root, inject)?;
        if path.extension().and_then(|ext| ext.to_str()) != Some("css") {
            return Err(Error::new(format!(
                "inject file must be .css: {}",
                path.display()
            )));
        }
    }
    Ok(())
}

pub fn resolve_inside(root: &Path, relative: &str) -> Result<PathBuf> {
    if relative.is_empty() || relative.contains('\0') {
        return Err(Error::new("resource path is empty"));
    }
    let relative_path = Path::new(relative);
    if relative_path.is_absolute()
        || relative_path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(Error::new(format!(
            "resource path must stay inside the plugin folder: {relative}"
        )));
    }
    let joined = root.join(relative_path);
    let resolved = joined
        .canonicalize()
        .map_err(|err| Error::new(format!("{}: {err}", joined.display())))?;
    if !resolved.starts_with(root) {
        return Err(Error::new(format!(
            "resource path must stay inside the plugin folder: {relative}"
        )));
    }
    Ok(resolved)
}

pub fn read_limited(path: &Path) -> Result<String> {
    let meta =
        fs::metadata(path).map_err(|err| Error::new(format!("{}: {err}", path.display())))?;
    if meta.len() > MAX_TEXT_BYTES {
        return Err(Error::new(format!(
            "{} is larger than {MAX_TEXT_BYTES} bytes",
            path.display()
        )));
    }
    fs::read_to_string(path).map_err(|err| Error::new(format!("{}: {err}", path.display())))
}

fn is_remote_url(src: &str) -> bool {
    let Some(rest) = src
        .strip_prefix("https://")
        .or_else(|| src.strip_prefix("http://"))
    else {
        return false;
    };
    !rest.is_empty() && !rest.chars().any(char::is_whitespace)
}

fn validate_id(id: &str) -> Result<()> {
    let mut chars = id.chars();
    let Some(first) = chars.next() else {
        return Err(Error::new("id is empty"));
    };
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return Err(Error::new(format!(
            "id '{id}' must start with a letter or digit"
        )));
    }
    if id.len() > 64
        || chars
            .any(|ch| !ch.is_ascii_lowercase() && !ch.is_ascii_digit() && ch != '-' && ch != '_')
    {
        return Err(Error::new(format!(
            "id '{id}' may contain only lowercase letters, digits, '-' and '_'"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::AppConfig;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!("gaming-companion-{nanos}"));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn write_plugin(root: &Path, manifest: &str) {
        fs::create_dir_all(root).unwrap();
        fs::write(root.join("plugin.toml"), manifest).unwrap();
    }

    #[test]
    fn loads_a_url_plugin_and_skips_a_broken_folder() {
        let dir = TempDir::new();
        write_plugin(
            &dir.0.join("notes"),
            r#"
id = "notes"
name = "Notes"
entry = "pad"
[[resource]]
id = "pad"
title = "Pad"
kind = "markdown"
src = "pad.md"
"#,
        );
        fs::write(dir.0.join("notes/pad.md"), "# Hi\n").unwrap();
        write_plugin(
            &dir.0.join("broken"),
            "id = \"broken\"\nname = \"Broken\"\nentry = \"missing\"\n",
        );
        fs::create_dir_all(dir.0.join("not-a-plugin")).unwrap();

        let loaded = load_plugins(&dir.0).unwrap();
        assert_eq!(loaded.plugins.len(), 1);
        assert_eq!(loaded.plugins[0].id, "notes");
        assert_eq!(loaded.errors.len(), 1);
    }

    #[test]
    fn rejects_a_path_that_leaves_the_plugin_folder() {
        let dir = TempDir::new();
        write_plugin(
            &dir.0.join("escape"),
            r#"
id = "escape"
name = "Escape"
entry = "page"
[[resource]]
id = "page"
title = "Page"
kind = "html"
src = "../outside.html"
"#,
        );
        let err = load_plugin(&dir.0.join("escape")).unwrap_err();
        assert!(err.to_string().contains("inside the plugin folder"));
    }

    #[test]
    fn a_sample_plugin_opens_its_url_resource() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata");
        let loaded = load_plugins(&dir).unwrap();
        assert!(loaded.errors.is_empty(), "{:?}", loaded.errors);
        let config = AppConfig {
            active_plugin: "sample".to_string(),
            ..AppConfig::default()
        };
        let (plugin, resource) = choose(&loaded.plugins, &config).unwrap();
        assert_eq!(plugin.id, "sample");
        assert_eq!(resource.id, "page");
        assert_eq!(resource.kind, ResourceKind::Url);
    }
}
