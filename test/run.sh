#!/bin/sh

cargo run --bin blackjack -- --parallel "$MINIKUBE_CPUS" test &&
! cargo run --bin blackjack -- --parallel "$MINIKUBE_CPUS" --timeout-scaling 0 test &&
echo success
