use crate::error::{Error, Result};
use envsubst;
use shell_words;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{ExitStatus, Stdio};
use tokio::io::AsyncReadExt;
use tokio::process::Command;

pub async fn execute_script(
    command_line: &str,
    wd: PathBuf,
    namespace: &str,
) -> Result<(ExitStatus, String, String)> {
    let argv = shell_words::split(&envsubst::substitute(
        command_line,
        &HashMap::from([("BLACKJACK_NAMESPACE".to_string(), namespace.to_string())]),
    )?)?;
    (!argv.is_empty())
        .then_some(())
        .ok_or(Error::Other("empty command line".to_string()))?;
    let command = &argv[0];
    let args = &argv[1..];

    let mut child = Command::new(command)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .current_dir(wd)
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

    Ok((
        status,
        String::from_utf8_lossy(&stdout_result).to_string(),
        String::from_utf8_lossy(&stderr_result).to_string(),
    ))
}
