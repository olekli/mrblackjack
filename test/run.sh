#!/bin/sh

export BLACKJACK_LOG_LEVEL=blackjack=debug
cargo run --bin blackjack -- --parallel "$MINIKUBE_CPUS" test &&
! cargo run --bin blackjack -- --parallel "$MINIKUBE_CPUS" --timeout-scaling 0 test/user &&
echo && echo TESTS PASSED!
