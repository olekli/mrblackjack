// Copyright 2024 Ole Kliemann
// SPDX-License-Identifier: Apache-2.0

use crate::error::Result;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestTypeConfig {
    pub parallel: u16,
    pub attempts: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub timeout_scaling: f32,
    pub loglevel: String,
    pub cluster: TestTypeConfig,
    pub user: TestTypeConfig,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            timeout_scaling: 1.0,
            loglevel: "info".to_string(),
            cluster: TestTypeConfig {
                parallel: 1,
                attempts: 1,
            },
            user: TestTypeConfig {
                parallel: 4,
                attempts: 2,
            },
        }
    }
}

static CONFIG: OnceCell<Config> = OnceCell::new();

impl Config {
    pub async fn new(filename: Option<String>) -> Result<Self> {
        if let Some(path) = filename {
            Ok(serde_yaml::from_str(&fs::read_to_string(&path).await?)?)
        } else {
            Ok(Config::default())
        }
    }

    pub fn with_timeout_scaling(self, timeout_scaling: Option<f32>) -> Self {
        if let Some(timeout_scaling) = timeout_scaling {
            Config{
                timeout_scaling,
                ..self
            }
        } else {
            self
        }
    }

    pub fn with_user_parallel(self, parallel: Option<u16>) -> Self {
        if let Some(parallel) = parallel {
            Config{
                user: TestTypeConfig{
                    parallel,
                    ..self.user
                },
                ..self
            }
        } else {
            self
        }
    }

    pub fn with_cluster_parallel(self, parallel: Option<u16>) -> Self {
        if let Some(parallel) = parallel {
            Config{
                cluster: TestTypeConfig{
                    parallel,
                    ..self.cluster
                },
                ..self
            }
        } else {
            self
        }
    }

    pub fn with_user_attempts(self, attempts: Option<u16>) -> Self {
        if let Some(attempts) = attempts {
            Config{
                user: TestTypeConfig{
                    attempts,
                    ..self.user
                },
                ..self
            }
        } else {
            self
        }
    }

    pub fn with_cluster_attempts(self, attempts: Option<u16>) -> Self {
        if let Some(attempts) = attempts {
            Config{
                cluster: TestTypeConfig{
                    attempts,
                    ..self.cluster
                },
                ..self
            }
        } else {
            self
        }
    }

    pub fn init(config: Config) {
        CONFIG.set(config).unwrap();
    }

    pub fn get() -> &'static Self {
        CONFIG.get().unwrap()
    }
}
