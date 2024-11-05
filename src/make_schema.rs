// Copyright 2024 Ole Kliemann
// SPDX-License-Identifier: Apache-2.0

use blackjack::test_spec::TestSpec;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", serde_json::to_string_pretty(&TestSpec::schema())?);

    Ok(())
}
