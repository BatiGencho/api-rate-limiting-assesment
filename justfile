# List available commands
default:
    @just --list

# === Development Setup ===

# Start infrastructure (PostgreSQL + Redis)
up:
    docker-compose up -d

# Run database migrations
migrate:
    diesel migration run --migration-dir db/migrations

# Stop infrastructure
down:
    docker-compose down

# === Running the API ===

# Run the API server in dev mode
run:
    cargo run --bin api

# Run the API server in release mode
run-releave:
    cargo run --bin api --release

# Run with development environment variables
run-dev:
    DATABASE_URL=postgres://postgres:postgres@localhost:5432/transaction_queue \
    REDIS_URL=redis://localhost:6379 \
    RUST_LOG=transaction_queue_api=debug,error,info,tower_http=debug \
    cargo run --bin api

# Run with release environment variables
run-release:
    DATABASE_URL=postgres://postgres:postgres@localhost:5432/transaction_queue \
    REDIS_URL=redis://localhost:6379 \
    RUST_LOG=transaction_queue_api=debug,error,info,tower_http=debug \
    cargo run --bin api --release

# === Testing ===

# Complete test environment setup (recommended for candidates)
test-setup:
    @echo "🔧 Setting up complete test environment..."
    @echo "Starting infrastructure (PostgreSQL + Redis)..."
    docker-compose up -d postgres redis
    @echo "Waiting for services to be ready..."
    @sleep 5
    @echo "Running database migrations..."
    diesel migration run --migration-dir db/migrations
    @echo "Starting API server in background..."
    @echo "Run this command in a separate terminal: just run-dev"
    @echo ""
    @echo "🎯 Test environment ready! You can now run:"
    @echo "   just test-validate    # Run all integration tests"
    @echo "   just load-test        # Run performance tests"
    @echo "   just test-complete    # Run everything"
    @echo ""
    @echo "💡 Tip: Keep the API server running with 'just run-dev'"

# Quick test - run all non-performance tests (most common)
test-quick:
    @echo "🚀 Running all non-performance tests..."
    cargo test --workspace
    @echo "✅ All non-performance tests passed!"

# Step-by-step validation with progress output
test-validate:
    @echo "🧪 Validating implementation completeness..."
    @echo "Running integration tests..."
    cargo test --test integration_test -- --nocapture
    @echo "✅ Integration tests passed"
    @echo "Running rate limiting tests..."
    cargo test --test rate_limiting_test -- --nocapture
    @echo "✅ Rate limiting tests passed"
    @echo "Running queue management tests..."
    cargo test --test queue_management_test -- --nocapture
    @echo "✅ Queue management tests passed"
    @echo "Running edge case tests..."
    cargo test --test edge_cases_test -- --nocapture
    @echo "✅ Edge case tests passed"
    @echo ""
    @echo "🎉 All integration tests passed! Implementation is complete."

# Run all tests including performance tests
test:
    cargo test --workspace -- --include-ignored

# Individual test suites
test-integration:
    cargo test --test integration_test

test-rate-limiting:
    cargo test --test rate_limiting_test

test-queue:
    cargo test --test queue_management_test

test-edge:
    cargo test --test edge_cases_test

# === Performance Tests (require API running) ===

# 10k concurrent request test
load-test:
    cargo test --test load_test --release -- test_10k_concurrent_requests --ignored --nocapture

# Rate limiting under load test
rate-limit-test:
    cargo test --test load_test --release -- test_rate_limit_under_load --ignored --nocapture

# All performance tests
perf-tests: load-test rate-limit-test

# Complete test suite (integration + performance)
test-complete:
    @echo "🧪 Running COMPLETE test suite..."
    @echo "⚠️  Make sure API is running: just run-dev"
    @echo ""
    just test-validate
    @echo ""
    @echo "🚀 Running performance tests..."
    just perf-tests
    @echo ""
    @echo "🎉 ALL TESTS PASSED! Implementation is complete and performant."

# === Code Quality ===

# Format code
fmt:
    cargo fmt --all

# Run clippy linter
clippy:
    cargo clippy --all-targets --all-features -- -D warnings

# Run CI checks (format, lint, test)
ci: fmt clippy test-quick