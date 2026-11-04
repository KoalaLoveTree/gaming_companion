//! Pages the Plugins form writes under the user plugins directory.
//!
//! A page is one plugin with one resource. The folder id comes from the name.
//! A link is resolved before it is stored: https when that responds, otherwise
//! http, and the stored address is the one after redirects.

use std::fs;
use std::path::Path;
use std::time::Duration;

use serde::Deserialize;
use serde::Serialize;

use super::plugins::{self, MAX_TEXT_BYTES};
use crate::model::{Plugin, ResourceKind};
use crate::{Error, Result};

pub const UNPROTECTED_CONFIRM: &str =
    "This link is only available over http, which is not protected. Add it anyway?";

const PAGE_RESOURCE: &str = "page";
const NOTES_FILE: &str = "notes.md";
const NAME_LIMIT: usize = 80;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub url: String,
    pub unprotected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveBody {
    Link(String),
    Notes(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveRequest {
    pub game_id: String,
    pub game_name: String,
    pub name: String,
    pub replace_id: Option<String>,
    pub body: SaveBody,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DraftKind {
    Link(String),
    Notes(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft {
    pub game_id: String,
    pub game_name: String,
    pub name: String,
    pub replace_id: String,
    pub kind: DraftKind,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveJson {
    #[serde(default)]
    game_id: String,
    #[serde(default)]
    game_name: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    replace: Option<String>,
}

#[derive(Serialize)]
struct GameToml<'a> {
    name: &'a str,
}

#[derive(Serialize)]
struct PluginToml<'a> {
    id: &'a str,
    name: &'a str,
    entry: &'a str,
    resource: Vec<ResourceToml<'a>>,
}

#[derive(Serialize)]
struct ResourceToml<'a> {
    id: &'a str,
    title: &'a str,
    kind: &'a str,
    src: &'a str,
}

/// Addresses to try, in order. `One` is an address the user already schemed.
#[derive(Debug, PartialEq, Eq)]
pub enum Candidates {
    One(String),
    Probe { https: String, http: String },
}

pub fn candidates(input: &str) -> Result<Candidates> {
    let input = input.trim();
    if input.is_empty() {
        return Err(Error::new("Enter a link"));
    }
    if input
        .chars()
        .any(|ch| ch.is_whitespace() || ch.is_control())
    {
        return Err(Error::new("A link cannot contain spaces"));
    }
    if let Some(scheme) = explicit_scheme(input) {
        return match scheme.as_str() {
            "https" | "http" => {
                let url = normalize_scheme(input);
                if !plugins::is_remote_url(&url) {
                    return Err(Error::new("Enter a link"));
                }
                Ok(Candidates::One(url))
            }
            _ => Err(Error::new("Only http and https links can be added")),
        };
    }
    if opaque_scheme(input) {
        return Err(Error::new("Only http and https links can be added"));
    }
    let rest = input.trim_start_matches('/');
    if rest.is_empty() {
        return Err(Error::new("Enter a link"));
    }
    Ok(Candidates::Probe {
        https: format!("https://{rest}"),
        http: format!("http://{rest}"),
    })
}

/// `fetch` returns the address after redirects, or fails when nothing answered.
pub fn resolve_with(
    input: &str,
    mut fetch: impl FnMut(&str) -> std::result::Result<String, ()>,
) -> Result<Resolved> {
    let resolved = match candidates(input)? {
        Candidates::One(url) => {
            fetch(&url).map_err(|_| Error::new("This link could not be opened"))?
        }
        Candidates::Probe { https, http } => match fetch(&https) {
            Ok(url) => url,
            Err(()) => fetch(&http).map_err(|_| Error::new("This link could not be opened"))?,
        },
    };
    finish(resolved)
}

pub fn resolve_link(input: &str) -> Result<Resolved> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(12))
        .redirects(5)
        .build();
    resolve_with(input, |url| probe(&agent, url))
}

pub fn save_from_json(user_root: &Path, plugins: &[Plugin], json: &str) -> Result<String> {
    let parsed: SaveJson = serde_json::from_str(json)
        .map_err(|_| Error::new("The page could not be saved"))?;
    let body = match parsed.kind.as_str() {
        "link" => SaveBody::Link(parsed.url),
        "notes" => SaveBody::Notes(parsed.text),
        _ => return Err(Error::new("Choose a link or notes")),
    };
    let replace = parsed.replace.and_then(|id| {
        let id = id.trim().to_string();
        if id.is_empty() { None } else { Some(id) }
    });
    save_page(
        user_root,
        plugins,
        &SaveRequest {
            game_id: parsed.game_id,
            game_name: parsed.game_name,
            name: parsed.name,
            replace_id: replace,
            body,
        },
    )
}

pub fn save_page(user_root: &Path, plugins: &[Plugin], request: &SaveRequest) -> Result<String> {
    let (game_id, game_name) = game_of(plugins, &request.game_id, &request.game_name)?;
    let name = display_name(&request.name, "Enter a name")?;
    let page_id = slug(&name)?;
    if let Some(replace) = request.replace_id.as_deref() {
        let old = owned_plugin(plugins, replace)?;
        if old.game_id != game_id {
            return Err(Error::new("That page stays in its game"));
        }
        if page_id != replace && plugins.iter().any(|plugin| plugin.id == page_id) {
            return Err(Error::new("That name is already used"));
        }
    } else if plugins.iter().any(|plugin| plugin.id == page_id) {
        return Err(Error::new("That name is already used"));
    }
    let (kind, src) = match &request.body {
        SaveBody::Link(url) => {
            let url = normalize_scheme(url.trim());
            if !plugins::is_remote_url(&url) {
                return Err(Error::new("Enter a link"));
            }
            ("url", url)
        }
        SaveBody::Notes(text) => {
            if text.len() as u64 > MAX_TEXT_BYTES {
                return Err(Error::new("Notes are too long"));
            }
            if text.contains('\0') {
                return Err(Error::new("Notes cannot contain a null"));
            }
            ("markdown", NOTES_FILE.to_string())
        }
    };

    let game_dir = user_root.join(&game_id);
    let page_dir = game_dir.join(&page_id);
    fs::create_dir_all(&page_dir)?;
    let root = user_root
        .canonicalize()
        .map_err(|err| Error::new(format!("plugins directory: {err}")))?;
    let page_dir = page_dir
        .canonicalize()
        .map_err(|err| Error::new(format!("page directory: {err}")))?;
    if !page_dir.starts_with(&root) {
        return Err(Error::new("The page has to stay in the plugins directory"));
    }
    if let SaveBody::Notes(text) = &request.body {
        fs::write(page_dir.join(NOTES_FILE), text)?;
    } else if page_dir.join(NOTES_FILE).is_file() {
        fs::remove_file(page_dir.join(NOTES_FILE))?;
    }
    let manifest = PluginToml {
        id: &page_id,
        name: &name,
        entry: PAGE_RESOURCE,
        resource: vec![ResourceToml {
            id: PAGE_RESOURCE,
            title: &name,
            kind,
            src: &src,
        }],
    };
    let text = toml::to_string(&manifest).map_err(|err| Error::new(format!("plugin.toml: {err}")))?;
    fs::write(page_dir.join("plugin.toml"), text)?;
    let game_file = game_dir.join("game.toml");
    if !game_file.is_file() {
        let label = toml::to_string(&GameToml { name: &game_name })
            .map_err(|err| Error::new(format!("game.toml: {err}")))?;
        fs::write(game_file, label)?;
    }
    if let Some(replace) = request.replace_id.as_deref() {
        if replace != page_id {
            if let Some(old) = plugins.iter().find(|plugin| plugin.id == replace) {
                remove_page_dir(&root, &old.root)?;
            }
        }
    }
    Ok(page_id)
}

pub fn delete_page(user_root: &Path, plugins: &[Plugin], id: &str) -> Result<()> {
    let plugin = owned_plugin(plugins, id)?;
    let root = user_root
        .canonicalize()
        .map_err(|err| Error::new(format!("plugins directory: {err}")))?;
    remove_page_dir(&root, &plugin.root)?;
    Ok(())
}

pub fn read_draft(plugin: &Plugin) -> Result<Draft> {
    if !plugin.owned {
        return Err(Error::new("That page can't be edited here"));
    }
    if plugin.resources.len() != 1 {
        return Err(Error::new("That page can't be edited here"));
    }
    let resource = &plugin.resources[0];
    let kind = match resource.kind {
        ResourceKind::Url => DraftKind::Link(resource.src.clone()),
        ResourceKind::Markdown => {
            let path = plugins::resolve_inside(&plugin.root, &resource.src)?;
            DraftKind::Notes(plugins::read_limited(&path)?)
        }
        ResourceKind::Html => return Err(Error::new("That page can't be edited here")),
    };
    Ok(Draft {
        game_id: plugin.game_id.clone(),
        game_name: plugin.game_name.clone(),
        name: plugin.name.clone(),
        replace_id: plugin.id.clone(),
        kind,
    })
}

pub fn draft_json(draft: &Draft) -> String {
    let (kind, url, text) = match &draft.kind {
        DraftKind::Link(url) => ("link", url.as_str(), ""),
        DraftKind::Notes(text) => ("notes", "", text.as_str()),
    };
    let value = serde_json::json!({
        "gameId": draft.game_id,
        "gameName": draft.game_name,
        "name": draft.name,
        "replace": draft.replace_id,
        "kind": kind,
        "url": url,
        "text": text,
    });
    let mut out = serde_json::to_string(&value).unwrap_or_else(|_| "null".to_string());
    out = out.replace('\u{2028}', "\\u2028");
    out.replace('\u{2029}', "\\u2029")
}

fn probe(agent: &ureq::Agent, url: &str) -> std::result::Result<String, ()> {
    let response = match agent
        .get(url)
        .set(
            "User-Agent",
            "Mozilla/5.0 (compatible; GamingCompanion/0.1)",
        )
        .call()
    {
        Ok(response) => response,
        Err(ureq::Error::Status(_, response)) => response,
        Err(ureq::Error::Transport(_)) => return Err(()),
    };
    let final_url = response.get_url().to_string();
    drop(response);
    if plugins::is_remote_url(&normalize_scheme(&final_url)) {
        Ok(final_url)
    } else {
        Err(())
    }
}

fn finish(url: String) -> Result<Resolved> {
    let url = normalize_scheme(url.trim());
    if !plugins::is_remote_url(&url) {
        return Err(Error::new("This link could not be opened"));
    }
    Ok(Resolved {
        unprotected: url.starts_with("http://"),
        url,
    })
}

fn explicit_scheme(input: &str) -> Option<String> {
    let idx = input.find("://")?;
    let scheme = &input[..idx];
    if scheme.is_empty()
        || !scheme
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '+' || ch == '-' || ch == '.')
    {
        return None;
    }
    Some(scheme.to_ascii_lowercase())
}

