use anyhow::Result;
use log::debug;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::fmt::Debug;
use std::process::Command;

#[derive(Debug)]
pub struct Output {
    code: i32,
    stdout: Option<String>,
    stderr: Option<String>,
}

impl Output {
    pub fn stdout(&self) -> Option<&String> {
        self.stdout.as_ref()
    }
}

fn bytes_to_maybe_str(b: &[u8]) -> Option<String> {
    let s = String::from_utf8_lossy(b).to_string();
    let s = s.trim();
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

pub fn run_command<S: AsRef<OsStr> + Debug>(
    exec: &str,
    args: &[S],
    env: Option<&HashMap<String, String>>,
) -> Result<Output> {
    debug!("Running '{exec}' with args {args:?}");
    let mut cmd = Command::new(exec);
    let mut cmd = cmd.args(args);
    if let Some(env) = env {
        cmd = cmd.envs(env);
    };
    let res = cmd.output()?;
    let stdout = bytes_to_maybe_str(&res.stdout);
    if let Some(stdout) = &stdout {
        debug!("Stdout:\n{stdout}");
    }
    let stderr = bytes_to_maybe_str(&res.stderr);
    if let Some(stderr) = &stderr {
        debug!("Stderr:\n{stderr}");
    }
    Ok(Output {
        code: res.status.code().unwrap(),
        stdout,
        stderr,
    })
}
