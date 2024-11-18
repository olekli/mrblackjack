// Copyright 2024 Ole Kliemann
// SPDX-License-Identifier: Apache-2.0

use crate::error::{Error, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{ExitStatus, Stdio};
use tempfile::NamedTempFile;
use tokio::fs;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use colored::Colorize;

pub async fn execute_script(
    command_line: &str,
    wd: PathBuf,
    env: &mut HashMap<String, String>,
) -> Result<(ExitStatus, String, String)> {
    let env_file = NamedTempFile::new()?;
    let env_file_path = env_file.path().to_owned();
    let shell_command = format!(
        ". {} && export -p > {}",
        command_line,
        env_file_path.display()
    );

    let mut child = Command::new("sh")
        .arg("-c")
        .arg(shell_command)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .current_dir(wd)
        .envs(env.clone())
        .spawn()?;

    let stdout = child
        .stdout
        .take()
        .ok_or(Error::Other("unable to capture script stdout".to_string()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or(Error::Other("unable to capture script stderr".to_string()))?;

    let stdout_future: tokio::task::JoinHandle<std::result::Result<_, futures::io::Error>> =
        tokio::spawn(async move {
            let mut buf = BufReader::new(stdout);
            let mut result: Vec<String> = Vec::new();
            loop {
                let mut s = String::new();
                let size = buf.read_line(&mut s).await?;
                if size == 0 {
                    break Ok(result);
                }
                log::info!("{}", s.strip_suffix("\n").or(Some(&s)).unwrap().dimmed());
                result.push(s);
            }
        });
    let stderr_future: tokio::task::JoinHandle<std::result::Result<_, futures::io::Error>> =
        tokio::spawn(async move {
            let mut buf = BufReader::new(stderr);
            let mut result: Vec<String> = Vec::new();
            loop {
                let mut s = String::new();
                let size = buf.read_line(&mut s).await?;
                if size == 0 {
                    break Ok(result);
                }
                log::info!("{}", s.strip_suffix("\n").or(Some(&s)).unwrap().dimmed());
                result.push(s);
            }
        });

    let status = child.wait().await?;
    let stdout_result = stdout_future.await??;
    let stderr_result = stderr_future.await??;

    let env_contents = fs::read_to_string(env_file_path).await?;
    for line in env_contents.lines() {
        if let Some(rest) = line.strip_prefix("export ") {
            if let Some(eq_pos) = rest.find('=') {
                let var_name = &rest[..eq_pos];
                if var_name.starts_with("BLACKJACK_") {
                    let value = &rest[eq_pos + 1..];
                    let value = value.trim_matches('\'');
                    env.insert(var_name.to_string(), value.to_string());
                }
            }
        }
    }

    Ok((status, stdout_result.join("\n"), stderr_result.join("\n")))
}
