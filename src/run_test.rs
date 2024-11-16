// Copyright 2024 Ole Kliemann
// SPDX-License-Identifier: Apache-2.0

use crate::collector::{Bucket, CollectedDataContainer, Collector};
use crate::config::Config;
use crate::error::{Error, FailedTest, Result, TestResult};
use crate::file::{list_directories, list_files};
use crate::manifest::ManifestHandle;
use crate::namespace::NamespaceHandle;
use crate::result_formatting::log_result;
use crate::script::execute_script;
use crate::test_spec::{ApplySpec, StepSpec, TestSpec, TestType};
use crate::wait::wait_for_all;
use kube::Client;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::task::{JoinHandle, JoinSet};
use tokio::time::{sleep, Duration};
use std::cmp;

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
    inherited_env: HashMap<String, String>,
) -> Result<HashMap<String, String>> {
    log::info!("Running step '{}' in namespace '{}'", step.name, namespace);
    log::debug!("Creating collector");
    collectors.push(
        Collector::new(
            client.clone(),
            namespace.clone(),
            step.watch.clone(),
            collected_data.clone(),
        )
        .await?,
    );

    log::debug!("Setting buckets");
    for bucket_spec in &step.bucket {
        let mut data = collected_data.lock().await;
        (*data)
            .buckets
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
                log::debug!("Creating manifest: {:?}", path);
                let handle = ManifestHandle::new_from_file(client, path, namespace).await?;
                log::debug!("Applying manifest");
                handle.apply().await?;
                log::debug!("Manifest applied");
                manifests.push(handle);
            }
            ApplySpec::Dir { dir } => {
                let path = dirname.join(dir);
                log::debug!("Creating manifest: {:?}", path);
                let handle = ManifestHandle::new_from_dir(client, path, namespace).await?;
                log::debug!("Applying manifest");
                handle.apply().await?;
                log::debug!("Manifest applied");
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

    log::debug!("Running scripts");
    let mut env: HashMap<String, String> = inherited_env;
    env.insert("BLACKJACK_NAMESPACE".to_string(), namespace.to_string());
    for script in &step.script {
        let (status, stdout, stderr) = execute_script(script, dirname.clone(), &mut env).await?;
        status
            .success()
            .then_some(())
            .ok_or(Error::ScriptFailed(stdout, stderr))?;
    }

    log::debug!("Sleeping");
    if step.sleep > 0 {
        sleep(Duration::from_secs(
            (step.sleep * Config::get().timeout_scaling).into(),
        ))
        .await;
    }

    log::debug!("Waiting");
    if step.wait.len() > 0 {
        wait_for_all(&step.wait, collected_data.clone(), &env).await?;
    }

    log::debug!("Done");
    Ok(env)
}

async fn run_steps(
    client: Client,
    namespace: &String,
    test_spec: &TestSpec,
    manifests: &mut Vec<ManifestHandle>,
    collectors: &mut Vec<Collector>,
    collected_data: &CollectedDataContainer,
) -> TestResult {
    let mut env: HashMap<String, String> = HashMap::new();
    for step in &test_spec.steps {
        env = run_step(
            client.clone(),
            namespace,
            test_spec.dir.clone(),
            &step,
            manifests,
            collectors,
            collected_data,
            env,
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

async fn run_test(client: Client, test_spec: TestSpec) -> (TestResult, Option<JoinHandle<()>>) {
    let namespace = make_namespace(&test_spec.name);
    log::info!(
        "Running test '{}' in namespace '{}'",
        test_spec.name,
        namespace
    );
    let namespace_handle = NamespaceHandle::new(client.clone(), &namespace);
    let ns = namespace_handle.create().await.map_err(|err| FailedTest {
        test_name: test_spec.name.clone(),
        step_name: "".to_string(),
        failure: err,
    });
    if ns.is_err() {
        return (Err(ns.unwrap_err()), None);
    }

    let mut manifests = Vec::<ManifestHandle>::new();
    let collected_data = Collector::new_data();
    let mut collectors = Vec::<Collector>::new();

    let test_task = run_steps(
        client.clone(),
        &namespace,
        &test_spec,
        &mut manifests,
        &mut collectors,
        &collected_data,
    );
    let sigint = tokio::signal::ctrl_c();
    let result = tokio::select! {
        test_result = test_task => test_result,
        _ = sigint => {
            log::info!("Received SIGINT, exiting...");
            Err(FailedTest {
                test_name: test_spec.name.clone(),
                step_name: "".to_string(),
                failure: Error::SIGINT,
            })
        }
    };

    log::debug!("step returned with success: {}", result.is_ok());

    log::debug!("initiating cleanup");
    let cleanup_task = tokio::task::spawn(async move {
        let mut results: Vec<Result<()>> = vec![];
        for mut collector in collectors {
            results.push(collector.stop().await);
        }
        {
            let data = collected_data.lock().await;
            results.push((*data).cleanup(client).await);
        }
        for manifest in manifests {
            results.push(manifest.delete().await);
        }
        results.push(namespace_handle.delete().await);
        for error in results.into_iter().filter(|r| r.is_err()) {
            log::warn!("Errors during cleanup: {:?}", error.unwrap_err());
        }
    });

    log::debug!("cleanup done");
    (result, Some(cleanup_task))
}

async fn run_all_tests(client: Client, test_specs: Vec<TestSpec>, parallel: u16) -> Result<Vec<TestResult>> {
    let mut results: Vec<TestResult> = vec![];
    let mut tasks = JoinSet::new();
    let mut it = test_specs.into_iter();
    let mut next = it.next();
    let mut cleanup_tasks: Vec<JoinHandle<()>> = vec![];
    loop {
        while next.is_some() && (tasks.len() < parallel.into()) {
            let client = client.clone();
            tasks.spawn(async move { run_test(client, next.unwrap()).await });
            next = it.next();
        }
        if let Some(result) = tasks.join_next().await {
            let (test_result, cleanup_task) = result.map_err(|err| Error::JoinError(err))?;
            if let Some(ct) = cleanup_task {
                cleanup_tasks.push(ct);
            }
            if test_result.is_ok() {
                results.push(test_result);
            } else {
                results.push(test_result);
                while next.is_some() {
                    let test_spec = next.unwrap();
                    results.push(Err(FailedTest {
                        test_name: test_spec.name,
                        step_name: "".to_string(),
                        failure: Error::NotExecuted,
                    }));
                    next = it.next();
                }
            }
        } else {
            break;
        }
    }
    log::info!("Waiting for all cleanup tasks");
    for task in cleanup_tasks {
        let sigint = tokio::signal::ctrl_c();
        tokio::select! {
            _ = task => {},
            _ = sigint => {
                log::info!("Received another SIGINT, exiting without cleanup");
                break;
            }
        };
    }

    Ok(results)
}

pub async fn run_test_suite(dirname: &Path) -> Result<()> {
    let client = Client::try_default().await?;
    let parallel = Config::get().parallel;
    let test_specs = discover_tests(&dirname.to_path_buf()).await?;
    let mut sorted_test_specs = test_specs.into_iter().fold(HashMap::new(), |mut map, item| {
        map.entry(item.test_type.clone()).or_insert(Vec::new()).push(item);
        map
    });
    for (_, tests) in &mut sorted_test_specs {
        tests.sort_by(|lhs, rhs| match (&lhs.ordering, &rhs.ordering) {
            (Some(ref l), Some(ref r)) => l.cmp(r),
            (Some(_), None) => cmp::Ordering::Greater,
            (None, Some(_)) => cmp::Ordering::Less,
            (None, None) => cmp::Ordering::Equal,
        });
    }
    let mut results: Vec<TestResult> = vec![];
    log::info!("Running cluster tests");
    if let Some(cluster_tests) = sorted_test_specs.remove(&TestType::Cluster) {
        results.append(&mut run_all_tests(client.clone(), cluster_tests, 1).await?);
    }
    log::info!("Running user tests");
    if let Some(user_tests) = sorted_test_specs.remove(&TestType::User) {
        results.append(&mut run_all_tests(client.clone(), user_tests, parallel).await?);
    }
    if results.is_empty() {
        return Err(Error::NoTestsFoundError);
    }
    let mut success = true;
    for result in results {
        log_result(&result);
        if result.is_err() {
            success = false;
        }
    }
    success.then_some(()).ok_or(Error::SomeTestsFailedError)
}

async fn discover_tests(dirname: &PathBuf) -> Result<Vec<TestSpec>> {
    log::trace!("Discovering tests: {dirname:?}");
    let mut result: Vec<TestSpec> = vec![];
    let files = list_files(dirname).await?;
    if files
        .iter()
        .filter_map(|e| e.file_name())
        .find(|&x| x == "test.yaml")
        .is_some()
    {
        result.push(TestSpec::new_from_file(dirname.clone()).await?);
    } else {
        let dirs: Vec<PathBuf> = list_directories(dirname).await?;
        log::trace!("Descending into {dirs:?}");
        for dir in dirs {
            result.append(&mut Box::pin(discover_tests(&dir)).await?);
        }
    }
    Ok(result)
}
