// Copyright 2024 Ole Kliemann
// SPDX-License-Identifier: Apache-2.0

use crate::error::{Result};
use std::path::{Path, PathBuf};
use std::fs;

pub fn read_yaml_files(dirname: PathBuf) -> Result<String> {
    let dir = Path::new(&dirname);
    let mut combined = String::new();
    let mut entries: Vec<_> = fs::read_dir(dir)?
        .filter_map(|res| res.ok())
        .filter(|e| {
            let path = e.path();
            path.is_file()
                && path.extension()
                    .map(|ext| ext.to_string_lossy().eq_ignore_ascii_case("yaml"))
                    .unwrap_or(false)
        })
        .collect();

    entries.sort_by_key(|e| e.path());

    let mut first = true;

    for entry in entries {
        let path = entry.path();
        let content = fs::read_to_string(&path)?;

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

pub fn list_directories(dirname: &str) -> Result<Vec<PathBuf>> {
    let root = Path::new(dirname);
    Ok(fs::read_dir(root)?
        .filter_map(|res| res.ok())
        .filter_map(|e| {
            let path = e.path();
            path.is_dir().then(|| root.join(path))
        })
        .collect()
    )
}
