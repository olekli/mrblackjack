// Copyright 2024 Ole Kliemann
// SPDX-License-Identifier: Apache-2.0
//
use once_cell::sync::OnceCell;

#[derive(Debug)]
pub struct Config {
    pub timeout_scaling: u16,
    pub parallel: u16,
}

static CONFIG: OnceCell<Config> = OnceCell::new();

impl Config {
    pub fn init(config: Config) {
        CONFIG.set(config).unwrap();
    }

    pub fn get() -> &'static Self {
        CONFIG.get().unwrap()
    }
}
