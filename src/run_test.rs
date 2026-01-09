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
use crate::test_spec::{EnvSubst, StepSpec, TestSpec, TestType, WaitSpec};
use crate::wait::wait_for_all;
use kube::Client;
use std::cmp;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::task::{JoinHandle, JoinSet};
use tokio::time::{sleep, Duration};
use uuid::Uuid;

/// Maximum length for Kubernetes namespace names
const MAX_NAMESPACE_LEN: usize = 63;

/// Generate a random suffix using random words or UUID as fallback
fn random_suffix() -> String {
    random_word::gen_len(8, random_word::Lang::En)
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            // Use first 8 chars of UUID as fallback
            Uuid::new_v4().to_string()[..8].to_string()
        })
}

fn make_namespace(name: &str) -> String {
    // Format: {name}-{word1}-{word2}
    // Reserve space for suffix: 8 + 1 + 8 = 17 chars (two 8-char words plus hyphen)
    let suffix = format!("{}-{}", random_suffix(), random_suffix());
    let max_name_len = MAX_NAMESPACE_LEN - suffix.len() - 1; // -1 for separator
    
    let truncated_name: String = name
        .chars()
        .take(max_name_len)
        .collect::<String>()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '-' })
        .collect();
    
    // Remove leading/trailing hyphens and collapse multiple hyphens
    let clean_name: String = truncated_name
        .trim_matches('-')
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    
    let namespace = if clean_name.is_empty() {
        suffix
    } else {
        format!("{clean_name}-{suffix}")
    };
    
    // Final safety check - truncate if somehow still too long
    namespace.chars().take(MAX_NAMESPACE_LEN).collect()
}

async fn run_step(
    client: Client,
    dirname: PathBuf,
    test_name: &str,
    step: StepSpec,
    manifests: &mut Vec<ManifestHandle>,
    collectors: &mut Vec<Collector>,
    collected_data: &CollectedDataContainer,
    inherited_env: HashMap<String, String>,
) -> Result<HashMap<String, String>> {
    let mut env: HashMap<String, String> = inherited_env;
    log::debug!("Creating collector");
    let watches: Vec<_> = step.watch.into_iter().map(|w| w.subst_env(&env)).collect();
    collectors.push(Collector::new(client.clone(), watches, collected_data.clone()).await?);

    log::debug!("Setting buckets");
    for bucket_spec in &step.bucket {
        let mut data = collected_data.lock().await;
        if !data.buckets.contains_key(&bucket_spec.name) {
            log::warn!(
                "Bucket '{}' referenced in bucket spec but no watch created it. Creating empty bucket.",
                bucket_spec.name
            );
        }
        data.buckets
            .entry(bucket_spec.name.clone())
            .and_modify(|bucket| bucket.allowed_operations = bucket_spec.operations.clone())
            .or_insert_with(|| Bucket::new(bucket_spec.operations.clone()));
    }

    log::debug!("Applying manifests");
    for apply in step.apply {
        let apply = apply.subst_env(&env);
        log::debug!("Creating manifest: {apply:?}");
        let handle = ManifestHandle::new(apply, dirname.clone(), client.clone()).await?;
        log::debug!("Applying manifest");
        handle.apply().await?;
        manifests.push(handle);
    }

    log::debug!("Deleting resources");
    for delete in step.delete {
        let delete = delete.subst_env(&env);
        log::debug!("Deleting manifest: {delete:?}");
        ManifestHandle::new(delete, dirname.clone(), client.clone())
            .await?
            .delete()
            .await?;
    }

    log::debug!("Running scripts");
    for script in step.script {
        let (status, stdout, stderr) = execute_script(&script, dirname.clone(), &mut env).await?;
        status
            .success()
            .then_some(())
            .ok_or(Error::ScriptFailed(stdout, stderr))?;
    }
    log::debug!(
        "{}/{} environment after script: {:?}",
        test_name,
        step.name,
        env
    );

    log::debug!("Sleeping");
    if step.sleep > 0 {
        sleep(Duration::from_secs(
            (step.sleep * Config::get().timeout_scaling.ceil() as u16).into(),
        ))
        .await;
    }

    log::debug!("Waiting");
    let wait: Vec<WaitSpec> = step.wait.into_iter().map(|w| w.subst_env(&env)).collect();
    if !wait.is_empty() {
        wait_for_all(wait, collected_data.clone()).await?;
    }

    log::debug!("Done");
    Ok(env)
}

