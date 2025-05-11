use crate::command::{run_command, run_command_line};
use crate::INDENT;
use anyhow::{bail, Result};
use colored::Colorize;
use different::{line_diff, Diff, DiffSettings};
use log::debug;
use minijinja::Environment;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use std::collections::HashMap;
use std::fmt::Display;
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::{env, fs};

fn default_exit_code() -> i32 {
    exitcode::OK
}

fn default_http_status() -> StatusCode {
    StatusCode::OK
}

fn str_compare<'a>(
    expected: &'a str,
    actual: &'a str,
    diff_settings: &'a DiffSettings,
) -> Option<String> {
    // TODO: are we sure we want to trim?
    let expected = expected.trim();
    let actual = actual.trim();

    let diff = line_diff(expected, actual, diff_settings);
    match diff {
        Diff::Same => None,
        Diff::Diff { .. } => Some(diff.to_string()),
    }
}

fn paths_to_string<'a>(input: &'a [PathBuf]) -> String {
    let input: Vec<String> = input.iter().map(|p| p.display().to_string()).collect();
    input.join("\n")
}

fn dir_compare<'a>(
    expected: &[PathBuf],
    actual: &[PathBuf],
    diff_settings: &'a DiffSettings,
) -> Option<String> {
    let expected = paths_to_string(expected);
    let actual = paths_to_string(actual);
    str_compare(&expected, &actual, diff_settings).map(|x| x.to_string())
}

#[derive(
    Debug, Deserialize, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, derive_more::Display, Hash,
)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Delete,
    Head,
    Deleate,
    Connect,
    Options,
    Trace,
    Patch,
}

pub trait CheckTrait {
    fn do_check(
        &self,
        diff_settings: &DiffSettings,
        env: &Environment,
        this_file_path: &Path,
        vars: &HashMap<String, String>,
    ) -> Result<Status>;

    fn describe(&self) -> String;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct FileCheck {
    path: PathBuf,

    /// Exact contents of file
    contents: Option<String>,

    /// List of text "fragments" that must be in the file
    #[serde(default)]
    contains: Vec<String>,

    /// Template to check against
    /// Path relative to checklist file
    template: Option<PathBuf>,
}

impl FileCheck {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl CheckTrait for FileCheck {
    fn describe(&self) -> String {
        let mut s = format!("File {}: must exist", self.path.display());

        if !self.contains.is_empty() {
            s.push_str(&format!(", must contain {:?}", self.contains));
        }

        if let Some(contents) = &self.contents {
            s.push_str(&format!(", contents must exactly match {:?}", contents));
        }

        s
    }

    fn do_check(
        &self,
        diff_settings: &DiffSettings,
        env: &Environment,
        this_file_path: &Path,
        vars: &HashMap<String, String>,
    ) -> Result<Status> {
        if !self.path.is_file() {
            return Ok(Status::fail(
                String::from("Path is not a valid file"),
                Some(self.path.display().to_string()),
            ));
        }

        let actual_contents = fs::read_to_string(&self.path)?;

        if let Some(expected_contents) = &self.contents {
            if let Some(diff) = str_compare(expected_contents, &actual_contents, diff_settings) {
                return Ok(Status::fail(
                    format!("Contents differ"),
                    Some(diff.to_string()),
                ));
            }
        }

        if !self.contains.is_empty() {
            for expected_fragment in &self.contains {
                if !actual_contents.contains(expected_fragment) {
                    return Ok(Status::fail(
                        String::from("Expected fragment not found in file"),
                        Some(format!("{}\n{expected_fragment}", self.path.display())),
                    ));
                }
            }
        }

        if let Some(template) = &self.template {
            let template = if template.is_relative() {
                let base = this_file_path.parent().unwrap();
                base.join(template)
            } else {
                template.to_owned()
            };

            let template_name = &template.display().to_string();
            let templ = env.get_template(template_name)?;
            debug!(
                "Checking '{}' against template '{}'",
                self.path.display(),
                template_name
            );

            let expected = templ.render(vars)?; // TODO
            if let Some(diff) = str_compare(&expected, &actual_contents, diff_settings) {
                return Ok(Status::fail(
                    String::from("Populated template does not match file"),
                    Some(diff.to_string()),
                ));
            }
        }

        Ok(Status::new(false, StatusStatus::Pass))
    }
}

fn dir_contents(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut dirs = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        dirs.push(path);
    }
    Ok(dirs)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct DirectoryCheck {
    path: PathBuf,

    /// Exact contents of directory
    /// TODO: make vec<enum(file|directory)>?
    #[serde(default)]
    contents: Vec<String>,

    /// List non-exhaustive list of children files that must exist in the dir
    /// TODO: also consider making enum
    #[serde(default)]
    contains: Vec<String>,
}

impl CheckTrait for DirectoryCheck {
    fn describe(&self) -> String {
        let mut s = format!("Directory {}: must exist", &self.path.display());

        if !self.contains.is_empty() {
            s.push_str(&format!(", must contain {:?}", self.contains));
        }

        if !self.contents.is_empty() {
            s.push_str(&format!(
                ", contents must exactly match {:?}",
                self.contents
            ));
        }

        s
    }

