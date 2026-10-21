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
///
/// A remote resource is [`Page::Url`]. Local HTML and rendered Markdown are
/// [`Page::Html`], with any injected CSS already in the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Page {
    Url {
        title: String,
        url: String,
        /// Document-start script that injects this resource's CSS.
        init_script: Option<String>,
    },
    Html {
        title: String,
        html: String,
        /// Document-start script that injects this resource's CSS.
        /// Local documents also embed that CSS in `html`.
        init_script: Option<String>,
    },
}

impl Page {
    pub fn title(&self) -> &str {
        match self {
            Page::Url { title, .. } | Page::Html { title, .. } => title,
        }
    }
}
