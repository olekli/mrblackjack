#!/bin/sh

kubectl apply -f nginx-deployment.yaml -n ${BLACKJACK_NAMESPACE}
