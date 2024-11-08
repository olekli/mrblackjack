// Copyright 2024 Ole Kliemann
// SPDX-License-Identifier: Apache-2.0

use crate::collector::{CollectedDataContainer, Collector};
use crate::error::{Error, FailedTest, Result, TestResult};
use crate::file::list_directories;
use crate::manifest::ManifestHandle;
use crate::namespace::NamespaceHandle;
use crate::result_formatting::log_result;
use crate::test_spec::{ApplySpec, StepSpec, TestSpec};
use crate::wait::wait_for_all;
use futures::future::join_all;
use kube::Client;
use std::env;
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

    if step.wait.len() > 0 {
        wait_for_all(&step.wait, collected_data.clone()).await?;
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

pub async fn run_test(client: Client, test_spec: TestSpec) -> TestResult {
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

pub async fn run_test_multiple_dir(
    client: Client,
    test_dirs: Vec<PathBuf>,
) -> Result<Vec<TestResult>> {
    let mut set = JoinSet::new();
    let mut test_specs: Vec<TestSpec> = vec![];
    for test_dir in test_dirs {
        test_specs.push(TestSpec::new_from_file(test_dir)?);
    }
    for test_spec in test_specs {
        let client = client.clone();
        set.spawn(async move { run_test(client, test_spec).await });
    }
    Ok(set.join_all().await)
}

pub async fn run_test_suite(dirname: &Path) -> Result<()> {
    let client = Client::try_default().await?;
    env::set_current_dir(&dirname)?;
    let test_dirs = list_directories(".")?;
    let mut results = run_test_multiple_dir(client, test_dirs).await?;
    //results.sort_by(|lhs, rhs| lhs.is_ok() < rhs.is_ok());
    for result in results {
        log_result(&result);
    }
    Ok(())
}
