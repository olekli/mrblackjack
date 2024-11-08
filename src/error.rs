// Copyright 2024 Ole Kliemann
// SPDX-License-Identifier: Apache-2.0

use crate::test_spec::Expr;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("File error: {0}")]
    FileError(#[from] std::io::Error),

    #[error("Discovery error: {group}/{version} {kind}")]
    DiscoveryError {
        group: String,
        version: String,
        kind: String,
    },

    #[error("Watcher error: {0}")]
    WatcherError(#[from] kube::runtime::watcher::Error),

    #[error("Kube error: {0}")]
    KubeError(#[from] kube::Error),

    #[error("Serialization error: {0}")]
    SerializationYamlError(#[from] serde_yaml::Error),

    #[error("Serialization error: {0}")]
    SerializationJsonError(#[from] serde_json::Error),

    #[error("Multiple errors: {0:?}")]
    MultipleErrors(Vec<Error>),

    #[error("NamespaceExists")]
    NamespaceExists,

    #[error("Tests failed: {0:?}")]
    TestFailures(Vec<TestFailure>),

    #[error("Path encoding error")]
    PathEncodingError,

    #[error("SIGINT")]
    SIGINT,

    #[error("Other error: {0}")]
    Other(String),
}

#[derive(Debug, thiserror::Error)]
pub enum TestFailure {
    #[error("Missed wait: {0}")]
    MissedWait(AssertDiagnostic),

    #[error("Failed assert: {0}")]
    FailedAssert(AssertDiagnostic),
}

pub struct FailedTest {
    pub test_name: String,
    pub step_name: String,
    pub failure: Error,
}

pub type SucceededTest = String;

pub type TestResult = std::result::Result<SucceededTest, FailedTest>;

#[derive(Clone, Debug)]
pub struct AssertDiagnostic {
    pub expr: Expr,
    pub input: Vec<serde_json::Value>,
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
