// Copyright 2024 Ole Kliemann
// SPDX-License-Identifier: Apache-2.0

use crate::error::Result;
use display_json::{DebugAsJson, DisplayAsJsonPretty};
use envsubst;
use schemars::{schema::RootSchema, schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use tokio::fs::read_to_string;

pub type Env = HashMap<String, String>;

pub trait EnvSubst {
    fn subst_env(self, env: &Env) -> Self;
}

#[derive(
    Default,
    Clone,
    Serialize,
    Deserialize,
    JsonSchema,
    Eq,
    PartialEq,
    Hash,
    DisplayAsJsonPretty,
    DebugAsJson,
)]
#[serde(rename_all = "lowercase")]
pub enum TestType {
    Cluster,
    #[default]
    User,
}

#[derive(Default, Clone, Serialize, Deserialize, JsonSchema, DisplayAsJsonPretty, DebugAsJson)]
pub struct TestSpec {
    #[serde(default)]
    pub name: String,
    #[serde(default, rename = "type")]
    pub test_type: TestType,
    #[serde(default)]
    pub ordering: Option<String>,
    #[serde(default)]
    pub steps: Vec<StepSpec>,
    #[serde(skip_deserializing)]
    pub dir: PathBuf,
}

impl TestSpec {
    pub async fn new_from_file(dirname: PathBuf) -> Result<TestSpec> {
        let path = dirname.join(Path::new("test.yaml"));
        let data = read_to_string(path).await?;
        let mut testspec: TestSpec = serde_yaml::from_str(&data)?;
        if testspec.name == "" {
            let mut it = dirname.components();
            let n2 = it.next_back().map_or_else(
                || "".to_string(),
                |x| {
                    let x: &OsStr = x.as_ref();
                    x.to_str().unwrap_or_default().to_string()
                },
            );
            let n1 = it.next_back().map_or_else(
                || "".to_string(),
                |x| {
                    let x: &OsStr = x.as_ref();
                    x.to_str().unwrap_or_default().to_string()
                },
            );
            testspec.name = format!("{n1}-{n2}");
        }
        testspec.dir = dirname;
        Ok(testspec)
    }

    pub fn schema() -> RootSchema {
        schema_for!(TestSpec)
    }
}

#[derive(Default, Clone, Serialize, Deserialize, JsonSchema, DisplayAsJsonPretty, DebugAsJson)]
pub struct StepSpec {
    pub name: String,
    #[serde(default)]
    pub bucket: Vec<BucketSpec>,
    #[serde(default)]
    pub watch: Vec<WatchSpec>,
    #[serde(default)]
    pub apply: Vec<ApplySpec>,
    #[serde(default)]
    pub delete: Vec<ApplySpec>,
    #[serde(default)]
    pub script: Vec<ScriptSpec>,
    #[serde(default)]
    pub sleep: u16,
    #[serde(default)]
    pub wait: Vec<WaitSpec>,
}

pub type ScriptSpec = String;

#[derive(Default, Clone, Serialize, Deserialize, JsonSchema, DisplayAsJsonPretty, DebugAsJson)]
pub struct BucketSpec {
    pub name: String,
    pub operations: HashSet<BucketOperation>,
}

#[derive(
    Clone, Serialize, Deserialize, JsonSchema, Eq, Hash, PartialEq, DisplayAsJsonPretty, DebugAsJson,
)]
#[serde(rename_all = "lowercase")]
pub enum BucketOperation {
    Create,
    Patch,
    Delete,
}

#[derive(Default, Clone, Serialize, Deserialize, JsonSchema, DisplayAsJsonPretty, DebugAsJson)]
pub struct WatchSpec {
    pub name: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub group: String,
    #[serde(default)]
    pub version: String,
    #[serde(default = "default_namespace")]
    pub namespace: String,
    #[serde(default)]
    pub labels: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub fields: Option<BTreeMap<String, String>>,
}

