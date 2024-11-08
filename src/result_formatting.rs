// Copyright 2024 Ole Kliemann
// SPDX-License-Identifier: Apache-2.0

use colored::Colorize;
use crate::error::{TestResult, FailedTest};

pub fn log_result(result: &TestResult) {
    match result {
        Ok(test_name) => {
            log::info!("{}  {}", "Test passed".green().bold(), test_name);
        },
        Err(FailedTest{test_name, step_name, failure}) => {
            log::info!("{}  {}: {}", "Test failed".red().bold(), test_name, step_name);
            log::info!("{}", failure);
        },
    }
}
