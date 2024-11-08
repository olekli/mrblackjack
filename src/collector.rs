// Copyright 2024 Ole Kliemann
// SPDX-License-Identifier: Apache-2.0

use crate::{
    error::Error,
    error::Result,
    test_spec::{BucketOperation, BucketSpec, WatchSpec},
};
use futures::StreamExt;
use kube::{
    api::{DynamicObject, Patch, PatchParams},
    core::{GroupVersionKind, TypeMeta},
    discovery::{Discovery, Scope},
    runtime::watcher,
    runtime::watcher::{Event, InitialListStrategy},
    Api, Client, ResourceExt,
};
use serde_json;
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::ops::DerefMut;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

const FINALIZER_NAME: &str = "blackjack.io/finalizer";

pub struct Bucket {
    pub allowed_operations: HashSet<BucketOperation>,
    pub data: HashMap<String, serde_json::Value>,
}

impl Bucket {
    pub fn new() -> Self {
        Bucket {
            allowed_operations: HashSet::from([
                BucketOperation::Create,
                BucketOperation::Patch,
                BucketOperation::Delete,
            ]),
            data: HashMap::new(),
        }
    }
}

pub type CollectedData = HashMap<String, Bucket>;
pub type CollectedDataContainer = Arc<RwLock<CollectedData>>;

pub struct Collector {
    token: CancellationToken,
    join_set: JoinSet<Result<()>>,
    discovery: Discovery,
    client: Client,
    collected_data: CollectedDataContainer,
}

impl Collector {
    pub fn new_data() -> CollectedDataContainer {
        CollectedDataContainer::new(RwLock::new(HashMap::new()))
    }

    pub async fn new(
        client: Client,
        collected_data: CollectedDataContainer,
        namespace: String,
        specs: Vec<WatchSpec>,
    ) -> Result<Self> {
        let token = CancellationToken::new();
        let mut join_set = JoinSet::new();

        let discovery = Discovery::new(client.clone()).run().await?;
        let annotated_specs = specs.into_iter().filter_map(|spec| {
            let gvk = GroupVersionKind {
                group: spec.group.clone(),
                version: spec.version.clone(),
                kind: spec.kind.clone(),
            };
            let (ar, caps) = discovery.resolve_gvk(&gvk).or_else(|| {
                log::warn!(
                    "Failed to find resource for group: '{}', version: '{}', kind: '{}'",
                    spec.group,
                    spec.version,
                    spec.kind
                );
                None
            })?;

            let ar = ar.clone();
            let caps = caps.clone();

            match caps.scope {
                Scope::Namespaced => Some((ar, spec)),
                Scope::Cluster => {
                    log::warn!("Resource {} is not namespaced, skipping", spec.kind);
                    None
                }
            }
        });

        for (ar, spec) in annotated_specs {
            let client = client.clone();
            let ns = namespace.clone();
            let collected_data = Arc::clone(&collected_data);
            let token = token.clone();

            join_set.spawn(async move {
                let api: Api<DynamicObject> = Api::namespaced_with(client.clone(), &ns, &ar);
                let label_selector = spec
                    .labels
                    .as_ref()
                    .and_then(|labels| {
                        Some(
                            labels
                                .iter()
                                .map(|(k, v)| format!("{}={}", k, v))
                                .collect::<Vec<_>>()
                                .join(","),
                        )
                    })
                    .or_else(|| Some(String::new()))
                    .unwrap();
                let field_selector = spec
                    .fields
                    .as_ref()
                    .and_then(|fields| {
                        Some(
                            fields
                                .iter()
                                .map(|(k, v)| format!("{}={}", k, v))
                                .collect::<Vec<_>>()
                                .join(","),
                        )
                    })
                    .or_else(|| Some(String::new()))
                    .unwrap();

                let config = watcher::Config {
                    label_selector: Some(label_selector),
                    field_selector: Some(field_selector),
                    initial_list_strategy: InitialListStrategy::ListWatch,
                    ..watcher::Config::default()
                };
                let mut stream = watcher(api.clone(), config).boxed();

                while let Some(event) = tokio::select! {
                    biased;
                    _ = token.cancelled() => {
                        log::debug!("Watcher for id '{}' received cancellation signal.", spec.name);
                        None
                    }
                    event = stream.next() => event,
                } {
                    match event {
                        Ok(Event::Apply(obj)) | Ok(Event::InitApply(obj)) => {
                            let name = obj.name_any();
                            let uid = obj.metadata.uid.clone();

                            if !obj.finalizers().contains(&FINALIZER_NAME.to_string()) {
                                let patch = json!({
                                    "metadata": {
                                        "finalizers": [FINALIZER_NAME]
                                    }
                                });
                                let patch_params = PatchParams::default();
                                match api.patch(&name, &patch_params, &Patch::Merge(&patch)).await {
                                    Ok(_) => log::debug!("Added finalizer to '{}'", name),
                                    Err(e) => {
                                        log::debug!("Failed to add finalizer to '{}': {}", name, e)
                                    }
                                }
                            }
                            if obj.metadata.deletion_timestamp.is_some() {
                                Self::handle_deletion(&api, obj, &collected_data).await?;
                            } else {
                                if let Some(uid) = uid {
                                    let value = serde_json::to_value(&obj)
                                        .unwrap_or_else(|_| serde_json::Value::Null);
                                    let mut data = collected_data.write().await;
                                    let bucket = data
                                        .deref_mut()
                                        .entry(spec.name.clone())
                                        .or_insert_with(Bucket::new);
                                    if (!bucket.data.contains_key(&uid)
                                        && bucket
                                            .allowed_operations
                                            .contains(&BucketOperation::Create))
                                        || (bucket.data.contains_key(&uid)
                                            && bucket
                                                .allowed_operations
                                                .contains(&BucketOperation::Patch))
                                    {
                                        bucket.data.insert(uid, value);
                                    }
                                }
                            }
                        }
                        Ok(_) => {}
                        Err(e) => {
                            log::warn!("Error watching resource for id '{}': {}", spec.name, e);
                        }
                    }
                }

                log::debug!("Watcher for id '{}' is terminating.", spec.name);

                Ok(())
            });
        }
        Ok(Collector {
            discovery,
            token,
            join_set,
            collected_data,
            client,
        })
    }

