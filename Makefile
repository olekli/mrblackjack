default: build

build:
	cargo test

run:
	BLACKJACK_LOG_LEVEL=blackjack=debug cargo run --bin blackjack test

run-info:
	BLACKJACK_LOG_LEVEL=blackjack=info cargo run --bin blackjack test

.PHONY: test
test:
	cargo run --bin blackjack test
