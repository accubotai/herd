pub mod detect;
pub mod env_resolver;
pub mod local_config;
pub mod solo_config;
pub mod trust;

pub use solo_config::{ProcessConfig, ProjectConfig, SoloConfig, UiConfig};
