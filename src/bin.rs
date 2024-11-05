// Copyright 2024 Ole Kliemann
// SPDX-License-Identifier: Apache-2.0

use blackjack::run_test::run_test;
use blackjack::error::{Result, Error};
use blackjack::test_spec::{TestSpec};
use env_logger;
use env_logger::Env;
use kube::Client;
use clap::Parser;
use std::env;
use std::path::Path;

#[derive(Parser)] // requires `derive` feature
#[command(version, about, long_about = None)]
struct Cli {
    #[arg()]
    test_dir: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let env = Env::default()
        .filter_or("BLACKJACK_LOG_LEVEL", "info");
    env_logger::init_from_env(env);
    let args = Cli::parse();

    let test_dir = Path::new(&args.test_dir);
    log::info!("{}", test_dir.display());
    env::set_current_dir(&test_dir)?;
    let test_spec = TestSpec::new_from_file(&"test.yaml".to_string())?;
    let client = Client::try_default().await?;
    match run_test(client.clone(), test_spec).await {
        Ok(_) => {
            log::info!("All tests passed!");
            Ok(())
        }
        Err(e) => match e {
            Error::TestFailures(failures) => {
                for failure in failures {
                    log::error!("{failure}");
                }
                Ok(())
            },
            _ => Err(e),
        }
    }
}
