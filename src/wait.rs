// Copyright 2024 Ole Kliemann
// SPDX-License-Identifier: Apache-2.0

use crate::check::assert_expr;
use crate::collector::{Bucket, CollectedData, CollectedDataContainer};
use crate::error::{AssertDiagnostic, Error, Result, TestFailure};
use crate::test_spec::WaitSpec;
use std::ops::Deref;
use tokio::time::{sleep, Duration};

fn check_spec_against_data(
    wait_spec: &WaitSpec,
    collected_data: &CollectedData,
) -> std::result::Result<(), AssertDiagnostic> {
    let default: Bucket = Default::default();
    let data = collected_data.buckets
        .get(&wait_spec.target)
        .or_else(|| Some(&default))
        .unwrap()
        .data
        .iter()
        .map(|(_, value)| value)
        .collect::<Vec<&serde_json::Value>>();
    let expr = &wait_spec.condition;
    assert_expr(&data, &expr)
}

pub async fn wait_for_all(
    wait_specs: &Vec<WaitSpec>,
    collected_data: CollectedDataContainer,
) -> Result<()> {
    let mut waits: Vec<_> = wait_specs.iter().collect();
    let mut timeout = wait_specs.iter().map(|spec| spec.timeout).max().unwrap() * 10;
    log::debug!("Found max timeout cycles: {timeout}");

    log::debug!("Waiting for {} conditions", waits.len());
    let result = loop {
        let data = collected_data.lock().await;

        let _waits = waits.clone();
        let result = _waits
            .iter()
            .map(|spec| check_spec_against_data(spec, data.deref()));
        let zipped: Vec<(&WaitSpec, std::result::Result<(), AssertDiagnostic>)> =
            waits.into_iter().zip(result.into_iter()).collect();
        let fail: Vec<(&WaitSpec, std::result::Result<(), AssertDiagnostic>)> =
            zipped.into_iter().filter(|(_, r)| r.is_err()).collect();
        let (remaining_waits, failed_results): (
            Vec<&WaitSpec>,
            Vec<std::result::Result<(), AssertDiagnostic>>,
        ) = fail.into_iter().unzip();
        let last_errors: Vec<_> = failed_results.into_iter().map(|r| r.unwrap_err()).collect();
        waits = remaining_waits;

        drop(data);

        log::trace!("Still {} conditions unfulfilled", waits.len());
        if waits.len() == 0 {
            break Ok(());
        }
        if timeout == 0 {
            let last_error = last_errors.iter().next().clone().unwrap().clone();
            break Err(Error::TestFailures(
                waits
                    .into_iter()
                    .map(|_| TestFailure::MissedWait(last_error.clone()))
                    .collect(),
            ));
        }
        timeout = timeout - 1;
        sleep(Duration::from_millis(100)).await;
    };
    log::debug!("Wait concluded with {result:?}");
    result
}