fn opaque_scheme(input: &str) -> bool {
    let Some(idx) = input.find(':') else {
        return false;
    };
    if input[idx..].starts_with("://") {
        return false;
    }
    let scheme = &input[..idx];
    if scheme.is_empty() || !scheme.chars().all(|ch| ch.is_ascii_alphabetic()) {
        return false;
    }
    let after = &input[idx + 1..];
    let port_len = after.chars().take_while(|ch| ch.is_ascii_digit()).count();
    if port_len > 0 {
        let rest = &after[port_len..];
        if rest.is_empty() || rest.starts_with('/') {
            return false;
        }
    }
    true
}

fn normalize_scheme(input: &str) -> String {
    match input.find("://") {
        Some(idx) => {
            let mut out = input[..idx].to_ascii_lowercase();
            out.push_str(&input[idx..]);
            out
        }
        None => input.to_string(),
    }
}

fn game_of(plugins: &[Plugin], game_id: &str, game_name: &str) -> Result<(String, String)> {
    let game_id = game_id.trim();
    if !game_id.is_empty() {
        plugins::validate_id(game_id)?;
        let name = plugins
            .iter()
            .find(|plugin| plugin.game_id == game_id)
            .map(|plugin| plugin.game_name.clone())
            .or_else(|| {
                let name = game_name.trim();
                if name.is_empty() {
                    None
                } else {
                    Some(name.to_string())
                }
            })
            .ok_or_else(|| Error::new("Enter a game name"))?;
        let name = display_name(&name, "Enter a game name")?;
        return Ok((game_id.to_string(), name));
    }
    let name = display_name(game_name, "Enter a game name")?;
    let id = slug(&name).map_err(|_| Error::new("Use a game name with a letter or digit"))?;
    let name = plugins
        .iter()
        .find(|plugin| plugin.game_id == id)
        .map(|plugin| plugin.game_name.clone())
        .unwrap_or(name);
    Ok((id, name))
}

