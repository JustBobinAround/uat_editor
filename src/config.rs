use crate::err_msg::WithErrMsg;
use crate::test_step::TestStep;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const CONFIG_PATH: &'static str = ".config/uat_editor/config.toml";

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub templates: HashMap<String, Vec<TestStep>>,
    pub editor: String,
}

impl Default for Config {
    fn default() -> Self {
        let editor = std::env::var("EDITOR").expect("EXPECTED EDITOR VARIABLE");
        Config {
            templates: HashMap::new(),
            editor,
        }
    }
}

impl Config {
    pub fn load_config() -> Result<Config, String> {
        let home = std::env::var("HOME").with_err_msg(&"EXPECTED HOME VARIABLE")?;
        let path = format!("{}/{}", home, CONFIG_PATH);
        Ok(match std::fs::read_to_string(path) {
            Ok(content) => match toml::from_str(&content) {
                Ok(config) => config,
                Err(_) => Config::default(),
            },
            Err(_) => Config::default(),
        })
    }

    pub fn save_config(&self) -> Result<(), String> {
        let home = std::env::var("HOME").with_err_msg(&"EXPECTED HOME VARIABLE")?;
        let toml = toml::to_string(self).with_err_msg(&"Failed to serialize config to toml")?;
        let path = format!("{}/{}", home, CONFIG_PATH);
        std::fs::write(path, toml).with_err_msg(&"Failed to write config to toml")
    }
}
