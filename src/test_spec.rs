// Copyright 2024 Ole Kliemann
// SPDX-License-Identifier: Apache-2.0

use crate::{
    error::{Result},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::File,
};
use schemars::{JsonSchema, schema_for, schema::RootSchema};
use std::path::{Path, PathBuf};

#[derive(Default, Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct TestSpec {
    pub id: String,
    #[serde(default)]
    pub steps: Vec<StepSpec>,
    #[serde(skip_deserializing)]
    pub dir: PathBuf,
}

impl TestSpec {
    pub fn new_from_file(dirname: PathBuf) -> Result<TestSpec> {
        let path = dirname.join(Path::new("test.yaml"));
        let file = File::open(path)?;
        let mut testspec: TestSpec = serde_yaml::from_reader(file)?;
        testspec.dir = dirname;
        Ok(testspec)
    }

    pub fn schema() -> RootSchema {
        schema_for!(TestSpec)
    }
}

#[derive(Default, Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct StepSpec {
    pub id: String,
    #[serde(default)]
    pub watch: Vec<WatchSpec>,
    #[serde(default)]
    pub apply: Vec<ApplySpec>,
    #[serde(default)]
    pub delete: Vec<ApplySpec>,
    #[serde(default)]
    pub wait: Vec<WaitSpec>,
    #[serde(default)]
    pub assert: Vec<AssertSpec>,
}

#[derive(Default, Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct WatchSpec {
    pub id: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub group: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub labels: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub fields: Option<BTreeMap<String, String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum ApplySpec {
    File { file: String },
    Dir { dir: String },
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct WaitSpec {
    pub target: String,
    pub condition: Expr,
    pub timeout: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AssertSpec {
    pub target: String,
    pub condition: Expr,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Expr {
    AndExpr { and: Vec<Expr> },
    OrExpr { or: Vec<Expr> },
    NotExpr { not: Box<Expr> },
    SizeExpr { size: usize },
    OneExpr { one: serde_json::Value },
    AllExpr { all: serde_json::Value },
}

impl std::fmt::Display for Expr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Expr::AndExpr { and } => {
                let exprs: Vec<String> = and.iter().map(|e| format!("{}", e)).collect();
                write!(f, "AND({})", exprs.join(", "))
            },
            Expr::OrExpr { or } => {
                let exprs: Vec<String> = or.iter().map(|e| format!("{}", e)).collect();
                write!(f, "OR({})", exprs.join(", "))
            },
            Expr::NotExpr { not } => {
                write!(f, "NOT({})", not)
            },
            Expr::SizeExpr { size } => {
                write!(f, "size == {}", size)
            },
            Expr::OneExpr { one } => {
                write!(f, "ANY({})", one)
            },
            Expr::AllExpr { all } => {
                write!(f, "ALL({})", all)
            },
        }
    }
}
