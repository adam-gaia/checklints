use crate::Cli;

use super::THIS_CRATE_NAME;
use anyhow::{bail, Result};
use log::debug;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::{env, fs};

fn default_user_checklists() -> bool {
    true
}

fn default_fail_fast() -> bool {
    false
}

pub fn write_default_config(path: &Path) -> Result<()> {
    let config = MaybeSettings::default();
    let contents = toml::to_string(&config)?;
    let mut f = File::create(path)?;
    debug!("Writing default config to {}", path.display());
    write!(f, "{contents}")?;
    Ok(())
}

#[derive(Debug)]
pub struct Settings {
    user_checklists: bool,
    fail_fast: bool,
}

impl Settings {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn builder() -> SettingsBuilder {
        SettingsBuilder::new()
    }

    pub fn user_checklists(&self) -> bool {
        self.user_checklists
    }

    pub fn fail_fast(&self) -> bool {
        self.fail_fast
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            user_checklists: default_user_checklists(),
            fail_fast: default_fail_fast(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct MaybeSettings {
    user_checklists: Option<bool>,
    fail_fast: Option<bool>,
}

impl MaybeSettings {
    fn to_settings(self) -> Result<Settings> {
        let Some(user_checklists) = self.user_checklists else {
            bail!("Settings option 'user_checklists' not set");
        };
        let Some(fail_fast) = self.fail_fast else {
            bail!("Settings option 'fail_fast' not set");
        };
        Ok(Settings {
            user_checklists,
            fail_fast,
        })
    }
}

impl MaybeSettings {
    fn empty() -> Self {
        Self {
            user_checklists: None,
            fail_fast: None,
        }
    }

    pub fn layer(&mut self, layer: Self) {
        if let Some(enable) = layer.user_checklists {
            self.user_checklists = Some(enable);
        }

        if let Some(enable) = layer.fail_fast {
            self.fail_fast = Some(enable);
        }
    }

    pub fn from_args(args: Cli) -> Self {
        let mut layer = MaybeSettings::empty();

        if args.no_user_checklists {
            layer.user_checklists = Some(false);
        }

        if args.fail_fast {
            layer.fail_fast = Some(true);
        }

        layer
    }

    pub fn from_env() -> Result<Self> {
        let mut layer = Self::empty();

        let key = "USER_CHECKLISTS";
        if let Ok(user_checklists) = env::var(prefix_key(key)) {
            layer.user_checklists = Some(true);
        }

        let key = "FAIL_FAST";
        if let Ok(fail_fast) = env::var(prefix_key(key)) {
            layer.fail_fast = Some(true);
        }

        Ok(layer)
    }
}

impl Default for MaybeSettings {
    fn default() -> Self {
        Self {
            user_checklists: Some(true),
            fail_fast: Some(false),
        }
    }
}

fn prefix_key(key: &str) -> String {
    let prefix = THIS_CRATE_NAME.to_uppercase();
    format!("{prefix}_{key}")
}

pub struct SettingsBuilder {
    settings: MaybeSettings,
}

impl SettingsBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn build(self) -> Result<Settings> {
        Ok(self.settings.to_settings()?)
    }

    pub fn env_layer(mut self) -> Result<Self> {
        let layer = MaybeSettings::from_env()?;
        self.settings.layer(layer);
        Ok(self)
    }

    pub fn config_layer(mut self, config_file: &Path) -> Result<Self> {
        let contents = fs::read_to_string(config_file)?;
        let layer: MaybeSettings = toml::from_str(&contents)?;
        self.settings.layer(layer);
        Ok(self)
    }

    pub fn arg_layer(mut self, args: Cli) -> Self {
        let layer = MaybeSettings::from_args(args);
        self.settings.layer(layer);
        self
    }

    pub fn user_checklists(mut self, enable: bool) -> Self {
        self.settings.user_checklists = Some(enable);
        self
    }

    pub fn fail_fast(mut self, enable: bool) -> Self {
        self.settings.fail_fast = Some(enable);
        self
    }
}

impl Default for SettingsBuilder {
    fn default() -> Self {
        Self {
            settings: MaybeSettings::default(),
        }
    }
}
