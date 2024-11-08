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

fn make_namespace(id: &String) -> String {
    format!(
        "{}-{}-{}",
        id.clone(),
        random_word::gen(random_word::Lang::En),
        random_word::gen(random_word::Lang::En)
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
    collectors.push(
        Collector::new(
            client.clone(),
            collected_data.clone(),
            namespace.clone(),
            step.watch.clone(),
        )
        .await?,
    );

    for bucket_spec in &step.bucket {
        let mut buckets = collected_data.write().await;
        buckets
            .entry(bucket_spec.name.clone())
            .and_modify(|bucket| bucket.allowed_operations = bucket_spec.operations.clone())
            .or_insert_with(|| Bucket::new(bucket_spec.operations.clone()));
    }

    let mut these_manifests = join_all(step.apply.iter().cloned().map(|apply| async {
        match apply {
            ApplySpec::File { file } => {
                let path = dirname.join(file);
                ManifestHandle::new_from_file(client.clone(), path, &namespace).await
            }
            ApplySpec::Dir { dir } => {
                let path = dirname.join(dir);
                ManifestHandle::new_from_dir(client.clone(), path, &namespace).await
            }
        }
    }))
    .await
    .into_iter()
    .collect::<Result<Vec<ManifestHandle>>>()?;
    join_all(these_manifests.iter().map(|manifest| manifest.apply()))
        .await
        .into_iter()
        .collect::<Result<Vec<_>>>()?;
    manifests.append(&mut these_manifests);

    for delete in &step.delete {
        match delete {
            ApplySpec::File { file } => {
                let path = dirname.join(file);
                ManifestHandle::new_from_file(client.clone(), path, &namespace)
                    .await?
                    .delete()
                    .await?
            }
            ApplySpec::Dir { dir } => {
                let path = dirname.join(dir);
                ManifestHandle::new_from_dir(client.clone(), path, &namespace)
                    .await?
                    .delete()
                    .await?
            }
        }
    }

    if step.wait.len() > 0 {
        let wait = wait_for_all(&step.wait, collected_data.clone());
        tokio::select! {
            result = wait => {
                return result;
            }
            _ = tokio::signal::ctrl_c() => {
                log::info!("SIGINT received, cleaning up...");
                return Err(Error::SIGINT);
            }
        }
    }

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

    join_all(collectors.iter_mut().map(|collector| collector.stop()))
        .await
        .into_iter()
        .chain(
            join_all(manifests.iter().map(|manifest| manifest.delete()))
                .await
                .into_iter(),
        )
        .chain(std::iter::once(namespace_handle.delete().await))
        .filter_map(|r| r.err())
        .for_each(|error| {
            log::warn!("Cleanup: {error:?}");
        });

    result
}

async fn run_all_tests(client: Client, test_specs: Vec<TestSpec>) -> Result<Vec<TestResult>> {
    let mut set = JoinSet::new();
    for test_spec in test_specs {
        let client = client.clone();
        set.spawn(async move { run_test(client, test_spec).await });
    }
    Ok(set.join_all().await)
}

pub async fn run_test_suite(dirname: &Path) -> Result<()> {
    let client = Client::try_default().await?;
    let test_specs = discover_tests(&dirname.to_path_buf())?;
    let results = run_all_tests(client, test_specs).await?;
    //results.sort_by(|lhs, rhs| lhs.is_ok() < rhs.is_ok());
    for result in results {
        log_result(&result);
    }
    Ok(())
}

fn discover_tests(dirname: &PathBuf) -> Result<Vec<TestSpec>> {
    log::trace!("Discovering tests: {dirname:?}");
    let files = list_files(dirname)?;
    if files
        .iter()
        .filter_map(|e| e.file_name())
        .find(|&x| x == "test.yaml")
        .is_some()
    {
        return Ok(vec![TestSpec::new_from_file(dirname.clone())?]);
    } else {
        let dirs: Vec<PathBuf> = list_directories(dirname)?;
        log::trace!("Descending into {dirs:?}");
        let result: Vec<TestSpec> = dirs
            .into_iter()
            .map(|dir| Ok(discover_tests(&dir)?))
            .collect::<Result<Vec<Vec<TestSpec>>>>()?
            .into_iter()
            .flatten()
            .collect();
        return Ok(result);
    }
}