    async fn handle_deletion(
        api: &Api<DynamicObject>,
        obj: DynamicObject,
        collected_data: &CollectedDataContainer,
    ) -> Result<()> {
        let name = obj.name_any();
        let uid = obj.metadata.uid.clone();

        if let Some(uid) = uid {
            let mut data = collected_data.write().await;
            for bucket in data.values_mut() {
                if bucket.allowed_operations.contains(&BucketOperation::Delete) {
                    bucket.data.remove(&uid);
                }
            }
        }

        let patch = json!({
            "metadata": {
                "finalizers": null
            }
        });
        let patch_params = PatchParams::default();
        match api.patch(&name, &patch_params, &Patch::Merge(&patch)).await {
            Ok(_) => log::debug!("Removed finalizer from '{}'", name),
            Err(e) => log::debug!("Failed to remove finalizer from '{}': {}", name, e),
        }

        Ok(())
    }

    async fn cleanup_finalizers(&self) -> Result<()> {
        let uids: Vec<String> = {
            let data = self.collected_data.read().await;
            data.iter()
                .flat_map(|(_, bucket)| bucket.data.iter())
                .map(|(uid, _)| uid.clone())
                .collect()
        };

        for uid in uids {
            let resource = {
                let data = self.collected_data.read().await;
                data.values()
                    .flat_map(|bucket| bucket.data.get(&uid))
                    .next()
                    .cloned()
            };

            if let Some(resource_value) = resource {
                let obj: DynamicObject = serde_json::from_value(resource_value)?;
                let TypeMeta { api_version, kind } = obj.types.clone().unwrap_or_default();
                let name = obj.name_any();
                let namespace = obj.namespace().unwrap_or_default();

                let group_version = api_version.split('/').collect::<Vec<&str>>();
                let (group, version) = if group_version.len() == 2 {
                    (group_version[0], group_version[1])
                } else {
                    ("", group_version[0])
                };

                let gvk = GroupVersionKind {
                    group: group.to_string(),
                    version: version.to_string(),
                    kind: kind.clone(),
                };

                let (ar, caps) =
                    self.discovery
                        .resolve_gvk(&gvk)
                        .ok_or_else(|| Error::DiscoveryError {
                            group: gvk.group,
                            version: gvk.version,
                            kind: gvk.kind,
                        })?;

                let api: Api<DynamicObject> = if caps.scope == Scope::Namespaced {
                    Api::namespaced_with(self.client.clone(), &namespace, &ar)
                } else {
                    Api::all_with(self.client.clone(), &ar)
                };

                let patch = json!({
                    "metadata": {
                        "finalizers": null
                    }
                });
                let patch_params = PatchParams::default();
                match api.patch(&name, &patch_params, &Patch::Merge(&patch)).await {
                    Ok(_) => log::debug!("Removed finalizer from '{}'", name),
                    Err(e) => log::warn!("Failed to remove finalizer from '{}': {}", name, e),
                }
            } else {
                log::warn!("Resource with UID '{}' not found in collected_data", uid);
            }
        }

        Ok(())
    }

    pub async fn stop(&mut self) -> Result<()> {
        assert!(!self.token.is_cancelled());

        self.token.cancel();
        let join_set = std::mem::take(&mut self.join_set);
        let results = join_set.join_all().await;
        let _ = self.cleanup_finalizers().await;
        let errors: Vec<Error> = results.into_iter().filter_map(|res| res.err()).collect();
        match errors.len() {
            0 => Ok(()),
            1 => Err(errors.into_iter().next().unwrap()),
            _ => Err(Error::MultipleErrors(errors)),
        }
    }
}
