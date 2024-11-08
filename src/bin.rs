// Copyright 2024 Ole Kliemann
// SPDX-License-Identifier: Apache-2.0

use blackjack::error::{Result};
use blackjack::run_test::run_test_suite;
use clap::Parser;
use env_logger;
use env_logger::Env;
use std::path::Path;

#[derive(Parser)] // requires `derive` feature
#[command(version, about, long_about = None)]
struct Cli {
    #[arg()]
    test_dir: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let env = Env::default().filter_or("BLACKJACK_LOG_LEVEL", "info");
    env_logger::init_from_env(env);
    let args = Cli::parse();

    let test_dir = Path::new(&args.test_dir);

    run_test_suite(test_dir).await
}