fn display_name(name: &str, empty: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::new(empty));
    }
    if name.chars().any(|ch| ch.is_control()) {
        return Err(Error::new("Use a name with a letter or digit"));
    }
    if name.chars().count() > NAME_LIMIT {
        return Err(Error::new("Use a shorter name"));
    }
    if !name.chars().any(|ch| ch.is_alphanumeric()) {
        return Err(Error::new("Use a name with a letter or digit"));
    }
    Ok(name.to_string())
}

pub fn slug(name: &str) -> Result<String> {
    let name = display_name(name, "Enter a name")?;
    let mut id = String::new();
    let mut gap = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            if gap && !id.is_empty() {
                id.push('-');
            }
            gap = false;
            id.push(ch.to_ascii_lowercase());
        } else {
            gap = true;
        }
        if id.len() >= 64 {
            break;
        }
    }
    while id.ends_with('-') {
        id.pop();
    }
    if id.is_empty() {
        id = hashed_id(&name);
    }
    plugins::validate_id(&id)?;
    Ok(id)
}

fn hashed_id(name: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in name.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("p{hash:016x}")
}

fn owned_plugin<'a>(plugins: &'a [Plugin], id: &str) -> Result<&'a Plugin> {
    let plugin = plugins
        .iter()
        .find(|plugin| plugin.id == id)
        .ok_or_else(|| Error::new("That page is not in the list"))?;
    if !plugin.owned {
        return Err(Error::new("That page can't be changed"));
    }
    Ok(plugin)
}

