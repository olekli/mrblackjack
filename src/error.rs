// Copyright 2024 Ole Kliemann
// SPDX-License-Identifier: Apache-2.0

use crate::test_spec::{Expr, WaitSpec};
use display_json::{DebugAsJson, DisplayAsJsonPretty};
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("File error: {0}")]
    FileError(#[from] std::io::Error),

    #[error("String error")]
    StringError,

    #[error("Path error: {0}")]
    PathError(std::path::PathBuf),

    #[error("Discovery error: {0:?}")]
    DiscoveryError(kube::core::GroupVersionKind),

    #[error("Watcher error: {0}")]
    WatcherError(#[from] kube::runtime::watcher::Error),

    #[error("Command line parse error: {0}")]
    CommandlineParseError(#[from] shell_words::ParseError),

    #[error("Env substitution error: {0}")]
    EnvSubstError(#[from] envsubst::Error),

    #[error("Kube error: {0}")]
    KubeError(#[from] kube::Error),

    #[error("ParseGroupVersionError: {0}")]
    ParseGroupVersionError(#[from] kube::core::gvk::ParseGroupVersionError),

    #[error("Serialization error: {0}")]
    SerializationYamlError(#[from] serde_yaml::Error),

    #[error("Serialization error: {0}")]
    SerializationJsonError(#[from] serde_json::Error),

    #[error("Multiple errors: {0:?}")]
    MultipleErrors(Vec<Error>),

    #[error("NamespaceExists")]
    NamespaceExists,

    #[error("Conditions failed: {0}")]
    ConditionsFailed(TestFailures),

    #[error("Path encoding error")]
    PathEncodingError,

    #[error("Join error: {0:?}")]
    JoinError(#[from] tokio::task::JoinError),

    #[error("Interrupted")]
    SIGINT,

    #[error("Not executed")]
    NotExecuted,

    #[error("No tests found")]
    NoTestsFoundError,

    #[error("No UID?!")]
    NoUidError,

    #[error("Script failed: {0} {1}")]
    ScriptFailed(String, String),

    #[error("Some tests failed")]
    SomeTestsFailedError,

    #[error("Other error: {0}")]
    Other(String),
}

#[derive(Clone, Serialize, Deserialize, DisplayAsJsonPretty, DebugAsJson)]
pub struct TestFailure {
    pub assert_diagnostic: AssertDiagnostic,
    pub spec: WaitSpec,
}

#[derive(Debug)]
pub struct TestFailures(pub Vec<TestFailure>);

pub struct FailedTest {
    pub test_name: String,
    pub step_name: String,
    pub failure: Error,
}

pub type SucceededTest = String;

pub type TestResult = std::result::Result<SucceededTest, FailedTest>;

#[derive(Clone, DebugAsJson, Serialize, Deserialize)]
pub struct AssertDiagnostic {
    pub expr: Expr,
    pub input: Vec<serde_json::Value>,
}

impl std::fmt::Display for TestFailures {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, test_failure) in self.0.iter().enumerate() {
            writeln!(f, "\nFailed condition {}", i+1)?;
            writeln!(f, "{test_failure}")?;
        }
        Ok(())
    }
}

impl std::fmt::Display for AssertDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "🔴 **Assertion Failed**")?;
        writeln!(f, "Failed Expression: {}", self.expr)?;
        writeln!(f, "Input Data:")?;
        for (i, input) in self.input.iter().enumerate() {
            writeln!(f, "  {}. {}", i + 1, input)?;
        }
        Ok(())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