    fn do_check(
        &self,
        diff_settings: &DiffSettings,
        env: &Environment,
        this_file_path: &Path,
        vars: &HashMap<String, String>,
    ) -> Result<Status> {
        if !self.path.is_dir() {
            return Ok(Status::fail(
                String::from("Path is not a valid directory"),
                Some(self.path.display().to_string()),
            ));
        }

        let actual_contents = dir_contents(&self.path)?;

        if !self.contents.is_empty() {
            let expected_contents: Vec<PathBuf> = self
                .contents
                .iter()
                .map(|name| self.path.join(name))
                .collect();
            if let Some(diff) = dir_compare(&expected_contents, &actual_contents, diff_settings) {
                return Ok(Status::fail(
                    String::from("Contents differ"),
                    Some(diff.to_string()),
                ));
            }
        }

        if !self.contains.is_empty() {
            for name in &self.contains {
                let expected_path = self.path.join(name);
                if !actual_contents.contains(&expected_path) {
                    return Ok(Status::fail(
                        String::from("Expected entry not found in directory"),
                        Some(format!(
                            "dir: {}, path: {}",
                            self.path.display(),
                            expected_path.display()
                        )),
                    ));
                }
            }
        }

        Ok(Status::new(false, StatusStatus::Pass))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct CommandCheck {
    cmd: String,

    #[serde(default = "default_exit_code")]
    code: i32,

    expected_stdout: Option<String>,
    expected_stderr: Option<String>,

    #[serde(default)]
    stdout_contains: Vec<String>,
    #[serde(default)]
    stderr_contains: Vec<String>,
}

impl CheckTrait for CommandCheck {
    fn describe(&self) -> String {
        todo!();
    }

    fn do_check(
        &self,
        diff_settings: &DiffSettings,
        env: &Environment,
        this_file_path: &Path,
        vars: &HashMap<String, String>,
    ) -> Result<Status> {
        todo!();
    }
}

#[serde_as]
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Hash)]
pub struct HttpCheck {
    method: HttpMethod,

    #[serde(default = "default_http_status")]
    #[serde_as(as = "DisplayFromStr")]
    code: StatusCode,

    url: String,

    #[serde(default)]
    body_contains: Vec<String>,

    expected_body: Option<String>,
}

impl CheckTrait for HttpCheck {
    fn describe(&self) -> String {
        let mut s = format!(
            "Http {} request to {} must return {}",
            self.method, self.url, self.code
        );

        if let Some(expected_body) = &self.expected_body {
            s.push_str(&format!(", body must match '{expected_body}'"));
        }

        if !self.body_contains.is_empty() {
            s.push_str(&format!(", body must contain {:?}", self.body_contains));
        }

        s
    }

