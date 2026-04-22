use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default)]
    pub system_prompt: Option<String>,
    #[serde(default)]
    pub max_iterations: Option<u32>,
}

fn default_model() -> String {
    std::env::var("PI_MODEL").unwrap_or_else(|_| "claude-sonnet-4-6".to_string())
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            model: default_model(),
            system_prompt: None,
            max_iterations: None,
        }
    }
}

impl Settings {
    /// Load `~/.pi/settings.json` and overlay `./.pi/settings.json` if present.
    pub fn load() -> anyhow::Result<Self> {
        let mut s = Settings::default();
        if let Some(home) = dirs_home() {
            let global = home.join(".pi").join("settings.json");
            if global.exists() {
                let bytes = std::fs::read(&global)?;
                s = serde_json::from_slice(&bytes)?;
            }
        }
        let project = std::env::current_dir()?.join(".pi").join("settings.json");
        if project.exists() {
            let bytes = std::fs::read(&project)?;
            let override_: Settings = serde_json::from_slice(&bytes)?;
            s.model = override_.model;
            if override_.system_prompt.is_some() {
                s.system_prompt = override_.system_prompt;
            }
            if override_.max_iterations.is_some() {
                s.max_iterations = override_.max_iterations;
            }
        }
        Ok(s)
    }
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}
