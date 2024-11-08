// Copyright 2024 Ole Kliemann
// SPDX-License-Identifier: Apache-2.0

use crate::error::{Error, Result};
use crate::file::read_yaml_files;
use kube::api::{Api, DeleteParams, DynamicObject, Patch, PatchParams};
use kube::{
    core::GroupVersionKind,
    discovery::{Discovery, Scope},
    Client, ResourceExt,
};
use serde::Deserialize;
use serde_yaml::Value;
use std::fs;
use std::path::{PathBuf};

#[derive(Debug)]
pub struct ManifestHandle {
    prepared_resources: Vec<(Api<DynamicObject>, DynamicObject)>,
}

impl ManifestHandle {
    pub async fn new_from_data(
        client: Client,
        yaml_str: String,
        namespace_override: &str,
    ) -> Result<Self> {
        let mut manifest_documents = Vec::new();
        for document in serde_yaml::Deserializer::from_str(&yaml_str) {
            let yaml_value: Value = Value::deserialize(document)?;
            manifest_documents.push(yaml_value);
        }

        let discovery = Discovery::new(client.clone()).run().await?;
        let prepared_resources =
            Self::prepare_resources(&client, &discovery, &manifest_documents, namespace_override)
                .await?;

        Ok(ManifestHandle { prepared_resources })
    }

    pub async fn new_from_file(
        client: Client,
        filename: PathBuf,
        namespace_override: &str,
    ) -> Result<Self> {
        ManifestHandle::new_from_data(client, fs::read_to_string(filename)?, namespace_override)
            .await
    }

    pub async fn new_from_dir(
        client: Client,
        dirname: PathBuf,
        namespace_override: &str,
    ) -> Result<Self> {
        ManifestHandle::new_from_data(client, read_yaml_files(dirname)?, namespace_override).await
    }

    async fn prepare_resources(
        client: &Client,
        discovery: &Discovery,
        manifest_documents: &[Value],
        namespace_override: &str,
    ) -> Result<Vec<(Api<DynamicObject>, DynamicObject)>> {
        let mut resources = Vec::new();

        for yaml_value in manifest_documents {
            let api_version = yaml_value
                .get("apiVersion")
                .and_then(|v| v.as_str())
                .ok_or_else(|| Error::Other("Missing apiVersion".to_string()))?;
            let kind = yaml_value
                .get("kind")
                .and_then(|v| v.as_str())
                .ok_or_else(|| Error::Other("Missing kind".to_string()))?;

            let group_version = api_version.split('/').collect::<Vec<&str>>();
            let (group, version) = if group_version.len() == 2 {
                (group_version[0], group_version[1])
            } else {
                ("", group_version[0])
            };
            let (ar, caps) = discovery
                .resolve_gvk(&GroupVersionKind {
                    group: group.to_string(),
                    version: version.to_string(),
                    kind: kind.to_string(),
                })
                .ok_or_else(|| {
                    Error::Other(format!(
                        "Resource {}/{} not found in cluster",
                        api_version, kind
                    ))
                })?;

            let mut dynamic_obj: DynamicObject = serde_yaml::from_value(yaml_value.clone())?;

            let is_namespaced = caps.scope == Scope::Namespaced;
            if is_namespaced {
                dynamic_obj.metadata.namespace = Some(namespace_override.to_string());
            }

            let api: Api<DynamicObject> = if is_namespaced {
                let ns = dynamic_obj
                    .namespace()
                    .unwrap_or_else(|| namespace_override.to_string());
                Api::namespaced_with(client.clone(), &ns, &ar)
            } else {
                Api::all_with(client.clone(), &ar)
            };

            resources.push((api, dynamic_obj));
        }

        Ok(resources)
    }

    pub async fn apply(&self) -> Result<()> {
        for (api, dynamic_obj) in &self.prepared_resources {
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
            api.patch(&dynamic_obj.name_any(), &patch_params, &patch)
                .await?;
        }

        Ok(())
    }

    pub async fn delete(&self) -> Result<()> {
        for (api, dynamic_obj) in &self.prepared_resources {
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
