// Copyright 2024 Ole Kliemann
// SPDX-License-Identifier: Apache-2.0

use crate::error::Result;
use std::path::{Path, PathBuf};
use tokio::fs;

pub async fn read_yaml_files(dirname: PathBuf) -> Result<String> {
    let mut combined = String::new();
    let mut entries: Vec<_> = list_files(&dirname)
        .await?
        .into_iter()
        .filter(|path| {
            path.extension()
                .map(|ext| ext.to_string_lossy().eq_ignore_ascii_case("yaml"))
                .unwrap_or(false)
        })
        .collect();

    entries.sort();

    let mut first = true;

    for path in entries {
        let content = fs::read_to_string(&path).await?;

        if !first {
            combined.push_str("---\n");
        } else {
            first = false;
        }

        combined.push_str(&content);
        combined.push('\n');
    }

    Ok(combined)
}

pub async fn list_directories(dirname: &PathBuf) -> Result<Vec<PathBuf>> {
    let root = Path::new(dirname);
    let mut dir = fs::read_dir(root).await?;
    let mut result: Vec<PathBuf> = vec![];
    while let Some(entry) = dir.next_entry().await? {
        let path = entry.path();
        if path.is_dir() {
            result.push(path);
        }
    }
    Ok(result)
}

pub async fn list_files(dirname: &PathBuf) -> Result<Vec<PathBuf>> {
    let root = Path::new(dirname);
    let mut dir = fs::read_dir(root).await?;
    let mut result: Vec<PathBuf> = vec![];
    while let Some(entry) = dir.next_entry().await? {
        let path = entry.path();
        if path.is_file() {
            result.push(path);
        }
    }
    Ok(result)
}
