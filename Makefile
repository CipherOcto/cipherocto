# CipherOcto development Makefile
# Coverage and testing targets

.PHONY: test test-l1 test-l2 test-l3 test-l4 coverage coverage-diff fmt clippy

# Run all CI tests (L1 + L2)
test:
	cargo test --workspace

# Run only L1 unit tests
test-l1:
	cargo test --workspace --lib

# Run only L2 integration tests
test-l2:
	cargo test --manifest-path crates/quota-router-integration-tests/Cargo.toml

# Run L3 cross-process TCP tests (manual, requires built CLI)
test-l3:
	cargo test --manifest-path crates/quota-router-integration-tests/Cargo.toml -- --ignored l3_

# Run L4 docker tests (manual, requires Docker engine)
test-l4:
	cargo test --manifest-path crates/quota-router-integration-tests/Cargo.toml -- --ignored layer4_

# Full workspace coverage report
coverage:
	cargo tarpaulin --workspace --skip-clean --out stdout

# Coverage with HTML report
coverage-html:
	cargo tarpaulin --workspace --skip-clean --out Html --output-dir coverage/
	@echo "Report: coverage/index.html"

# Coverage diff against baseline (requires baseline.txt)
coverage-diff:
	@echo "=== Current coverage ==="
	cargo tarpaulin --workspace --skip-clean --out stdout 2>&1 | grep "^|| " | sort > /tmp/current.txt
	@if [ -f baseline.txt ]; then \
		echo "=== Diff vs baseline ==="; \
		diff --color baseline.txt /tmp/current.txt || true; \
	else \
		echo "No baseline.txt found. Run 'make coverage > baseline.txt' to create one."; \
	fi

# Format all code
fmt:
	cargo fmt --all

# Check formatting
fmt-check:
	cargo fmt --all -- --check

# Run clippy
clippy:
	cargo clippy --workspace --all-targets -- -D warnings

# Build CLI binary
build-cli:
	cargo build -p quota-router-cli

# Clean build artifacts
clean:
	cargo clean
