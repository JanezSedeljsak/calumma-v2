use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const FILE_NAME: &str = "prefs.toml";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ShellPrefs {
    #[serde(default = "default_theme")]
    pub theme: u8,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_active_project_id: Option<String>,
    #[serde(default = "default_layers_panel_open")]
    pub layers_panel_open: bool,
}

fn default_theme() -> u8 {
    1
}

fn default_language() -> String {
    "en".to_string()
}

fn default_layers_panel_open() -> bool {
    true
}

impl Default for ShellPrefs {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            language: default_language(),
            last_active_project_id: None,
            layers_panel_open: default_layers_panel_open(),
        }
    }
}

impl ShellPrefs {
    pub fn load() -> Self {
        match prefs_path() {
            Ok(path) if path.is_file() => fs::read_to_string(&path)
                .ok()
                .and_then(|text| toml::from_str(&text).ok())
                .unwrap_or_default(),
            _ => Self::default(),
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = prefs_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        }
        let text = toml::to_string_pretty(self).context("serializing prefs.toml")?;
        fs::write(&path, text).with_context(|| format!("writing {}", path.display()))
    }

    pub fn is_dark(&self) -> bool {
        self.theme != 0
    }

    pub fn set_theme_dark(&mut self, dark: bool) {
        self.theme = if dark { 1 } else { 0 };
    }

    pub fn set_last_active_project(&mut self, id: Option<&str>) {
        self.last_active_project_id = id.map(str::to_string);
    }
}

fn prefs_path() -> Result<PathBuf> {
    let base = dirs::config_dir().context("resolving the OS config directory")?;
    Ok(base.join("Calumma").join(FILE_NAME))
}