fn remove_page_dir(user_root: &Path, page: &Path) -> Result<()> {
    let page = page
        .canonicalize()
        .map_err(|err| Error::new(format!("page directory: {err}")))?;
    if !page.starts_with(user_root) || page == user_root {
        return Err(Error::new("That page can't be changed"));
    }
    let game = page
        .parent()
        .ok_or_else(|| Error::new("That page can't be changed"))?
        .to_path_buf();
    fs::remove_dir_all(&page)?;
    if game != user_root && game.starts_with(user_root) && game_is_empty(&game) {
        fs::remove_dir_all(&game)?;
    }
    Ok(())
}

fn game_is_empty(game: &Path) -> bool {
    let Ok(entries) = fs::read_dir(game) else {
        return false;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        if name != "game.toml" {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!("gaming-companion-page-{nanos}"));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn hit(url: &str) -> std::result::Result<String, ()> {
        Ok(url.to_string())
    }

    #[test]
    fn a_bare_address_tries_https_then_http() {
        let resolved = resolve_with("maxroll.gg/build", |url| {
            if url.starts_with("https://") {
                Err(())
            } else {
                hit(url)
            }
        })
        .unwrap();
        assert_eq!(resolved.url, "http://maxroll.gg/build");
        assert!(resolved.unprotected);

        let upgraded = resolve_with("maxroll.gg/build", hit).unwrap();
        assert_eq!(upgraded.url, "https://maxroll.gg/build");
        assert!(!upgraded.unprotected);
    }

    #[test]
    fn an_explicit_https_link_is_not_downgraded() {
        let mut calls = 0;
        let err = resolve_with("https://maxroll.gg/missing", |url| {
            calls += 1;
            assert_eq!(url, "https://maxroll.gg/missing");
            Err(())
        })
        .unwrap_err();
        assert_eq!(calls, 1);
        assert!(err.to_string().contains("could not be opened"));
    }

    #[test]
    fn an_explicit_http_link_stays_unprotected() {
        let resolved = resolve_with("HTTP://example.com/a", |url| {
            assert_eq!(url, "http://example.com/a");
            hit("http://example.com/a")
        })
        .unwrap();
        assert!(resolved.unprotected);
    }

    #[test]
    fn a_redirect_up_to_https_is_protected() {
        let resolved = resolve_with("http://example.com", |_| hit("https://example.com/")).unwrap();
        assert_eq!(resolved.url, "https://example.com/");
        assert!(!resolved.unprotected);
    }

    #[test]
    fn other_schemes_are_rejected() {
        for input in ["javascript:alert(1)", "file:///tmp/x", "ftp://example.com", "data:text/plain,hi"] {
            let err = candidates(input).unwrap_err();
            assert!(err.to_string().contains("http and https"), "{input}");
        }
        assert!(candidates("localhost:8080/map").is_ok());
    }

    #[test]
    fn a_blank_name_and_a_blank_link_are_rejected() {
        assert!(slug("   ").unwrap_err().to_string().contains("Enter a name"));
        assert!(slug("!!!").is_err());
        assert_eq!(slug("Path of Exile").unwrap(), "path-of-exile");
        assert!(slug("Лига").unwrap().starts_with('p'));
        assert!(candidates("  ").is_err());
        assert!(candidates("not a url").is_err());
    }

    #[test]
    fn save_edit_and_delete_stay_inside_the_user_directory() {
        let user = TempDir::new();
        let shipped = Plugin {
            id: "maps".to_string(),
            name: "Maps".to_string(),
            game_id: "arc-raiders".to_string(),
            game_name: "Arc Raiders".to_string(),
            entry: "page".to_string(),
            root: user.0.join("nope"),
            resources: vec![],
            owned: false,
        };
        let mut plugins = vec![shipped];
        let id = save_page(
            &user.0,
            &plugins,
            &SaveRequest {
                game_id: "arc-raiders".to_string(),
                game_name: "Arc Raiders".to_string(),
                name: "League notes".to_string(),
                replace_id: None,
                body: SaveBody::Notes("pob code".to_string()),
            },
        )
        .unwrap();
        assert_eq!(id, "league-notes");
        let shipped = TempDir::new();
        let loaded = plugins::load_library(&shipped.0, &user.0).unwrap();
        assert_eq!(loaded.plugins.len(), 1);
        assert!(loaded.plugins[0].owned);
        assert_eq!(loaded.plugins[0].game_name, "Arc Raiders");
        let draft = read_draft(&loaded.plugins[0]).unwrap();
        assert_eq!(draft.kind, DraftKind::Notes("pob code".to_string()));

        plugins = loaded.plugins;
        let renamed = save_page(
            &user.0,
            &plugins,
            &SaveRequest {
                game_id: "arc-raiders".to_string(),
                game_name: "Ignored".to_string(),
                name: "Filter".to_string(),
                replace_id: Some("league-notes".to_string()),
                body: SaveBody::Link("https://maxroll.gg/build".to_string()),
            },
        )
        .unwrap();
        assert_eq!(renamed, "filter");
        assert!(!user.0.join("arc-raiders/league-notes").exists());
        let text = fs::read_to_string(user.0.join("arc-raiders/filter/plugin.toml")).unwrap();
        assert!(text.contains("https://maxroll.gg/build"));
        assert!(!user.0.join("arc-raiders/filter/notes.md").exists());

        let outside = Plugin {
            id: "filter".to_string(),
            name: "Filter".to_string(),
            game_id: "arc-raiders".to_string(),
            game_name: "Arc Raiders".to_string(),
            entry: "page".to_string(),
            root: std::env::temp_dir(),
            resources: vec![],
            owned: true,
        };
        let err = delete_page(&user.0, &[outside], "filter").unwrap_err();
        assert!(err.to_string().contains("can't be changed"));
        assert!(user.0.join("arc-raiders/filter/plugin.toml").is_file());

        let loaded = plugins::load_library(&shipped.0, &user.0).unwrap();
        delete_page(&user.0, &loaded.plugins, "filter").unwrap();
        assert!(!user.0.join("arc-raiders").exists());

        let err = save_page(
            &user.0,
            &[],
            &SaveRequest {
                game_id: "..".to_string(),
                game_name: "Escape".to_string(),
                name: "Nope".to_string(),
                replace_id: None,
                body: SaveBody::Link("javascript:alert(1)".to_string()),
            },
        )
        .unwrap_err();
        assert!(!err.to_string().is_empty());
        assert!(fs::read_dir(&user.0).unwrap().next().is_none());
    }

    #[test]
    fn a_duplicate_name_and_a_shipped_page_are_refused() {
        let user = TempDir::new();
        let saved = save_page(
            &user.0,
            &[],
            &SaveRequest {
                game_id: String::new(),
                game_name: "Path of Exile".to_string(),
                name: "Maxroll".to_string(),
                replace_id: None,
                body: SaveBody::Link("https://maxroll.gg".to_string()),
            },
        )
        .unwrap();
        assert_eq!(saved, "maxroll");
        let plugins = plugins::load_plugins(&user.0).unwrap().plugins;
        let err = save_page(
            &user.0,
            &plugins,
            &SaveRequest {
                game_id: String::new(),
                game_name: "Path of Exile".to_string(),
                name: "Maxroll".to_string(),
                replace_id: None,
                body: SaveBody::Notes(String::new()),
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("already used"));
        let err = delete_page(
            &user.0,
            &[Plugin {
                owned: false,
                ..plugins[0].clone()
            }],
            "maxroll",
        )
        .unwrap_err();
        assert!(err.to_string().contains("can't be changed"));
    }
}
