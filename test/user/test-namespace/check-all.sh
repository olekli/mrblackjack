#!/bin/sh

test "$(kubectl get pod --all-namespaces --selector app=nginx-namespace-test -o json | yq '.items | length')" = "9"
