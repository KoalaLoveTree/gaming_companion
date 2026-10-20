pub(crate) mod bindings;
pub(crate) mod config;
pub(crate) mod hotkey;
pub(crate) mod plugin;

pub use bindings::Bindings;
pub use config::AppConfig;
pub use plugin::{Page, Plugin, Resource, ResourceKind};
