// Copyright 2024 Ole Kliemann
// SPDX-License-Identifier: Apache-2.0

use blackjack::config::{Config};
use blackjack::error::Result;
use blackjack::run_test::run_test_suite;
use env_logger;
use env_logger::{Builder, Env};
use std::path::Path;
use clap::Parser;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[arg(short, long)]
    config: Option<String>,

    #[arg(long)]
    user_parallel: Option<u16>,

    #[arg(long)]
    cluster_parallel: Option<u16>,

    #[arg(long)]
    user_attempts: Option<u16>,

    #[arg(long)]
    cluster_attempts: Option<u16>,

    #[arg(long)]
    timeout_scaling: Option<f32>,

    #[arg()]
    test_dir: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Cli::parse();
    Config::init(
        Config::new(args.config)
            .await?
            .with_user_parallel(args.user_parallel)
            .with_cluster_parallel(args.cluster_parallel)
            .with_user_attempts(args.user_attempts)
            .with_cluster_attempts(args.cluster_attempts)
            .with_timeout_scaling(args.timeout_scaling),
    );

    let env = Env::default().filter_or("BLACKJACK_LOG_LEVEL", Config::get().loglevel.clone());
    Builder::from_env(env).format_timestamp(None).init();

    let test_dir = Path::new(&args.test_dir);

    run_test_suite(test_dir).await
}