impl EnvSubst for WatchSpec {
    fn subst_env(self, env: &Env) -> Self {
        WatchSpec {
            name: self.name,
            kind: subst_or_not(self.kind, env),
            group: subst_or_not(self.group, env),
            version: subst_or_not(self.version, env),
            namespace: subst_or_not(self.namespace, env),
            labels: self.labels,
            fields: self.fields,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, JsonSchema, DisplayAsJsonPretty, DebugAsJson)]
pub struct ApplySpec {
    pub path: String,
    #[serde(default = "default_namespace")]
    pub namespace: String,
    #[serde(default = "default_override_namespace", rename = "override-namespace")]
    pub override_namespace: bool,
}

fn default_override_namespace() -> bool {
    true
}
fn default_namespace() -> String {
    "${BLACKJACK_NAMESPACE}".to_string()
}

impl EnvSubst for ApplySpec {
    fn subst_env(self, env: &Env) -> Self {
        ApplySpec {
            path: subst_or_not(self.path, env),
            namespace: subst_or_not(self.namespace, env),
            override_namespace: self.override_namespace,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, JsonSchema, DisplayAsJsonPretty, DebugAsJson)]
pub struct WaitSpec {
    pub target: String,
    pub condition: Expr,
    pub timeout: u16,
}

impl EnvSubst for WaitSpec {
    fn subst_env(self, env: &Env) -> Self {
        WaitSpec {
            target: self.target,
            condition: self.condition.subst_env(env),
            timeout: self.timeout,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, JsonSchema, DebugAsJson)]
#[serde(untagged)]
pub enum Expr {
    AndExpr { and: Vec<Expr> },
    OrExpr { or: Vec<Expr> },
    NotExpr { not: Box<Expr> },
    SizeExpr { size: usize },
    OneExpr { one: serde_json::Value },
    AllExpr { all: serde_json::Value },
}

impl EnvSubst for Expr {
    fn subst_env(self, env: &Env) -> Self {
        match self {
            Expr::AndExpr { and } => Expr::AndExpr {
                and: and.into_iter().map(|expr| expr.subst_env(env)).collect(),
            },
            Expr::OrExpr { or } => Expr::OrExpr {
                or: or.into_iter().map(|expr| expr.subst_env(env)).collect(),
            },
            Expr::NotExpr { not } => Expr::NotExpr {
                not: Box::new(not.subst_env(env)),
            },
            Expr::SizeExpr { size } => Expr::SizeExpr { size },
            Expr::OneExpr { one } => Expr::OneExpr {
                one: env_subst_json(one, env),
            },
            Expr::AllExpr { all } => Expr::AllExpr {
                all: env_subst_json(all, env),
            },
        }
    }
}

impl std::fmt::Display for Expr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Expr::AndExpr { and } => {
                let exprs: Vec<String> = and.iter().map(|e| format!("{}", e)).collect();
                write!(f, "AND({})", exprs.join(", "))
            }
            Expr::OrExpr { or } => {
                let exprs: Vec<String> = or.iter().map(|e| format!("{}", e)).collect();
                write!(f, "OR({})", exprs.join(", "))
            }
            Expr::NotExpr { not } => {
                write!(f, "NOT({})", not)
            }
            Expr::SizeExpr { size } => {
                write!(f, "size == {}", size)
            }
            Expr::OneExpr { one } => {
                write!(f, "ANY({})", one)
            }
            Expr::AllExpr { all } => {
                write!(f, "ALL({})", all)
            }
        }
    }
}

fn env_subst_json(value: serde_json::Value, env: &Env) -> serde_json::Value {
    match value {
        serde_json::Value::String(s) => serde_json::Value::String(subst_or_not(s, env)),
        serde_json::Value::Array(arr) => {
            let new_arr = arr.into_iter().map(|v| env_subst_json(v, env)).collect();
            serde_json::Value::Array(new_arr)
        }
        serde_json::Value::Object(obj) => {
            let new_obj = obj
                .into_iter()
                .map(|(k, v)| (k, env_subst_json(v, env)))
                .collect();
            serde_json::Value::Object(new_obj)
        }
        other => other,
    }
}

fn subst_or_not(s: String, env: &Env) -> String {
    envsubst::substitute(&s, env).or::<String>(Ok(s)).unwrap()
}
