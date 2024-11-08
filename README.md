# Mr. Blackjack
Simple framework for testing Kubernetes operators

## Basic Usage

### Test Specs

A `test` consists of a number of `steps`.

Each `step` can have a `watch` specifying resources to watch for.
Contents of watched resources are stored in a bucket accessible via the `name` of the `watch`.

Each `step` can have a `apply` specifying files containing manifests to apply to the cluster.

Each `step` can have a `delete` specifying files containing manifests to delete from the cluster.

Each `step` can have a `wait` specifying a `condition` to await in a `target`ed bucket of watched resources.

```yaml
name: my-test
steps:
  - name: preconditions
    watch:
      - name: pre-pods
        group: ''
        version: v1
        kind: Pod
    apply:
      - file: preconditions.yaml
    wait:
      - target: pre-pods
        timeout: 20
        condition:
          and:
            - size: 3
            - all:
                status:
                  conditions:
                    - type: Ready
                      status: "True"
  - name: deploy-as-deployment
    watch:
      - name: crd-pods
        group: ''
        version: v1
        kind: Pod
        labels:
          app: my-custom-app
    apply:
      - file: sample-deployment.yaml
    wait:
      - target: crd-pods
        timeout: 30
        condition:
          and:
            - size: 3
            - all:
                status:
                  conditions:
                    - type: Ready
                      status: "True"
            - all:
                metadata:
                  ownerReferences:
                    - kind: ReplicaSet
  - name: deploy-as-statefulset
    apply:
      - file: sample-statefulset.yaml
    wait:
      - target: crd-pods
        timeout: 30
        condition:
          and:
            - size: 3
            - all:
                status:
                  conditions:
                    - type: Ready
                      status: "True"
            - all:
                metadata:
                  ownerReferences:
                    - kind: StatefulSet
```

### Running blackjack

Place the test spec in a `test.yaml` inside a directory, let's say `my-test-1`.
The directory `my-test-1` itself should be in a another directory that collects all your tests, let's say `test`.
```shell
cargo run --bin blackjack test
```
All files specified in the `apply` sections should be relative to the test directory `my-test-1`.

In the above example, the directory structure would look like:

```shell
test/my-test-1/test.yaml
test/my-test-1/preconditions.yaml
test/my-test-1/sample-deployment.yaml
test/my-test-1/sample-statefulset.yaml
test/my-test-2/test.yaml
test/my-test-2/...
```

Blackjack will overrnamee all namespaces of the resources it applies with a randomly generated namespace.

Blackjack will cleanup all resources it has applied after the tests are finished.

All test directories in `test` will be run in parallel.


### Condition Expression

An expression can be `not` specifying an expression negated by logical NOT.

An expression can be `and` specifying an array of expressions connected by logical AND.

An expression can be `or` specifying an array of expressions connected by logical OR.

An expression can be `size` specifying the expected number of resources recorded in the `target` bucket.

An expression can be `all` specifying a partial object that needs to match against all resources recorded in the `target` bucket.

An expression can be `one` specifying a partial object that needs to match against at least one resources recorded in the `target` bucket.

## Full Schema for Test Spec

```yaml
$schema: http://json-schema.org/draft-07/schema#
title: TestSpec
type: object
required:
  - name
properties:
  name:
    type: string
  steps:
    default: []
    type: array
    items:
      $ref: '#/definitions/StepSpec'
definitions:
  ApplySpec:
    anyOf:
      - type: object
        required:
          - file
        properties:
          file:
            type: string
      - type: object
        required:
          - dir
        properties:
          dir:
            type: string
  AssertSpec:
    type: object
    required:
      - condition
      - target
    properties:
      condition:
        $ref: '#/definitions/Expr'
      target:
        type: string
  Expr:
    anyOf:
      - type: object
        required:
          - and
        properties:
          and:
            type: array
            items:
              $ref: '#/definitions/Expr'
      - type: object
        required:
          - or
        properties:
          or:
            type: array
            items:
              $ref: '#/definitions/Expr'
      - type: object
        required:
          - not
        properties:
          not:
            $ref: '#/definitions/Expr'
      - type: object
        required:
          - size
        properties:
          size:
            type: integer
            format: uint
            minimum: 0.0
      - type: object
        required:
          - one
        properties:
          one: true
      - type: object
        required:
          - all
        properties:
          all: true
  StepSpec:
    type: object
    required:
      - name
    properties:
      apply:
        default: []
        type: array
        items:
          $ref: '#/definitions/ApplySpec'
      assert:
        default: []
        type: array
        items:
          $ref: '#/definitions/AssertSpec'
      name:
        type: string
      wait:
        default: []
        type: array
        items:
          $ref: '#/definitions/WaitSpec'
      watch:
        default: []
        type: array
        items:
          $ref: '#/definitions/WatchSpec'
  WaitSpec:
    type: object
    required:
      - condition
      - target
      - timeout
    properties:
      condition:
        $ref: '#/definitions/Expr'
      target:
        type: string
      timeout:
        type: integer
        format: uint32
        minimum: 0.0
  WatchSpec:
    type: object
    required:
      - name
    properties:
      fields:
        default: null
        type:
          - object
          - "null"
        additionalProperties:
          type: string
      group:
        default: ""
        type: string
      name:
        type: string
      kind:
        default: ""
        type: string
      labels:
        default: null
        type:
          - object
          - "null"
        additionalProperties:
          type: string
      version:
        default: ""
        type: string
```

## Known Issues

* You should not have `kind: Namespace` resources in the manifests you let blackjack apply.
Currently, this will lead to the entire namespace being deleted after the test finished.
