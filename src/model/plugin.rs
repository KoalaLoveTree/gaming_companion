use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResourceKind {
    Url,
    Html,
    Markdown,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Resource {
    pub id: String,
    pub title: String,
    pub kind: ResourceKind,
    pub src: String,
    #[serde(default)]
    pub inject: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plugin {
    pub id: String,
    pub name: String,
    /// Folder that groups this plugin with the others for the same game.
    /// A plugin that sits directly in the plugins directory uses its own id.
    pub game_id: String,
    pub game_name: String,
    pub entry: String,
    pub root: std::path::PathBuf,
    pub resources: Vec<Resource>,
}

impl Plugin {
    pub fn resource(&self, id: &str) -> Option<&Resource> {
        self.resources.iter().find(|resource| resource.id == id)
    }
}

/// What the webview should show for one resource.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub title: String,
    pub url: Option<String>,
    pub html: Option<String>,
    pub init_script: Option<String>,
}
