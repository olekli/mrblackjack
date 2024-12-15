default: build

build:
	cargo test --color always 2>&1 | less -R

run:
	BLACKJACK_LOG_LEVEL=blackjack=debug cargo run --bin blackjack test

run-info:
	BLACKJACK_LOG_LEVEL=blackjack=info cargo run --bin blackjack test

.PHONY: test
test:
	sh test/run.sh

schema/test_spec.yaml: src/test_spec.rs
	cargo run --bin make-schema | yq -P > $@

publish:
	cargo test && sh test/run.sh && cargo publish
