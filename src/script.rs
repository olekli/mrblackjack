// Copyright 2024 Ole Kliemann
// SPDX-License-Identifier: Apache-2.0

use crate::error::{Error, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{ExitStatus, Stdio};
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tempfile::NamedTempFile;
use tokio::fs;

fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace("'", "'\\''"))
}

pub async fn execute_script(
    command_line: &str,
    wd: PathBuf,
    env: &mut HashMap<String, String>,
) -> Result<(ExitStatus, String, String)> {
    let env_file = NamedTempFile::new()?;
    let env_file_path = env_file.path().to_owned();
    let shell_command = format!(
        "{} && export -p > {}",
        shell_escape(command_line), env_file_path.display()
    );

    let mut child = Command::new("sh")
        .arg("-lc")
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

    let stdout_future: tokio::task::JoinHandle<
        std::result::Result<std::vec::Vec<u8>, futures::io::Error>,
    > = tokio::spawn(async move {
        let mut stdout = stdout;
        let mut stdout_buf: Vec<u8> = Vec::new();
        stdout.read_to_end(&mut stdout_buf).await?;
        Ok(stdout_buf)
    });
    let stderr_future: tokio::task::JoinHandle<
        std::result::Result<std::vec::Vec<u8>, futures::io::Error>,
    > = tokio::spawn(async move {
        let mut stderr = stderr;
        let mut stderr_buf: Vec<u8> = Vec::new();
        stderr.read_to_end(&mut stderr_buf).await?;
        Ok(stderr_buf)
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

    Ok((
        status,
        String::from_utf8_lossy(&stdout_result).to_string(),
        String::from_utf8_lossy(&stderr_result).to_string(),
    ))
}
