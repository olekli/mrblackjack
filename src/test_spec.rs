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

/// # Test Type
/// Tests of type `Cluster` will be run first and not concurrent to tests of type `User`.
/// Limits to concurrency and number of retries can be set separately for both types
/// via the command line arguments.
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
#[serde(deny_unknown_fields)]
pub struct TestSpec {
    /// # Test Name
    #[serde(default)]
    pub name: String,
    /// # Test Type
    #[serde(default, rename = "type")]
    pub test_type: TestType,
    /// # Ordering
    /// String will be used to determine ordering of tests by lexicographical comparison.
    #[serde(default)]
    pub ordering: Option<String>,
    /// # Test Steps
    #[serde(default)]
    pub steps: Vec<StepSpec>,
    #[serde(skip_deserializing)]
    pub dir: PathBuf,
    #[serde(default)]
    /// # Attempts
    /// On failure, the test will be retried for a total number of attempts.
    pub attempts: Option<u16>,
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
#[serde(deny_unknown_fields)]
pub struct StepSpec {
    /// # Step Name
    pub name: String,
    /// # Watches
    /// Set any number of watches.
    /// Will immediately start and reflect all matching resources observed in the corresponding
    /// buckets.
    #[serde(default)]
    pub watch: Vec<WatchSpec>,
    /// # Bucket Operations
    /// Modify any existing watch bucket to only reflect certain events.
    #[serde(default)]
    pub bucket: Vec<BucketSpec>,
    #[serde(default)]
    /// # Apply Manifests
    pub apply: Vec<ApplySpec>,
    #[serde(default)]
    /// # Delete Manifests
    pub delete: Vec<ApplySpec>,
    #[serde(default)]
    /// # Run Script
    /// A list of paths to shell scripts that will be _sourced_ by `sh`.
    /// All exported env variables starting with prefix `BLACKJACK_` will be
    /// available within the test spec as `${BLACKJACK_XXX}`.
    pub script: Vec<ScriptSpec>,
    #[serde(default)]
    /// # Sleep
    /// Sleep unconditionally, in seconds.
    pub sleep: u16,
    /// # Wait
    /// Wait for all of the listed conditions to be fulfilled.
    #[serde(default)]
    pub wait: Vec<WaitSpec>,
}

pub type ScriptSpec = String;

#[derive(Default, Clone, Serialize, Deserialize, JsonSchema, DisplayAsJsonPretty, DebugAsJson)]
#[serde(deny_unknown_fields)]
pub struct BucketSpec {
    /// # Bucket Name
    /// Name of the bucket to set operations on.
    pub name: String,
    /// # Operations
    /// List of operations observed that will be reflected in the bucket.
    ///
    /// Not setting `Create` will result in newly created resources that match
    /// the buckets watch pattern to be _not_ recorded.
    ///
    /// Not setting `Delete` will result in resources in the bucket not being removed when the
    /// reflected resource is deleted on the cluster.
    ///
    /// Not setting `Patch` will result in resources in the bucket  not being updated when the
    /// reflected resource is modified on the cluster.
    pub operations: HashSet<BucketOperation>,
}

#[derive(
    Clone, Serialize, Deserialize, JsonSchema, Eq, Hash, PartialEq, DisplayAsJsonPretty, DebugAsJson,
)]
#[serde(rename_all = "lowercase")]
#[serde(deny_unknown_fields)]
pub enum BucketOperation {
    Create,
    Patch,
    Delete,
}

#[derive(Default, Clone, Serialize, Deserialize, JsonSchema, DisplayAsJsonPretty, DebugAsJson)]
#[serde(deny_unknown_fields)]
pub struct WatchSpec {
    /// # Bucket Name
    pub name: String,
    /// # Kind
    /// Kind of resources to match.
    #[serde(default)]
    pub kind: String,
    /// # Group
    /// Group of resources to match.
    #[serde(default)]
    pub group: String,
    /// # Version
    /// Version of resources to match.
    #[serde(default)]
    pub version: String,
    /// # Namespace
    /// Namespace of resources to match.
    /// Blackjack creates a unique namespace for each test.
    /// If no namespace to watch is specified,
    /// it defaults to the namespace created by Blackjack.
    #[serde(default = "default_namespace")]
    pub namespace: String,
    /// # Label Selector
    #[serde(default)]
    pub labels: Option<BTreeMap<String, String>>,
    /// # Field Selector
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
#[serde(deny_unknown_fields)]
pub struct ApplySpec {
    /// # Path of Manifest
    /// Can be a single file or a whole directory.
    pub path: String,
    #[serde(default = "default_override_namespace", rename = "override-namespace")]
    /// # Override Namespace
    /// Whether to override namespace specifications in the manifests.
    /// Defaults to `true`.
    pub override_namespace: bool,
    /// # Namespace
    /// Namespace to override with.
    /// Defaults to the namespace created by Blackjack for this test.
    #[serde(default = "default_namespace")]
    pub namespace: String,
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
#[serde(deny_unknown_fields)]
pub struct WaitSpec {
    /// # Target Bucket
    /// The name of the bucket to check condition against.
    pub target: String,
    /// # Condition
    pub condition: Expr,
    /// # Timeout
    /// Timeout in seconds. When a wait times out without the condition fulfilled, the test has failed.
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
