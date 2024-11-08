// Copyright 2024 Ole Kliemann
// SPDX-License-Identifier: Apache-2.0

use crate::collector::{CollectedDataContainer, Collector};
use crate::error::Result;
use crate::manifest::ManifestHandle;
use crate::namespace::NamespaceHandle;
use crate::test_spec::{ApplySpec, StepSpec, TestSpec};
use crate::wait::wait_for_all;
use futures::future::join_all;
use kube::Client;

fn make_namespace(id: &String) -> String {
    format!(
        "{}-{}-{}",
        id.clone(),
        random_word::gen(random_word::Lang::En),
        random_word::gen(random_word::Lang::En)
    )
}

async fn run_steps(
    client: Client,
    namespace: &String,
    steps: &Vec<StepSpec>,
    manifests: &mut Vec<ManifestHandle>,
    collectors: &mut Vec<Collector>,
    collected_data: &CollectedDataContainer,
) -> Result<()> {
    for step in steps {
        log::info!("Running step '{}' in namespace '{}'", step.id, namespace);
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
                    ManifestHandle::new_from_file(client.clone(), &file, &namespace).await
                }
                ApplySpec::Dir { dir } => {
                    ManifestHandle::new_from_dir(client.clone(), &dir, &namespace).await
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
    }

    Ok(())
}

pub async fn run_test(client: Client, test_spec: TestSpec) -> Result<()> {
    let namespace = make_namespace(&test_spec.id);
    log::info!("Running test '{}' in namespace '{}'", test_spec.id, namespace);
    let namespace_handle = NamespaceHandle::new(client.clone(), &namespace);
    namespace_handle.create().await?;

    let mut manifests = Vec::<ManifestHandle>::new();
    let collected_data = Collector::new_data();
    let mut collectors = Vec::<Collector>::new();

    let result = run_steps(
        client,
        &namespace,
        &test_spec.steps,
        &mut manifests,
        &mut collectors,
        &collected_data,
    ).await;

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