async fn run_steps(
    client: Client,
    namespace: &String,
    test_spec: TestSpec,
    manifests: &mut Vec<ManifestHandle>,
    collectors: &mut Vec<Collector>,
    collected_data: &CollectedDataContainer,
) -> TestResult {
    let mut env: HashMap<String, String> = HashMap::new();
    env.insert("BLACKJACK_NAMESPACE".to_string(), namespace.to_string());
    for step in test_spec.steps {
        log::info!("Running step {}/{}", test_spec.name, step.name);
        log::debug!(
            "{}/{} current environment: {:?}",
            test_spec.name,
            step.name,
            env
        );
        let step_name = step.name.clone();
        env = run_step(
            client.clone(),
            test_spec.dir.clone(),
            &test_spec.name,
            step,
            manifests,
            collectors,
            collected_data,
            env,
        )
        .await
        .map_err(|err| {
            log::error!("Test step {}/{} failed", test_spec.name, step_name);
            FailedTest {
                test_name: test_spec.name.clone(),
                step_name,
                failure: err,
            }
        })?;
    }

    Ok(test_spec.name.clone())
}

async fn run_test(client: Client, test_spec: TestSpec) -> (TestResult, TestSpec, Option<JoinHandle<()>>) {
    let namespace = make_namespace(&test_spec.name);
    log::info!(
        "Running test '{}' with unique namespace '{}'",
        test_spec.name,
        namespace
    );
    let namespace_handle = NamespaceHandle::new(client.clone(), &namespace);
    let ns = namespace_handle.create().await.map_err(|err| FailedTest {
        test_name: test_spec.name.clone(),
        step_name: "".to_string(),
        failure: err,
    });
    if let Err(e) = ns {
        return (Err(e), test_spec, None);
    }

    let mut manifests = Vec::<ManifestHandle>::new();
    let collected_data = Collector::new_data();
    let mut collectors = Vec::<Collector>::new();

    let test_name = test_spec.name.clone();
    let test_task = run_steps(
        client.clone(),
        &namespace,
        test_spec.clone(),
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
                test_name,
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
    (result, test_spec, Some(cleanup_task))
}

async fn run_all_tests(
    client: Client,
    test_specs: Vec<TestSpec>,
    parallel: u16,
    attempts: u16,
) -> Result<Vec<TestResult>> {
    let mut results: Vec<TestResult> = vec![];
    let mut tasks = JoinSet::new();
    let mut it = test_specs.into_iter();
    let mut cleanup_tasks: Vec<JoinHandle<()>> = vec![];
    let mut attempt_counter: HashMap<String, u16> = HashMap::new();

    let mut next = it.next();
    loop {
        while next.is_some() && (tasks.len() < parallel.into()) {
            let client = client.clone();
            tasks.spawn(async move { run_test(client, next.unwrap()).await });
            next = it.next();
        }
        if let Some(result) = tasks.join_next().await {
            let (test_result, test_spec, cleanup_task) =
                result.map_err(Error::JoinError)?;
            attempt_counter
                .entry(test_spec.name.clone())
                .and_modify(|i| *i += 1)
                .or_insert(1);
            if let Some(ct) = cleanup_task {
                cleanup_tasks.push(ct);
            }
            if test_result.is_ok() {
                results.push(test_result);
            } else {
                let attempts = test_spec.attempts.unwrap_or(attempts);
                if attempt_counter.get(&test_spec.name).unwrap() < &attempts {
                    it = it.chain(std::iter::once(test_spec)).collect::<Vec<_>>().into_iter();
                    if next.is_none() {
                        next = it.next();
                    }
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
    let test_specs = discover_tests(&dirname.to_path_buf()).await?;
    let mut sorted_test_specs: HashMap<TestType, Vec<TestSpec>> = test_specs
        .into_iter()
        .fold(HashMap::new(), |mut map, item| {
            map.entry(item.test_type.clone())
                .or_default()
                .push(item);
            map
        });
    for tests in sorted_test_specs.values_mut() {
        tests.sort_by(|lhs, rhs| match (&lhs.ordering, &rhs.ordering) {
            (Some(l), Some(r)) => l.cmp(r),
            (Some(_), None) => cmp::Ordering::Greater,
            (None, Some(_)) => cmp::Ordering::Less,
            (None, None) => cmp::Ordering::Equal,
        });
    }
    let mut results: Vec<TestResult> = vec![];
    log::info!("Running cluster tests");
    if let Some(cluster_tests) = sorted_test_specs.remove(&TestType::Cluster) {
        results.append(
            &mut run_all_tests(
                client.clone(),
                cluster_tests,
                Config::get().cluster.parallel,
                Config::get().cluster.attempts,
            )
            .await?,
        );
    }
    if results.iter().all(|r| r.is_ok()) {
        log::info!("Running user tests");
        if let Some(user_tests) = sorted_test_specs.remove(&TestType::User) {
            results.append(
                &mut run_all_tests(
                    client.clone(),
                    user_tests,
                    Config::get().user.parallel,
                    Config::get().user.attempts,
                )
                .await?,
            );
        }
    } else {
        log::error!("Skipping user tests after cluster test failed");
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
        .any(|x| x == "test.yaml")
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
