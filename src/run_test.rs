// Copyright 2024 Ole Kliemann
// SPDX-License-Identifier: Apache-2.0

use crate::collector::{Bucket, CollectedDataContainer, Collector};
use crate::error::{Error, FailedTest, Result, TestResult};
use crate::file::{list_directories, list_files};
use crate::manifest::ManifestHandle;
use crate::namespace::NamespaceHandle;
use crate::result_formatting::log_result;
use crate::test_spec::{ApplySpec, StepSpec, TestSpec};
use crate::wait::wait_for_all;
use futures::future::join_all;
use kube::Client;
use std::path::{Path, PathBuf};
use tokio::task::JoinSet;
use tokio::time::{sleep, Duration};

fn make_namespace(name: &String) -> String {
    let mut truncated_name = name.clone();
    truncated_name.truncate(32);
    format!(
        "{}-{}-{}",
        truncated_name,
        random_word::gen_len(8, random_word::Lang::En)
            .or_else(|| Some(""))
            .unwrap(),
        random_word::gen_len(8, random_word::Lang::En)
            .or_else(|| Some(""))
            .unwrap()
    )
}

async fn run_step(
    client: Client,
    namespace: &String,
    dirname: PathBuf,
    step: &StepSpec,
    manifests: &mut Vec<ManifestHandle>,
    collectors: &mut Vec<Collector>,
    collected_data: &CollectedDataContainer,
) -> Result<()> {
    log::info!("Running step '{}' in namespace '{}'", step.name, namespace);
    log::debug!("Creating collector");
    collectors.push(
        Collector::new(
            client.clone(),
            collected_data.clone(),
            namespace.clone(),
            step.watch.clone(),
        )
        .await?,
    );

    log::debug!("Setting buckets");
    for bucket_spec in &step.bucket {
        let mut buckets = collected_data.write().await;
        buckets
            .entry(bucket_spec.name.clone())
            .and_modify(|bucket| bucket.allowed_operations = bucket_spec.operations.clone())
            .or_insert_with(|| Bucket::new(bucket_spec.operations.clone()));
    }

    log::debug!("Applying manifests");
    for apply in &step.apply {
        let client = client.clone();
        let namespace = namespace.clone();
        match apply {
            ApplySpec::File { file } => {
                let path = dirname.join(file);
                log::debug!("Applying: {:?}", path);
                let handle = ManifestHandle::new_from_file(client, path, namespace).await?;
                handle.apply().await?;
                manifests.push(handle);
            }
            ApplySpec::Dir { dir } => {
                let path = dirname.join(dir);
                log::debug!("Applying: {:?}", path);
                let handle = ManifestHandle::new_from_dir(client, path, namespace).await?;
                handle.apply().await?;
                manifests.push(handle);
            }
        }
    }

    log::debug!("Deleting resources");
    for delete in &step.delete {
        match delete {
            ApplySpec::File { file } => {
                let path = dirname.join(file);
                log::debug!("Deleting: {:?}", path);
                ManifestHandle::new_from_file(client.clone(), path, namespace.clone())
                    .await?
                    .delete()
                    .await?
            }
            ApplySpec::Dir { dir } => {
                let path = dirname.join(dir);
                log::debug!("Deleting: {:?}", path);
                ManifestHandle::new_from_dir(client.clone(), path, namespace.clone())
                    .await?
                    .delete()
                    .await?
            }
        }
    }

    if step.sleep > 0 {
        sleep(Duration::from_secs(step.sleep.into())).await;
    }

    if step.wait.len() > 0 {
        wait_for_all(&step.wait, collected_data.clone()).await?;
    }

    log::debug!("Done");
    Ok(())
}

async fn run_steps(
    client: Client,
    namespace: &String,
    test_spec: &TestSpec,
    manifests: &mut Vec<ManifestHandle>,
    collectors: &mut Vec<Collector>,
    collected_data: &CollectedDataContainer,
) -> TestResult {
    for step in &test_spec.steps {
        run_step(
            client.clone(),
            namespace,
            test_spec.dir.clone(),
            &step,
            manifests,
            collectors,
            collected_data,
        )
        .await
        .map_err(|err| FailedTest {
            test_name: test_spec.name.clone(),
            step_name: step.name.clone(),
            failure: err,
        })?;
    }

    Ok(test_spec.name.clone())
}

async fn run_test(client: Client, test_spec: TestSpec) -> TestResult {
    let namespace = make_namespace(&test_spec.name);
    log::info!(
        "Running test '{}' in namespace '{}'",
        test_spec.name,
        namespace
    );
    let namespace_handle = NamespaceHandle::new(client.clone(), &namespace);
    namespace_handle.create().await.map_err(|err| FailedTest {
        test_name: test_spec.name.clone(),
        step_name: "".to_string(),
        failure: err,
    })?;

    let mut manifests = Vec::<ManifestHandle>::new();
    let collected_data = Collector::new_data();
    let mut collectors = Vec::<Collector>::new();

    let result = run_steps(
        client,
        &namespace,
        &test_spec,
        &mut manifests,
        &mut collectors,
        &collected_data,
    )
    .await;
    log::debug!("step returned with success: {}", result.is_ok());

    log::debug!("running cleanup");
    let mut tasks = JoinSet::new();
    for mut collector in collectors {
        tasks.spawn(async move { collector.stop().await });
    }
    for manifest in manifests {
        tasks.spawn(async move { manifest.delete().await });
    }
    tasks.spawn(async move { namespace_handle.delete().await });
    let cleanup = tasks.join_all().await.into_iter().collect::<Result<Vec<_>>>();
    if cleanup.is_err() {
        log::warn!("Errors during cleanup: {:?}", cleanup.unwrap_err());
    }

    log::debug!("cleanup done");
    result
}

async fn run_all_tests(
    client: Client,
    test_specs: Vec<TestSpec>,
    parallel: u8,
) -> Result<Vec<TestResult>> {
    let mut results: Vec<TestResult> = vec![];
    for chunk in test_specs.chunks(parallel.into()) {
        let mut set = JoinSet::new();
        for test_spec in chunk {
            let client = client.clone();
            let test_spec = test_spec.clone();
            set.spawn(async move { run_test(client, test_spec).await });
        }
        results.append(&mut set.join_all().await);
    }

    Ok(results)
}

pub async fn run_test_suite(dirname: &Path, parallel: u8) -> Result<()> {
    let client = Client::try_default().await?;
    let test_specs = discover_tests(&dirname.to_path_buf()).await?;
    if test_specs.len() == 0 {
        return Err(Error::NoTestsFoundError);
    }
    let results = run_all_tests(client, test_specs, parallel).await?;
    //results.sort_by(|lhs, rhs| lhs.is_ok() < rhs.is_ok());
    for result in results {
        log_result(&result);
    }
    Ok(())
}

async fn discover_tests(dirname: &PathBuf) -> Result<Vec<TestSpec>> {
    log::trace!("Discovering tests: {dirname:?}");
    let files = list_files(dirname).await?;
    if files
        .iter()
        .filter_map(|e| e.file_name())
        .find(|&x| x == "test.yaml")
        .is_some()
    {
        return Ok(vec![TestSpec::new_from_file(dirname.clone()).await?]);
    } else {
        let dirs: Vec<PathBuf> = list_directories(dirname).await?;
        log::trace!("Descending into {dirs:?}");
        let result: Vec<TestSpec> = join_all(dirs.iter().map(|dir| discover_tests(&dir)))
            .await
            .into_iter()
            .collect::<Result<Vec<Vec<TestSpec>>>>()?
            .into_iter()
            .flatten()
            .collect();
        return Ok(result);
    }
}
