use anyhow::{bail, Result};
use checklints::cli::Cli;
use checklints::project::Project;
use checklints::settings::{write_default_config, Settings};
use checklints::{CONFIG_FILE_NAME, THIS_CRATE_NAME};
use clap::Parser;
use different::DiffSettings;
use directories::ProjectDirs;
use log::debug;
use std::env;
use std::fs;

fn main() -> Result<()> {
    env_logger::init();
    let args = Cli::parse();

    let Some(proj_dirs) = ProjectDirs::from("", "", THIS_CRATE_NAME) else {
        bail!("Unable to get XDG project dirs");
    };

    let config_dir = if let Some(config_dir) = &args.config_dir {
        config_dir
    } else {
        proj_dirs.config_dir()
    };
    if !config_dir.is_dir() {
        fs::create_dir_all(&config_dir)?;
    }

    let cache_dir = if let Some(cache_dir) = args.cache_dir.clone() {
        cache_dir
    } else {
        proj_dirs.cache_dir().to_path_buf()
    };
    if !cache_dir.is_file() {
        fs::create_dir_all(&cache_dir)?;
    }

    let project_dir = match args.project_dir {
        Some(ref project_dir) => project_dir,
        None => &env::current_dir()?,
    };
    let project_dir = project_dir.canonicalize()?;

    let config_file = config_dir.join(CONFIG_FILE_NAME);
    let config_file = if config_file.is_file() {
        Some(config_file)
    } else {
        None
    };

    let user_checklists_dir = config_dir.join("checklists");
    let user_templates_dir = config_dir.join("templates");

    let mut settings = Settings::builder();
    if let Some(config_file) = config_file {
        settings = settings.config_layer(&config_file)?;
    };

    let settings = settings.env_layer()?.arg_layer(args).build()?;
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
