use anyhow::Error;
use anyhow::{bail, Result};
use clap::Parser;
use derive_more::Display;
use different::DiffSettings;
use directories::ProjectDirs;
use log::debug;
use log::error;
use log::info;
use log::warn;
use minijinja::path_loader;
use minijinja::Environment;
use project::Project;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::env;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
mod cache;
mod settings;
use settings::{write_default_config, Settings};
mod command;
mod project;
mod types;

const THIS_CRATE_NAME: &str = env!("CARGO_PKG_NAME");
const INDENT: &str = "    ";
const CONFIG_FILE_NAME: &str = "config.toml";

#[derive(Parser)]
struct Cli {
    /// Display more output
    #[clap(short, long)]
    verbose: bool,

    /// Additional checklist files (or directories of) to use
    #[clap(short, long = "check", value_name = "CHECK_FILE")]
    checks: Vec<PathBuf>,

    /// Directory of project to auit
    #[clap(value_name = "PROJECT_DIR")]
    project_dir: Option<PathBuf>,

    /// Do not try to read from cache
    #[clap(short, long)]
    no_cache: bool,

    /// Delete cache file before running
    #[clap(long)]
    clear_cache: bool,

    /// Do not use user-wide checklists from ~/.config/checklist
    #[clap(long)]
    no_user_checklists: bool,

    /// Stop after the first failure
    /// (Default behavior is to run all checks, even if a previous check has failed)
    #[clap(long)]
    fail_fast: bool,
}

fn main() -> Result<()> {
    env_logger::init();
    let args = Cli::parse();

    let Some(proj_dirs) = ProjectDirs::from("", "", THIS_CRATE_NAME) else {
        bail!("Unable to get XDG project dirs");
    };

    let config_dir = proj_dirs.config_dir();
    if !config_dir.is_dir() {
        fs::create_dir_all(&config_dir)?;
    }

    let cache_dir = proj_dirs.cache_dir();
    if !cache_dir.is_file() {
        fs::create_dir_all(&cache_dir)?;
    }

    let project_dir = match args.project_dir {
        Some(ref project_dir) => project_dir,
        None => &env::current_dir()?,
    };
    let project_dir = project_dir.canonicalize()?;

    let config_file = config_dir.join(CONFIG_FILE_NAME);
    if !config_file.is_file() {
        write_default_config(&config_file)?;
    }

    let user_checklists_dir = config_dir.join("checklists");
    let user_templates_dir = config_dir.join("templates");

    let settings = Settings::builder()
        .config_layer(&config_file)?
        .env_layer()?
        .arg_layer(args)
        .build()?;
    debug!("{settings:?}");

    let diff_settings = DiffSettings::new().names(String::from("expected"), String::from("actual")); // TODO
    let mut project = Project::new(
        project_dir,
        settings,
        diff_settings,
        user_checklists_dir,
        user_templates_dir,
        cache_dir.to_path_buf(),
    )?;
    let statuses = project.run_checks()?;
    statuses.print();

    let code = statuses.exit_code();
    std::process::exit(code);
}
