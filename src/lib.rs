pub mod cache;
pub mod cli;
pub mod command;
pub mod project;
pub mod settings;
pub mod types;

pub const THIS_CRATE_NAME: &str = env!("CARGO_PKG_NAME");
pub const INDENT: &str = "    ";
pub const CONFIG_FILE_NAME: &str = "config.toml";
