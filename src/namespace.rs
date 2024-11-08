// Copyright 2024 Ole Kliemann
// SPDX-License-Identifier: Apache-2.0

use crate::error::{Error, Result};
use k8s_openapi::api::core::v1::Namespace;
use kube::api::{DeleteParams, Patch, PatchParams, PostParams};
use kube::{Api, Client};
use serde_json::json;
use tokio::time::{sleep, Duration};

pub struct NamespaceHandle {
    namespace: String,
    api: Api<Namespace>,
}

impl NamespaceHandle {
    pub fn new(client: Client, namespace: &str) -> Self {
        let api: Api<Namespace> = Api::all(client.clone());
        NamespaceHandle {
            namespace: namespace.to_string(),
            api,
        }
    }

    pub async fn create(&self) -> Result<()> {
        let ns = Namespace {
            metadata: kube::api::ObjectMeta {
                name: Some(self.namespace.clone()),
                ..Default::default()
            },
            ..Default::default()
        };

        match self.api.create(&PostParams::default(), &ns).await {
            Ok(_) => Ok(()),
            Err(kube::Error::Api(ae)) if ae.code == 409 => Err(Error::NamespaceExists),
            Err(e) => Err(Error::from(e)),
        }
    }

    pub async fn delete(&self) -> Result<()> {
        log::debug!("Deleting namespace");
        if self.try_delete().await? {
            self.force_delete().await?;
        } else {
            log::debug!("Namespace '{}' deleted gracefully.", self.namespace);
        }
        Ok(())
    }

    async fn try_delete(&self) -> Result<bool> {
        let delete_params = DeleteParams::default();

        match self.api.delete(&self.namespace, &delete_params).await {
            Ok(delete_response) => {
                if delete_response.left().is_some() {
                    if self.wait_for_deletion(10).await? {
                        Ok(false)
                    } else {
                        Ok(true)
                    }
                } else {
                    Ok(false)
                }
            }
            Err(kube::Error::Api(ae)) if ae.code == 404 => Ok(false),
            Err(e) => Err(Error::from(e)),
        }
    }

    async fn wait_for_deletion(&self, timeout_seconds: u64) -> Result<bool> {
        log::debug!("Waiting for namespace deletion");
        for _ in 0..timeout_seconds {
            match self.api.get(&self.namespace).await {
                Ok(_) => sleep(Duration::from_secs(1)).await,
                Err(kube::Error::Api(ae)) if ae.code == 404 => {
                    return Ok(true);
                }
                Err(e) => return Err(Error::from(e)),
            }
        }
        Ok(false)
    }

    async fn force_delete(&self) -> Result<()> {
        log::debug!("Force deleting namespace");
        let patch = json!({
            "metadata": {
                "finalizers": null
            }
        });

        self.api
            .patch(
                &self.namespace,
                &PatchParams::default(),
                &Patch::Merge(&patch),
            )
            .await
            .map_err(Error::from)?;

        let delete_params = DeleteParams {
            grace_period_seconds: Some(0),
            ..DeleteParams::default()
        };

        match self.api.delete(&self.namespace, &delete_params).await {
            Ok(_) => {
                self.wait_for_deletion(10).await?;
                log::debug!("Namespace '{}' force deleted.", self.namespace);
                Ok(())
            }
            Err(e) => Err(Error::from(e)),
        }
    }
}
