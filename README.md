# Mr. Blackjack
Simple framework for testing Kubernetes operators

## Overview

`blackjack` provides end-to-end testing by:
- Watching for resources that match certain patterns (based on group, version, kind, and selectors).
- Storing all observed resources in named "buckets" for later checks.
- Applying and deleting Kubernetes manifests.
- Running scripts to modify the cluster state or verify conditions.
- Waiting for defined conditions on watched resources to be met.

Each test is defined in a `test.yaml` according to a specified schema. Tests can be retried, ordered, and categorized by type. Individual test steps can have multiple operations (watch, bucket configuration, apply, delete, script, sleep, wait) to fully automate and validate complex cluster states.

## Basic Usage

1. Create a `test.yaml` file (or multiple files in a directory structure) describing your test using the schema.
2. Run the `blackjack` binary with the directory containing your tests.
```shell
cargo run --bin blackjack TEST-DIR
```

`blackjack` will:
- Discover all `test.yaml` files recursively under the specified directory.
- For each test, override namespaces in applied manifests with a randomly generated namespace, unique to the test run.
- After the test completes (pass or fail), it will clean up all resources it applied.

For reference, see the examples in `test/`.

## Test Specification

A test specification is defined by a top-level object (see `schema/test_spec.yaml` for the full schema):

### Top-Level Test Fields

- **name** (string):
  The name of the test. Defaults to an empty string.

- **attempts** (integer or null):
  On failure, the test can be retried for a specified number of attempts in total. Defaults to `null`, meaning no additional retries.

- **ordering** (string or null):
  A string to determine test ordering via lexicographical comparison. If two tests have the same type and concurrency rules, this string can be used to order them. Defaults to `null`.

- **type** (enum: `cluster` or `user`):
  Specifies the type of test. Defaults to `user`.
  - **`cluster`** tests are run first and not concurrently with `user` tests.
  - **`user`** tests can run concurrently, with concurrency limits defined by command line arguments.

- **steps** (array):
  A list of test steps. Each step describes a phase of the test with various operations (watch, apply, delete, script, sleep, bucket operations, wait). Each step is defined by a `StepSpec`.

### Steps (StepSpec)

Each step is an object with the following fields:

- **name** (string, required):
  A name for the step. Used for identification and logging.

- **watch** (array of WatchSpec):
  A list of watches to start. Starting a watch sets up a "bucket" that reflects the state of resources matching the given criteria. By default, all operations (create, patch, delete) are recorded unless later modified by bucket operations.
  Each `WatchSpec` can specify:
  - **name** (string, required): Bucket name to store observed resources.
  - **group** (string): Resource group to watch. Defaults to `""` (core group).
  - **version** (string): Resource version (e.g., `v1`). Defaults to `""`.
  - **kind** (string): Resource kind (e.g., `Pod`, `Deployment`). Defaults to `""`.
  - **namespace** (string): Namespace to watch. Defaults to `${BLACKJACK_NAMESPACE}`, the unique namespace created for this test run.
  - **labels** (object or null): A map of label key-value pairs to filter watched resources by label selectors. Defaults to `null`.
  - **fields** (object or null): A map of field selectors. Defaults to `null`.

- **bucket** (array of BucketSpec):
  Modify existing watch buckets to reflect only certain events. For example, you may choose not to record resource deletions or patches.
  Each `BucketSpec` includes:
  - **name** (string, required): Name of the bucket (as defined by a `watch`).
  - **operations** (array of BucketOperation, required): Which operations should be reflected.
    Possible values:
    - `create`: Newly created matching resources are recorded.
    - `patch`: Updated resources are recorded upon modifications.
    - `delete`: Deleted resources are removed from the bucket.

  By omitting an operation, the corresponding changes will not be reflected in the bucket.

- **apply** (array of ApplySpec):
  Apply Kubernetes manifests to the cluster.
  Each `ApplySpec` includes:
  - **path** (string, required): Path to a manifest file or directory of manifests.
  - **namespace** (string): Namespace override. Defaults to `${BLACKJACK_NAMESPACE}`.
  - **override-namespace** (boolean): Whether to override namespace specifications in the manifests. Defaults to `true`.

- **delete** (array of ApplySpec):
  Delete Kubernetes manifests from the cluster. The fields are the same as `apply`, but these resources will be removed.

- **script** (array of strings):
  A list of paths to shell scripts to run. These scripts are sourced by `sh`, and all `BLACKJACK_` prefixed environment variables are available in them. Scripts that exit non-zero cause the test to fail.

- **sleep** (integer):
  Sleep unconditionally for the specified number of seconds. Defaults to `0`.

- **wait** (array of WaitSpec):
  Wait until certain conditions are met for the resources in a specific bucket.

  Each `WaitSpec` includes:
  - **condition** (Expr, required): A logical expression describing the condition to check.
  - **target** (string, required): The name of the bucket to check.
  - **timeout** (integer, required): How many seconds to wait for the condition. If the condition is not met in time, the test fails.

### Condition Expressions (Expr)

Conditions control the logic for `wait` steps. Expressions can be combined with logical operators:

- **and**: An array of expressions all of which must be true.
- **or**: An array of expressions at least one of which must be true.
- **not**: A single expression whose truth value is negated.
- **size**: A numeric check that the number of resources in the target bucket matches a certain integer.
- **one**: Checks that at least one resource in the target bucket matches a certain pattern (partial object match).
- **all**: Checks that all resources in the target bucket match a certain pattern (partial object match).

The `one` and `all` checks are represented as boolean fields in the schema. In practice, these would be used in conjunction with additional logic to define the pattern that the resources must match.

### Test Type

As mentioned, tests have a `type` which can be either `cluster` or `user`:
- **cluster** tests run first and exclusively before `user` tests start.
- **user** tests can run concurrently after all `cluster` tests have completed.

This allows for a clear separation of "setup" or "integration" tests from more common "user scenario" tests.

### Retries

The `attempts` field defines how many times a test can be retried if it fails. By default, `null` means it is not retried beyond the initial attempt.

### Ordering

The `ordering` field is used to lexicographically order tests of the same type and within the same concurrency limits. This ensures a deterministic test run order if desired.

### Full Schema for Test Spec

The full schema is located in `schema/test_spec.yaml`. This document is a high-level description of the fields and how they relate.

**Key elements from the schema:**
- **ApplySpec**: How manifests are applied.
- **BucketSpec**: How watch buckets reflect changes (create/patch/delete).
- **WaitSpec**: Conditions and timeouts for waiting on bucket states.
- **WatchSpec**: How resources are watched and recorded.
- **Expr**: Logical condition structures for `wait` steps.
- **TestType**: Distinguish between `cluster` and `user` tests.

Refer to the schema for precise validation rules and defaults.

## Known Issues
