// Copyright 2024 Ole Kliemann
// SPDX-License-Identifier: Apache-2.0

use crate::error::{Error, Result};
use crate::file::read_yaml_files;
use crate::test_spec::ApplySpec;
use kube::api::{Api, DeleteParams, DynamicObject, Patch, PatchParams};
use kube::core::discovery::Scope;
use kube::{core::GroupVersionKind, Client, ResourceExt};
use serde::Deserialize;
use serde_yaml::Value;
use std::path::PathBuf;
use tokio::fs;
use kube::Discovery;

#[derive(Debug)]
pub struct ManifestHandle {
    resources: Vec<(Api<DynamicObject>, DynamicObject)>,
}

impl ManifestHandle {
    pub async fn new(spec: ApplySpec, wd: PathBuf, client: Client) -> Result<Self> {
        let path = wd.join(spec.path);
        let namespace = spec.override_namespace.then_some(spec.namespace);
        if path.is_file() {
            ManifestHandle::new_from_file(client, path, namespace).await
        } else if path.is_dir() {
            ManifestHandle::new_from_dir(client, path, namespace).await
        } else {
            Err(Error::PathError(path))
        }
    }

    async fn new_from_data(
        client: Client,
        yaml_str: String,
        namespace_override: Option<String>,
    ) -> Result<Self> {
        let mut resources = Vec::new();
        let discovery = Discovery::new(client.clone()).run().await?;
        for document in serde_yaml::Deserializer::from_str(&yaml_str) {
            let yaml_value: Value = Value::deserialize(document)?;
            let mut dynamic_obj: DynamicObject = serde_yaml::from_value(yaml_value)?;
            let gvk = GroupVersionKind::try_from(dynamic_obj.types.clone().unwrap_or_default())?;

            if namespace_override.is_some() && gvk.kind == "Namespace" {
                continue;
            }

            let (ar, caps) = discovery
                .resolve_gvk(&gvk)
                .ok_or_else(|| Error::DiscoveryError(gvk))?;

            resources.push(match caps.scope {
                Scope::Namespaced => {
                    if let Some(ref ns) = namespace_override {
                        dynamic_obj.metadata.namespace = Some(ns.clone());
                    }
                    let namespace = dynamic_obj
                        .metadata
                        .namespace
                        .clone()
                        .or_else(|| Some("default".to_string()))
                        .unwrap();

                    (
                        Api::<DynamicObject>::namespaced_with(client.clone(), &namespace, &ar),
                        dynamic_obj,
                    )
                }
                Scope::Cluster => (
                    Api::<DynamicObject>::all_with(client.clone(), &ar),
                    dynamic_obj,
                ),
            });
        }

        Ok(ManifestHandle { resources })
    }

    async fn new_from_file(
        client: Client,
        filename: PathBuf,
        namespace_override: Option<String>,
    ) -> Result<Self> {
        ManifestHandle::new_from_data(
            client,
            fs::read_to_string(filename).await?,
            namespace_override,
        )
        .await
    }

    async fn new_from_dir(
        client: Client,
        dirname: PathBuf,
        namespace_override: Option<String>,
    ) -> Result<Self> {
        log::debug!("new_from_dir");
        let manifest_data_ = read_yaml_files(dirname).await;
        log::debug!("got manifest data: {}", manifest_data_.is_ok());
        let manifest_data = manifest_data_?;
        ManifestHandle::new_from_data(client, manifest_data, namespace_override).await
    }

    pub async fn apply(&self) -> Result<()> {
        for (api, dynamic_obj) in &self.resources {
            log::debug!("applying: {dynamic_obj:?}");
            let kind = dynamic_obj.types.clone().unwrap_or_default().kind;
            let name = dynamic_obj.name_any();
            let namespace = dynamic_obj.namespace().unwrap_or_default();

            log::debug!(
                "Applying resource: kind={}, name={}, namespace={}",
                kind,
                name,
                namespace
            );

            let patch_params = PatchParams::apply("blackjack").force();
            let patch = Patch::Apply(dynamic_obj);
            let result = api
                .patch(&dynamic_obj.name_any(), &patch_params, &patch)
                .await;
            if result.is_err() {
                log::error!("{result:?}");
                return Err(Error::KubeError(result.unwrap_err()));
            }
        }

        Ok(())
    }

    pub async fn delete(&self) -> Result<()> {
        log::debug!("manifest.delete");
        for (api, dynamic_obj) in &self.resources {
            let kind = dynamic_obj.types.clone().unwrap_or_default().kind;
            let name = dynamic_obj.name_any();
            let namespace = dynamic_obj.namespace().unwrap_or_default();

            log::debug!(
                "Deleting resource: kind={}, name={}, namespace={}",
                kind,
                name,
                namespace
            );

            let delete_params = DeleteParams::default();
            match api.delete(&dynamic_obj.name_any(), &delete_params).await {
                Ok(_) => {}
                Err(kube::Error::Api(ae)) if ae.code == 404 => {}
                Err(e) => return Err(Error::from(e)),
            }
        }

        Ok(())
    }
}
