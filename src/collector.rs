// Copyright 2024 Ole Kliemann
// SPDX-License-Identifier: Apache-2.0

use crate::{
    error::Error,
    error::Result,
    test_spec::{BucketOperation, WatchSpec},
};
use futures::StreamExt;
use kube::{
    api::{DynamicObject, Patch, PatchParams},
    core::{ApiResource, GroupVersionKind},
    runtime::watcher,
    runtime::watcher::{Event, InitialListStrategy},
    Api, Client, ResourceExt,
};
use serde_json;
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task::JoinSet;
use tokio::time::{sleep, Duration};
use tokio_util::sync::CancellationToken;

const FINALIZER_NAME: &str = "blackjack.io/finalizer";

pub struct Bucket {
    pub allowed_operations: HashSet<BucketOperation>,
    pub data: HashMap<String, serde_json::Value>,
}

impl Default for Bucket {
    fn default() -> Self {
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

impl Bucket {
    pub fn new(allowed_operations: HashSet<BucketOperation>) -> Self {
        Bucket {
            allowed_operations,
            data: HashMap::new(),
        }
    }
}

pub type Buckets = HashMap<String, Bucket>;

pub struct CollectedData {
    pub buckets: Buckets,
}
pub type CollectedDataContainer = Arc<Mutex<CollectedData>>;

impl CollectedData {
    pub fn new() -> Self {
        CollectedData {
            buckets: HashMap::new(),
        }
    }

    pub fn contains(&self, uid: &str) -> bool {
        for (_, bucket) in &self.buckets {
            if bucket.data.contains_key(uid) {
                return true;
            }
        }
        false
    }

    pub async fn cleanup(&self, client: Client) -> Result<()> {
        let uids: Vec<String> = {
            self.buckets
                .iter()
                .flat_map(|(_, bucket)| bucket.data.iter())
                .map(|(uid, _)| uid.clone())
                .collect()
        };

        for uid in uids {
            log::debug!("Removing finalizer for {uid}");
            let resource = {
                self.buckets
                    .values()
                    .flat_map(|bucket| bucket.data.get(&uid))
                    .next()
                    .cloned()
            };

            if let Some(resource_value) = resource {
                let obj: DynamicObject = serde_json::from_value(resource_value)?;
                let name = obj.name_any();
                let namespace = obj.namespace().unwrap_or_default();
                let api: Api<DynamicObject> = Api::namespaced_with(
                    client.clone(),
                    &namespace,
                    &ApiResource::from_gvk(&GroupVersionKind::try_from(
                        &obj.types.unwrap_or_default(),
                    )?),
                );

                let patch = json!({
                    "metadata": {
                        "finalizers": null
                    }
                });
                let patch_params = PatchParams::default();
                log::debug!("calling API");
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
}

struct CollectorBrief {
    client: Client,
    namespace: String,
    api_resource: ApiResource,
    spec: WatchSpec,
    collected_data: CollectedDataContainer,
    token: CancellationToken,
}

pub struct Collector {
    token: CancellationToken,
    tasks: JoinSet<Result<()>>,
}

impl Collector {
    pub fn new_data() -> CollectedDataContainer {
        CollectedDataContainer::new(Mutex::new(CollectedData::new()))
    }

    pub async fn new(
        client: Client,
        namespace: String,
        specs: Vec<WatchSpec>,
        collected_data: CollectedDataContainer,
    ) -> Result<Self> {
        let token = CancellationToken::new();
        let mut tasks = JoinSet::new();
        for spec in specs {
            let brief = CollectorBrief {
                client: client.clone(),
                namespace: spec
                    .namespace
                    .clone()
                    .or_else(|| Some(namespace.clone()))
                    .unwrap(),
                collected_data: collected_data.clone(),
                token: token.clone(),
                api_resource: ApiResource::from_gvk(&GroupVersionKind::gvk(
                    &spec.group,
                    &spec.version,
                    &spec.kind,
                )),
                spec,
            };

            tasks.spawn(async move { brief.start().await });
        }

        Ok(Collector { token, tasks })
    }

    pub async fn stop(&mut self) -> Result<()> {
        assert!(!self.token.is_cancelled());

        self.token.cancel();
        let tasks = std::mem::take(&mut self.tasks);
        log::debug!("joining all watcher");
        let results = tasks.join_all().await;
        let errors: Vec<Error> = results.into_iter().filter_map(|res| res.err()).collect();
        log::debug!("num errors: {}", errors.len());
        match errors.len() {
            0 => Ok(()),
            1 => Err(errors.into_iter().next().unwrap()),
            _ => Err(Error::MultipleErrors(errors)),
        }
    }
}

impl CollectorBrief {
    async fn handle_apply(&self, api: Api<DynamicObject>, obj: DynamicObject) -> Result<()> {
        let name = obj.name_any();
        let uid = obj.metadata.uid.clone().unwrap();
        let mut data = self.collected_data.lock().await;
        let is_marked_for_deletion = obj.metadata.deletion_timestamp.is_some();
        let mut is_stored = (*data).contains(&uid);
        let mut has_finalizer = obj.finalizers().contains(&FINALIZER_NAME.to_string());
        if !is_stored && !is_marked_for_deletion {
            if !has_finalizer {
                let patch = json!({
                    "metadata": {
                        "finalizers": [FINALIZER_NAME]
                    }
                });
                let patch_params = PatchParams::default();
                match api.patch(&name, &patch_params, &Patch::Merge(&patch)).await {
                    Ok(_) => {
                        has_finalizer = true;
                        log::debug!("Added finalizer to '{}'", name);
                    }
                    Err(e) => {
                        log::debug!("Failed to add finalizer to '{}': {}", name, e);
                    }
                }
            }
        }
        if is_marked_for_deletion {
            if is_stored {
                is_stored = false;
                for (_, bucket) in &mut (*data).buckets {
                    if bucket.allowed_operations.contains(&BucketOperation::Delete) {
                        bucket.data.remove(&uid);
                    } else {
                        is_stored = true;
                    }
                }
            }
            if has_finalizer && !is_stored {
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
            }
        } else {
            let value = serde_json::to_value(&obj).unwrap_or_else(|_| serde_json::Value::Null);
            let bucket = (*data)
                .buckets
                .entry(self.spec.name.clone())
                .or_insert_with(Default::default);
            if (!bucket.data.contains_key(&uid)
                && bucket.allowed_operations.contains(&BucketOperation::Create))
                || (bucket.data.contains_key(&uid)
                    && bucket.allowed_operations.contains(&BucketOperation::Patch))
            {
                bucket.data.insert(uid, value);
            }
        }
        Ok(())
    }

    async fn start(&self) -> Result<()> {
        let api: Api<DynamicObject> =
            Api::namespaced_with(self.client.clone(), &self.namespace, &self.api_resource);
        let label_selector = self
            .spec
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
        let field_selector = self
            .spec
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
            _ = self.token.cancelled() => {
                log::debug!("Watcher for id '{}' received cancellation signal.", self.spec.name);
                None
            }
            event = stream.next() => event,
        } {
            let result = match event {
                Ok(Event::Apply(obj)) | Ok(Event::InitApply(obj)) => match obj.uid() {
                    Some(_) => self.handle_apply(api.clone(), obj).await,
                    None => Err(Error::NoUidError),
                },
                Ok(_) => Ok(()),
                Err(e) => Err(Error::WatcherError(e)),
            };
            match result {
                Ok(_) => {}
                Err(e) => {
                    log::warn!(
                        "Error watching resource for watch '{}': {}",
                        self.spec.name,
                        e
                    );
                    sleep(Duration::from_secs(10)).await;
                }
            }
        }

        log::debug!("Watcher for id '{}' is terminating.", self.spec.name);

        Ok(())
    }
}
