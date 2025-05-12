use anyhow::{bail, Result};
use log::debug;
use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::fmt::Debug;
use std::os::fd::BorrowedFd;
use std::os::unix::io::AsFd;
use std::process::{Child, ChildStdout, Command, Stdio};

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

pub fn run_command_line(command: &str, env: Option<&HashMap<String, String>>) -> Result<Output> {
    let pipeline = Pipeline::new(command)?;
    let output = pipeline.run(env)?;
    Ok(output)
}

pub fn run_command<S: AsRef<OsStr> + Debug>(
    exec: &S,
    args: &[S],
    env: Option<&HashMap<String, String>>,
) -> Result<Output> {
    let command = XCommand::from_parts(exec, args);
    command.run(env)
}

#[derive(Debug, Clone)]
struct XCommand {
    exec: OsString,
    args: Vec<OsString>,
}

impl XCommand {
    pub fn from_parts<S: AsRef<OsStr> + Debug>(exec: &S, args: &[S]) -> Self {
        Self {
            exec: exec.into(),
            args: args.iter().map(|x| x.into()).collect(),
        }
    }

    pub fn from_single(command: &str) -> Result<Self> {
        let parts = shlex::split(command).unwrap();

        let foo = parts.into_iter().map(|x| x.to_string()).collect::<Vec<_>>();
        let Some((exec, args)) = foo.split_first() else {
            bail!("Invalid command '{command}'")
        };

        Ok(Self {
            exec: exec.into(),
            args: args.iter().map(|x| x.into()).collect(),
        })
    }

    pub fn run(&self, env: Option<&HashMap<String, String>>) -> Result<Output> {
        let child = spawn(self, None, env)?;
        let res = child.wait_with_output()?;
        let output = output_to_output(res)?;
        Ok(output)
    }
}

fn output_to_output(input: std::process::Output) -> Result<Output> {
    Ok(Output {
        code: input.status.code().unwrap(),
        stdout: bytes_to_maybe_str(&input.stdout),
        stderr: bytes_to_maybe_str(&input.stderr),
    })
}

#[derive(Debug)]
pub struct Pipeline {
    first: XCommand,
    rest: Vec<XCommand>,
}

impl Pipeline {
    pub fn new(command: &str) -> Result<Self> {
        let foo = command.split("|").map(XCommand::from_single);
        let foo = foo.collect::<Result<Vec<_>>>()?;

        for cmd in &foo {
            let exec = &cmd.exec;
            if which::which(exec).is_err() {
                bail!("Command {exec:?} not found");
            }
        }

        let Some((first, rest)) = foo.split_first() else {
            bail!("Invalid command pipeline '{command}'")
        };

        Ok(Self {
            first: first.to_owned(),
            rest: rest.to_vec(),
        })
    }

    pub fn run(&self, env: Option<&HashMap<String, String>>) -> Result<Output> {
        let output = match self.rest.len() {
            0 => self.first.run(env)?,
            _ => {
                let mut previous = spawn(&self.first, None, env)?;
                let mut previous_stdout_fd = previous.stdout.as_ref().unwrap().as_fd();

                for next in &self.rest {
                    previous = spawn(next, Some(previous_stdout_fd), env)?;
                    previous_stdout_fd = previous.stdout.as_ref().unwrap().as_fd();
                }
                let res = previous.wait_with_output()?;
                output_to_output(res)?
            }
        };
        Ok(output)
    }
}

fn spawn(
    c: &XCommand,
    stdin_fd: Option<BorrowedFd>,
    env: Option<&HashMap<String, String>>,
) -> Result<Child> {
    let exec = &c.exec;
    let args = &c.args;
    debug!("Running '{exec:?}' with args {args:?}");
    let mut cmd = Command::new(exec);
    let mut cmd = cmd.args(args);
    if let Some(stdin_fd) = stdin_fd {
        let owned_fd = stdin_fd.try_clone_to_owned()?;
        let stdin = ChildStdout::from(owned_fd);

        cmd = cmd.stdin(stdin);
    }
    let mut cmd = cmd.stdout(Stdio::piped());
    if let Some(env) = env {
        cmd = cmd.envs(env);
    };
    let child = cmd.spawn()?;
    Ok(child)
}
