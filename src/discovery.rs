// Copyright 2024 Ole Kliemann
// SPDX-License-Identifier: Apache-2.0

use crate::error::{Error, Result};
use kube::Client;
use kube::discovery::Discovery;
use tokio::sync::OnceCell;

static DISCOVERY: OnceCell<Discovery> = OnceCell::const_new();

pub async fn init(client: Client) -> Result<()> {
    DISCOVERY
        .set(Discovery::new(client).run().await?)
        .map_err(|_| Error::Other("Cannot init Discovery".to_string()))
}

pub fn get() -> &'static Discovery {
    DISCOVERY.get().expect("Discovery not initialized")
}