    fn do_check(
        &self,
        diff_settings: &DiffSettings,
        env: &Environment,
        this_file_path: &Path,
        vars: &HashMap<String, String>,
    ) -> Result<Status> {
        todo!();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct VarCheck {
    key: String,
    /// Omitting (None) means any value is ok, but must be set
    value: Option<String>,
}

impl CheckTrait for VarCheck {
    fn describe(&self) -> String {
        let mut s = format!("Var {} ", self.key);
        if let Some(value) = &self.value {
            s.push_str(&format!(" must be set to {value}"));
        } else {
            s.push_str("must be set");
        }
        s
    }

    fn do_check(
        &self,
        diff_settings: &DiffSettings,
        env: &Environment,
        this_file_path: &Path,
        vars: &HashMap<String, String>,
    ) -> Result<Status> {
        todo!();
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum CheckType {
    File(FileCheck),
    Directory(DirectoryCheck),
    Command(CommandCheck),
    Http(HttpCheck),
    VarSet(VarCheck),
}

impl CheckType {
    fn describe(&self) -> String {
        match self {
            Self::File(f) => f.describe(),
            Self::Directory(d) => d.describe(),
            Self::Command(c) => c.describe(),
            Self::Http(h) => h.describe(),
            Self::VarSet(v) => v.describe(),
        }
    }

    fn do_check(
        &self,
        diff_settings: &DiffSettings,
        env: &Environment,
        this_file_path: &Path,
        vars: &HashMap<String, String>,
    ) -> Result<Status> {
        match self {
            Self::File(f) => f.do_check(diff_settings, env, this_file_path, vars),
            Self::Directory(d) => d.do_check(diff_settings, env, this_file_path, vars),
            Self::Command(c) => c.do_check(diff_settings, env, this_file_path, vars),
            Self::Http(h) => h.do_check(diff_settings, env, this_file_path, vars),
            Self::VarSet(v) => v.do_check(diff_settings, env, this_file_path, vars),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Hash)]
struct Condition {
    description: Option<String>,
    #[serde(flatten)]
    condition: CheckType,
}

impl CheckTrait for Condition {
    fn do_check(
        &self,
        diff_settings: &DiffSettings,
        env: &Environment,
        this_file_path: &Path,
        vars: &HashMap<String, String>,
    ) -> Result<Status> {
        self.condition
            .do_check(diff_settings, env, this_file_path, vars)
    }

    fn describe(&self) -> String {
        todo!();
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Hash)]
pub struct Check {
    #[serde(flatten)]
    check: CheckType,
    description: Option<String>,
    #[serde(default)]
    conditions: Vec<Condition>,
    #[serde(default)]
    requirements: Vec<Requirement>,
}

impl Check {
    pub fn description(&self) -> String {
        if let Some(description) = &self.description {
            return description.clone();
        }

        self.check.describe()
    }

    pub fn do_check(
        &self,
        diff_settings: &DiffSettings,
        env: &Environment,
        this_file_path: &Path,
        vars: &HashMap<String, String>,
    ) -> Result<Status> {
        for condition in &self.conditions {
            let status = condition.do_check(diff_settings, env, this_file_path, vars)?;
            if status.is_skipped() {
                return Ok(status);
            }
        }

        for requirement in &self.requirements {
            let status = requirement.do_check(diff_settings, env, this_file_path, vars)?;
            if status.is_failure() {
                return Ok(status);
            }
        }

        self.check
            .do_check(diff_settings, env, this_file_path, vars)
    }

    pub fn ttype(&self) -> &CheckType {
        &self.check
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum FactValue {
    #[serde(rename = "eval-command")]
    Command { command: String },
    #[serde(rename = "literal")]
    Literal { value: String },
    #[serde(rename = "env-var")]
    Env { key: String },
}

impl FactValue {
    fn value(&self, vars: &HashMap<String, String>) -> Result<String> {
        let value = match self {
            Self::Command { command } => {
                let output = run_command_line(&command, Some(vars))?;
                let Some(stdout) = output.stdout() else {
                    bail!("Command produced empty output");
                };

                stdout.clone()
            }
            Self::Literal { value } => value.to_string(),
            Self::Env { key } => {
                let Ok(value) = env::var(key) else {
                    bail!("Env var '{key}' not set");
                };
                value
            }
        };
        Ok(value)
    }
}

#[derive(Debug, Deserialize)]
pub struct Fact {
    key: String,
    #[serde(flatten)]
    value: FactValue,
    #[serde(default, rename = "requires")]
    requirements: Vec<Requirement>,
}

impl Fact {
    pub fn key(&self) -> String {
        self.key.clone()
    }

    pub fn value(&self, vars: &HashMap<String, String>) -> Result<String> {
        self.value.value(vars)
    }

    pub fn requirements(&self) -> &[Requirement] {
        &self.requirements
    }
}

#[derive(Debug, Deserialize, PartialEq, Eq, Hash, Clone, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Requirement {
    Command { command: String },
    Env { key: String },
}

impl CheckTrait for Requirement {
    fn do_check(
        &self,
        diff_settings: &DiffSettings,
        env: &Environment,
        this_file_path: &Path,
        vars: &HashMap<String, String>,
    ) -> Result<Status> {
        let status = match self {
            Self::Command { command } => match which::which(command) {
                Ok(_) => Status::new(false, StatusStatus::Pass),
                Err(_) => Status::new(
                    false,
                    StatusStatus::Fail {
                        reason: Reason {
                            main: format!("Command not found '{command}'"),
                            secondary: Some(format!(
                                "Required for a check in {}",
                                this_file_path.display()
                            )),
                        },
                    },
                ),
            },
            Self::Env { key } => match env::var(key) {
                Ok(_) => Status::new(false, StatusStatus::Pass),
                Err(_) => Status::new(
                    false,
                    StatusStatus::Fail {
                        reason: Reason {
                            main: format!("Env var '{key}' not set"),
                            secondary: Some(format!(
                                "Required for a check in {}",
                                this_file_path.display()
                            )),
                        },
                    },
                ),
            },
        };
        Ok(status)
    }

    fn describe(&self) -> String {
        todo!();
    }
}

#[derive(Debug, Deserialize)]
struct ChecklistFileContents {
    #[serde(rename = "fact", default)]
    facts: Vec<Fact>,
    #[serde(rename = "condition", default)]
    conditions: Vec<Condition>,
    #[serde(rename = "check", default)]
    checks: Vec<Check>,
    #[serde(rename = "requires", default)]
    requirements: Vec<Requirement>,
}

#[derive(Debug)]
pub struct Checklist {
    path: PathBuf,
    checks: ChecklistFileContents,
}

impl Checklist {
    pub fn from_path(path: PathBuf) -> Result<Self> {
        let contents = fs::read_to_string(&path)?;
        let checks: ChecklistFileContents = toml::from_str(&contents)?;

        Ok(Self { checks, path })
    }

    pub fn checks(&self) -> &[Check] {
        &self.checks.checks
    }

    pub fn name(&self) -> Result<String> {
        let Some(name) = self.path.as_os_str().to_str() else {
            bail!("Unable to get name from path {}", self.path.display());
        };
        Ok(name.to_string())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn templates(&self) -> Vec<PathBuf> {
        let mut templates = Vec::new();
        let checklist_path = &self.path;
        for check in &self.checks.checks {
            match &check.check {
                CheckType::File(f) => {
                    if let Some(name) = &f.template {
                        let template = rel_to(checklist_path.parent().unwrap(), name);
                        debug!("found template {}", template.display());
                        templates.push(template);
                    }
                }
                CheckType::Directory(d) => {
                    // TODO
                }
                CheckType::Command(c) => {
                    // TODO
                }
                CheckType::Http(h) => {
                    // TODO
                }
                CheckType::VarSet(v) => {
                    // TODO
                }
            }
        }

        templates
    }

    pub fn facts(&self) -> &[Fact] {
        &self.checks.facts
    }
}

fn rel_to(a: &Path, b: &Path) -> PathBuf {
    a.join(b)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Reason {
    main: String,
    secondary: Option<String>,
}

impl Reason {
    pub fn new(main: String, secondary: Option<String>) -> Self {
        Self { main, secondary }
    }

    pub fn main(&self) -> &str {
        &self.main
    }

    pub fn secondary(&self) -> Option<&String> {
        self.secondary.as_ref()
    }
}

impl Display for Reason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = format!("{}", self.main);
        if let Some(secondary) = &self.secondary {
            s.push_str(&format!(": {secondary}"));
        };
        write!(f, "{s}")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StatusStatus {
    Pass,
    Skip { reason: Reason },
    Fail { reason: Reason },
}

impl StatusStatus {
    pub fn is_skipped(&self) -> bool {
        match self {
            StatusStatus::Skip { .. } => true,
            _ => false,
        }
    }

    pub fn is_success(&self) -> bool {
        match self {
            StatusStatus::Pass => true,
            _ => false,
        }
    }

    pub fn is_failure(&self) -> bool {
        match self {
            StatusStatus::Fail { .. } => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Status {
    status: StatusStatus,
    cached: bool,
}

impl Status {
    pub fn new(cached: bool, status: StatusStatus) -> Self {
        Self { cached, status }
    }

    pub fn fail(main: String, secondary: Option<String>) -> Self {
        Self::new(
            false,
            StatusStatus::Fail {
                reason: Reason::new(main, secondary),
            },
        )
    }

    pub fn mark_as_cached(&mut self) {
        self.cached = true;
    }

    pub fn is_cached(&self) -> bool {
        self.cached
    }

    pub fn is_skipped(&self) -> bool {
        self.status.is_skipped()
    }

    pub fn is_success(&self) -> bool {
        self.status.is_success()
    }

    pub fn is_failure(&self) -> bool {
        self.status.is_failure()
    }

    pub fn status(&self) -> &StatusStatus {
        &self.status
    }
}

impl Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self.status() {
            StatusStatus::Pass => "Pass",
            StatusStatus::Skip { reason } => &format!("Skipped ({reason})"),
            StatusStatus::Fail { reason } => &format!("Failed ({reason})"),
        };
        write!(f, "{s}")
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Statuses {
    /// Map checklist path to map of check name to check status
    map: HashMap<PathBuf, HashMap<String, Status>>,
}

impl Statuses {
    pub fn new() -> Self {
        let map = HashMap::new();
        Self { map }
    }

    pub fn exit_code(&self) -> i32 {
        let mut code = 0;
        for checklist in self.map.values() {
            for status in checklist.values() {
                if *status.status() != StatusStatus::Pass {
                    code = 1;
                    break;
                }
            }
        }

        code
    }

    pub fn insert(&mut self, checklist_path: PathBuf, job_name: String, job_status: Status) {
        if !self.map.contains_key(&checklist_path) {
            self.map.insert(checklist_path.clone(), HashMap::new());
        }

        let inner = self.map.get_mut(&checklist_path).unwrap();
        inner.insert(job_name, job_status);
    }

    pub fn json(&self) -> Result<String> {
        let json = serde_json::to_string_pretty(&self)?;
        Ok(json)
    }

    pub fn print(&self) {
        let last_index = self.map.len() - 1;
        for (i, (checklist_path, checks)) in self.map.iter().enumerate() {
            let checklist_name = checklist_path.file_name().unwrap().to_str().unwrap();
            print_section_header(&checklist_name);

            for (name, status) in checks {
                print_status(status, name, None); // TODO: introduce timings back
            }

            if i < last_index {
                println!();
            }
        }
    }
}

fn format_duration(d: Duration) -> String {
    let truncated = Duration::from_millis(d.as_millis() as u64);
    if truncated == Duration::ZERO {
        "< 1ms".to_string()
    } else {
        humantime::format_duration(truncated).to_string()
    }
}

fn print_section_header(name: &str) {
    println!("> Checklist '{}'", name.cyan());
}

fn print_status(status: &Status, desc: &str, duration: Option<Duration>) {
    let (status_str, reason) = match status.status() {
        StatusStatus::Skip { reason } => ("SKIP".yellow(), Some(reason)),
        StatusStatus::Pass => ("PASS".green(), None),
        StatusStatus::Fail { reason } => ("FAIL".red(), Some(reason)),
    };
    let cached = if status.is_cached() { " (cached)" } else { "" };
    let duration = if let Some(duration) = duration {
        &format!(" - took {}", format_duration(duration))
    } else {
        ""
    };
    println!(
        "{INDENT}[{}] {desc}{}{}",
        status_str.bold(),
        cached.dimmed(),
        duration.dimmed()
    );

    if let Some(reason) = reason {
        let subindent = format!("{INDENT}  ");
        print!("{INDENT}{subindent}- {}", reason.main().purple());
        if let Some(secondary) = reason.secondary() {
            println!(":\n{secondary}");
        } else {
            println!();
        }
    }
}

pub use remote_checklist::RemoteFile;

mod remote_checklist {

    use anyhow::bail;
    use serde::Deserialize;
    use serde::Serialize;
    use std::fmt::Display;
    use std::str::FromStr;
    use winnow::ascii::dec_uint;
    use winnow::combinator::alt;
    use winnow::combinator::opt;
    use winnow::combinator::seq;
    use winnow::error::ContextError;
    use winnow::prelude::*;
    use winnow::token::rest;
    use winnow::token::take_till;
    use winnow::token::take_until;
    use winnow::Result;

    fn last_component_of(s: &str) -> String {
        s.split("/").last().unwrap().to_string()
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Url {
        scheme: String,
        host: String,
        port: Option<u32>,
        path: Option<String>,
        fragment: Option<String>,
    }

    impl Url {
        pub fn name(&self) -> String {
            match &self.path {
                Some(path) => last_component_of(path),
                None => {
                    // Fall back to host
                    self.host.clone()
                }
            }
        }
    }

    impl Display for Url {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            let port = match &self.port {
                Some(port) => format!(":{port}"),
                None => String::new(),
            };

            let path = match &self.path {
                Some(path) => path.clone(),
                None => String::new(),
            };

            let fragment = match &self.fragment {
                Some(fragment) => format!("#{fragment}"),
                None => String::new(),
            };

            write!(f, "{}://{}{port}{path}{fragment}", self.scheme, self.host)
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct RemoteFile {
        url: Url,
        hash: Option<String>,
    }

    impl RemoteFile {
        pub fn url(&self) -> &Url {
            &self.url
        }

        pub fn hash(&self) -> Option<&String> {
            self.hash.as_ref()
        }
    }

    fn scheme(s: &mut &str) -> Result<String> {
        take_until(1.., "://")
            .map(|s: &str| s.to_string())
            .parse_next(s)
    }

    fn host(s: &mut &str) -> Result<String> {
        alt((take_till(1.., |c: char| c == ':' || c == '/'), rest))
            .map(|s: &str| s.to_string())
            .parse_next(s)
    }

    fn port(s: &mut &str) -> Result<u32> {
        let _ = ":".parse_next(s)?;
        dec_uint.parse_next(s)
    }

    fn fragment(s: &mut &str) -> Result<String> {
        let _ = "#".parse_next(s)?;
        take_until(1.., "::")
            .map(|s: &str| s.to_string())
            .parse_next(s)
    }

    fn url(s: &mut &str) -> Result<Url> {
        seq! {Url {
            scheme: scheme,
            _: "://",
            host: host,
            port: opt(port),
            path: opt(path),
            fragment: opt(fragment)
        }}
        .parse_next(s)
    }

    fn path(s: &mut &str) -> Result<String> {
        alt((take_till(0.., |c: char| c == '?' || c == '#'), rest))
            .map(|s: &str| s.to_string())
            .parse_next(s)
    }

    fn hash(s: &mut &str) -> Result<String> {
        let _ = "::".parse_next(s)?;
        rest.map(|s: &str| s.to_string()).parse_next(s)
    }

    fn remote_checklist(s: &mut &str) -> Result<RemoteFile> {
        let url = url.parse_next(s)?;
        let hash = opt(hash).parse_next(s)?;
        Ok(RemoteFile { url, hash })
    }

    use winnow_parse_error::ParseError;
    impl FromStr for RemoteFile {
        type Err = ParseError;
        fn from_str(s: &str) -> Result<Self, Self::Err> {
            remote_checklist
                .parse(s)
                .map_err(|e| ParseError::from_parse(e))
        }
    }
}
