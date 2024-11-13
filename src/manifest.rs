// Copyright 2024 Ole Kliemann
// SPDX-License-Identifier: Apache-2.0

use crate::error::{Error, Result};
use crate::file::read_yaml_files;
use kube::api::{Api, ApiResource, DeleteParams, DynamicObject, Patch, PatchParams};
use kube::{
    core::GroupVersionKind,
    Client, ResourceExt,
};
use serde::Deserialize;
use serde_yaml::Value;
use std::path::PathBuf;
use tokio::fs;

#[derive(Debug)]
pub struct ManifestHandle {
    resources: Vec<(Api<DynamicObject>, DynamicObject)>,
}

impl ManifestHandle {
    pub async fn new_from_data(
        client: Client,
        yaml_str: String,
        namespace_override: String,
    ) -> Result<Self> {
        let mut resources = Vec::new();
        for document in serde_yaml::Deserializer::from_str(&yaml_str) {
            let yaml_value: Value = Value::deserialize(document)?;
            let mut dynamic_obj: DynamicObject = serde_yaml::from_value(yaml_value)?;
            let gvk = GroupVersionKind::try_from(dynamic_obj.types.clone().unwrap_or_default())?;

            if gvk.kind == "Namespace" {
                continue;
            }

            dynamic_obj.metadata.namespace = Some(namespace_override.clone());

            let api: Api<DynamicObject> =
                Api::namespaced_with(
                    client.clone(),
                    &namespace_override,
                    &ApiResource::from_gvk(&gvk)
                );

            resources.push((api, dynamic_obj));
        }

        Ok(ManifestHandle { resources })
    }

    pub async fn new_from_file(
        client: Client,
        filename: PathBuf,
        namespace_override: String,
    ) -> Result<Self> {
        ManifestHandle::new_from_data(
            client,
            fs::read_to_string(filename).await?,
            namespace_override,
        )
        .await
    }

    pub async fn new_from_dir(
        client: Client,
        dirname: PathBuf,
        namespace_override: String,
    ) -> Result<Self> {
        log::debug!("new_from_dir");
        let manifest_data_ = read_yaml_files(dirname).await;
        log::debug!("got manifest data: {}", manifest_data_.is_ok());
        let manifest_data = manifest_data_?;
        ManifestHandle::new_from_data(client, manifest_data, namespace_override).await
    }

    pub async fn apply(&self) -> Result<()> {
        for (api, dynamic_obj) in &self.resources {
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
