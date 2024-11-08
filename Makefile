default: build

build:
	cargo test

run:
	BLACKJACK_LOG_LEVEL=blackjack=trace cargo run --bin blackjack test

run-info:
	BLACKJACK_LOG_LEVEL=blackjack=info cargo run --bin blackjack test

.PHONY: test
test:
	cargo run --bin blackjack test

schema/test_spec.yaml: src/test_spec.rs
	cargo run --bin make-schema > $@
