// Copyright 2024 Ole Kliemann
// SPDX-License-Identifier: Apache-2.0

use crate::check::assert_expr;
use crate::collector::{Bucket, CollectedData, CollectedDataContainer};
use crate::config::Config;
use crate::error::{AssertDiagnostic, Error, Result, TestFailure, TestFailures};
use crate::test_spec::WaitSpec;
use tokio::time::{sleep, Duration};

fn check_spec_against_data(
    wait_spec: &WaitSpec,
    collected_data: &CollectedData,
) -> std::result::Result<(), AssertDiagnostic> {
    let default: Bucket = Default::default();
    let data: Vec<&serde_json::Value> = collected_data
        .buckets
        .get(&wait_spec.target)
        .unwrap_or(&default)
        .data
        .values()
        .collect();
    assert_expr(&data, &wait_spec.condition)
}

pub async fn wait_for_all(
    wait_specs: Vec<WaitSpec>,
    collected_data: CollectedDataContainer,
) -> Result<()> {
    let mut timeout = wait_specs.iter().map(|spec| spec.timeout).max().unwrap() * 10;
    timeout *= Config::get().timeout_scaling.ceil() as u16;
    log::debug!("Found max timeout cycles: {timeout}");

    log::debug!("Waiting for {} conditions", wait_specs.len());
    let mut wait_specs = wait_specs;
    while timeout > 0 && !wait_specs.is_empty() {
        log::trace!("trying to lock mutex");
        let data = collected_data.lock().await;
        log::trace!("mutex locked");
        wait_specs.retain(|w| check_spec_against_data(w, &data).is_err());
        drop(data);
        timeout -= 1;
        log::trace!("Still {} conditions unfulfilled", wait_specs.len());
        log::trace!("sleeping");
        sleep(Duration::from_millis(100)).await;
    }
    let result = if wait_specs.is_empty() {
        Ok(())
    } else {
        let data = collected_data.lock().await;
        let mut errors: Vec<TestFailure> = Vec::new();
        for spec in wait_specs {
            if let Err(assert_diagnostic) = check_spec_against_data(&spec, &data) {
                errors.push(TestFailure {
                    assert_diagnostic,
                    spec,
                });
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(Error::ConditionsFailed(TestFailures(errors)))
        }
    };
    log::debug!("Wait concluded with {result:?}");
    result
}
