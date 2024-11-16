#!/bin/sh

kubectl apply -f nginx-deployment.yaml -n default && export BLACKJACK_IMAGE=nginx:1.14.2
