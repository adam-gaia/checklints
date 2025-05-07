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

fn default_no_read_cache() -> bool {
    false
}

fn default_no_write_cache() -> bool {
    false
}

fn default_no_cache() -> bool {
    false
}

fn default_clear_cache() -> bool {
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
    no_read_cache: bool,
    no_write_cache: bool,
    clear_cache: bool,
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

    pub fn no_read_cache(&self) -> bool {
        self.no_read_cache
    }

    pub fn no_write_cache(&self) -> bool {
        self.no_write_cache
    }

    pub fn clear_cache(&self) -> bool {
        self.clear_cache
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            user_checklists: default_user_checklists(),
            fail_fast: default_fail_fast(),
            no_read_cache: default_no_read_cache(),
            no_write_cache: default_no_write_cache(),
            clear_cache: default_clear_cache(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct MaybeSettings {
    user_checklists: Option<bool>,
    fail_fast: Option<bool>,
    no_read_cache: Option<bool>,
    no_write_cache: Option<bool>,
    no_cache: Option<bool>,
    clear_cache: Option<bool>,
}

impl MaybeSettings {
    fn to_settings(self) -> Result<Settings> {
        let Some(user_checklists) = self.user_checklists else {
            bail!("Settings option 'user_checklists' not set");
        };
        let Some(fail_fast) = self.fail_fast else {
            bail!("Settings option 'fail_fast' not set");
        };

        let (no_read_cache, no_write_cache) = match self.no_cache {
            Some(no_cache) => {
                // No cache implies no_read and no_write
                if no_cache {
                    (true, true)
                } else {
                    let Some(no_read_cache) = self.no_read_cache else {
                        bail!("Settings option 'no_read_cache' not set");
                    };
                    let Some(no_write_cache) = self.no_write_cache else {
                        bail!("Settings option 'no_write_cache' not set");
                    };
                    (no_read_cache, no_write_cache)
                }
            }
            None => {
                let Some(no_read_cache) = self.no_read_cache else {
                    bail!("Settings option 'no_read_cache' not set");
                };
                let Some(no_write_cache) = self.no_write_cache else {
                    bail!("Settings option 'no_write_cache' not set");
                };
                (no_read_cache, no_write_cache)
            }
        };

        let Some(clear_cache) = self.clear_cache else {
            bail!("Settings option 'clear_cache' not set");
        };
        Ok(Settings {
            user_checklists,
            fail_fast,
            no_read_cache,
            no_write_cache,
            clear_cache,
        })
    }
}

impl MaybeSettings {
    fn empty() -> Self {
        Self {
            user_checklists: None,
            fail_fast: None,
            no_read_cache: None,
            no_write_cache: None,
            no_cache: None,
            clear_cache: None,
        }
    }

    pub fn layer(&mut self, layer: Self) {
        if let Some(enable) = layer.user_checklists {
            self.user_checklists = Some(enable);
        }

        if let Some(enable) = layer.fail_fast {
            self.fail_fast = Some(enable);
        }

        if let Some(enable) = layer.no_read_cache {
            self.no_read_cache = Some(enable);
        }

        if let Some(enable) = layer.no_read_cache {
            self.no_read_cache = Some(enable);
        }

        if let Some(enable) = layer.no_write_cache {
            self.no_write_cache = Some(enable);
        }

        if let Some(enable) = layer.no_cache {
            self.no_cache = Some(enable);
        }

        if let Some(enable) = layer.clear_cache {
            self.clear_cache = Some(enable);
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

        if args.no_write_cache {
            layer.no_write_cache = Some(true);
        }

        if args.no_read_cache {
            layer.no_read_cache = Some(true);
        }

        if args.no_cache {
            layer.no_cache = Some(true);
        }

        if args.clear_cache {
            layer.clear_cache = Some(true);
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

        let key = "NO_CACHE";
        if let Ok(no_cache) = env::var(prefix_key(key)) {
            layer.no_cache = Some(true);
        }

        let key = "NO_READ_CACHE";
        if let Ok(no_read_cache) = env::var(prefix_key(key)) {
            layer.no_read_cache = Some(true);
        }

        let key = "NO_WRITE_CACHE";
        if let Ok(no_write_cache) = env::var(prefix_key(key)) {
            layer.no_write_cache = Some(true);
        }

        let key = "CLEAR_CACHE";
        if let Ok(clear_cache) = env::var(prefix_key(key)) {
            layer.clear_cache = Some(true);
        }

        Ok(layer)
    }
}

impl Default for MaybeSettings {
    fn default() -> Self {
        Self {
            user_checklists: Some(default_user_checklists()),
            fail_fast: Some(default_fail_fast()),
            no_read_cache: Some(default_no_read_cache()),
            no_write_cache: Some(default_no_write_cache()),
            no_cache: Some(default_no_cache()),
            clear_cache: Some(default_clear_cache()),
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

    pub fn no_read_cache(mut self, enable: bool) -> Self {
        self.settings.no_read_cache = Some(enable);
        self
    }

    pub fn no_write_cache(mut self, enable: bool) -> Self {
        self.settings.no_write_cache = Some(enable);
        self
    }

    pub fn no_cache(mut self, enable: bool) -> Self {
        self.settings.no_cache = Some(enable);
        self
    }

    pub fn clear_cache(mut self, enable: bool) -> Self {
        self.settings.clear_cache = Some(enable);
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
