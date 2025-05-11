use crate::types::RemoteFile;
use clap::Parser;
use std::{path::PathBuf, str::FromStr};

#[derive(Parser)]
pub struct Cli {
    /// Display more output
    #[clap(short, long)]
    pub(crate) verbose: bool,

    /// Additional checklist files (or directories of) to use
    #[clap(short, long = "check", value_name = "CHECK_FILE")]
    pub(crate) checks: Vec<PathBuf>,

    /// Directory of project to auit
    #[clap(value_name = "PROJECT_DIR")]
    pub project_dir: Option<PathBuf>,

    /// Do not read from cache
    #[clap(long)]
    pub(crate) no_read_cache: bool,

    /// Do not write to cache
    #[clap(long)]
    pub(crate) no_write_cache: bool,

    /// Do not try to read from or write to cache
    /// Implies 'no-read-cache' and 'no-write-cache'
    #[clap(short, long)]
    pub(crate) no_cache: bool,

    /// Delete cache file before running
    #[clap(long)]
    pub(crate) clear_cache: bool,

    /// Do not use user-wide checklists from ~/.config/checklist
    #[clap(long)]
    pub(crate) no_user_checklists: bool,

    /// Stop after the first failure
    /// (Default behavior is to run all checks, even if a previous check has failed)
    #[clap(long)]
    pub(crate) fail_fast: bool,

    /// Pull external checklist from remote
    #[clap(long)]
    pub(crate) external_checklist: Vec<RemoteFile>,

    /// Pull external template from remote
    #[clap(long)]
    pub(crate) external_template: Vec<RemoteFile>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn verify_clap_args() {
        Cli::command().debug_assert();
    }
}
