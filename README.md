# Mr. Blackjack
Simple framework for testing Kubernetes operators

## Basic Usage

### Test Specs

A `test` consists of a number of `steps`.

Each `step` can have a `watch` list, specifying resources to watch for
by group, version and kind as well as label and field selectors.
All resources matching the specifications are stored in a bucket under the `name` of this `watch` entry.
The later tests will consist of conditions that have to hold true for the contents of a bucket.

Each `step` can have a `bucket` list, allowing to set the operations on a bucket.
Operations are `Create`, `Patch`, `Delete`.
If a `watch` sees a new resource and its bucket allows `Create`,
the resource is stored in the bucket.
If a `watch` sees a changed resource and its bucket allows `Patch`,
the resource stored is updated.
If a `watch` sees a resource deletion and its bucket allows `Delete`,
the resource stored is removed from the bucket.

You can, for example, collect all Pods matching some label in one step,
then set the bucket to `Delete` only in the next step.
If the Pods are restarted now, they disappear from the bucket,
but the newly started ones do not appear.

Each `step` can have an `apply` list specifying files containing manifests to apply to the cluster.

Each `step` can have a `delete` list specifying files containing manifests to delete from the cluster.

Each `step` can have a `sleep` value specifying a number of seconds to wait unconditionally after all `apply`s and `delete`s.

Each `step` can have a `wait` list specifying `condition`s to await in a `target`ed bucket of watched resources.

For examples, please see the tests in `test/`.

### Running blackjack

```shell
cargo run --bin blackjack TEST-DIR
```
All tests have to be named `test.yaml`.
Blackjack will discover all `test.yaml` in the directory tree starting at `TEST-DIR`.
All files specified in the `apply` sections have to be relative to the directory where the corresponding `test.yaml` resides.

All test found will be run in parallel.

For each test, blackjack will override all namespaces of the resources it applies with a randomly generated namespace.

For each test, blackjack will clean up all resources it has applied after the tests are finished.


### Condition Expression

An expression can be `not` specifying an expression negated by logical NOT.

An expression can be `and` specifying an array of expressions connected by logical AND.

An expression can be `or` specifying an array of expressions connected by logical OR.

An expression can be `size` specifying the expected number of resources recorded in the `target` bucket.

An expression can be `all` specifying a partial object that needs to match against all resources recorded in the `target` bucket.

An expression can be `one` specifying a partial object that needs to match against at least one resources recorded in the `target` bucket.

### Full Schema for Test Spec

The schema can be found in `schema/test_spec.yaml`.

## Known Issues
