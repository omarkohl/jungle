# Run all checks (mirrors CI)
check:
    cargo fmt --check
    cargo clippy --all-targets -- -D warnings
    cargo nextest run

# Format code
fmt:
    cargo fmt

# Run integration tests only
integration:
    cargo nextest run --test integration

# Run tests in watch mode
dev:
    bacon test
