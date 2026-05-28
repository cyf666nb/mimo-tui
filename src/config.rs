use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    pub thinking: bool,
    pub permission_mode: String,
    pub max_output_tokens: u32,
    pub web_search: bool,
    pub auto_routing: bool,
    pub system_prompt_extra: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: "https://api.xiaomimimo.com/v1".into(),
            model: "mimo-v2.5-pro".into(),
            thinking: true,
            permission_mode: "agent".into(),
            max_output_tokens: 16384,
            web_search: false,
            auto_routing: false,
            system_prompt_extra: String::new(),
        }
    }
}

impl Config {
    pub fn config_dir() -> PathBuf {
        dirs::home_dir().unwrap_or_default().join(".mimo-tui")
    }

    pub fn config_file() -> PathBuf {
        Self::config_dir().join("config.toml")
    }

    pub fn load() -> Self {
        let mut cfg = Self::default();
        // From file
        let path = Self::config_file();
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(file_cfg) = toml::from_str::<Config>(&content) {
                    cfg = file_cfg;
                }
            }
        }
        // Env overrides
        if let Ok(key) = std::env::var("MIMO_API_KEY") {
            cfg.api_key = key;
        }
        if let Ok(url) = std::env::var("MIMO_BASE_URL") {
            cfg.base_url = url;
        }
        if let Ok(model) = std::env::var("MIMO_MODEL") {
            cfg.model = model;
        }
        cfg
    }

    pub fn save(&self) {
        let dir = Self::config_dir();
        if let Err(e) = std::fs::create_dir_all(&dir) {
            eprintln!("Warning: Could not create config dir: {}", e);
            return;
        }
        let path = Self::config_file();
        match toml::to_string_pretty(self) {
            Ok(content) => {
                if let Err(e) = std::fs::write(&path, &content) {
                    eprintln!("Warning: Could not write config: {}", e);
                }
                // Set permissions to 0600 (owner read/write only) for API key safety
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
                }
            }
            Err(e) => eprintln!("Warning: Could not serialize config: {}", e),
        }
    }
}
