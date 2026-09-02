.PHONY: dev backend frontend build check test lint fmt bench-persistence perf-baseline perf-profile

dev:
	bash scripts/dev.sh

backend:
	cd backend && cargo run

frontend:
	cd frontend && npm run dev

build:
	cd backend && cargo build --release
	cd frontend && npm ci
	cd frontend && npm run build

# Mirror CI: format, lint, type-check, tests and build for both sides.
check:
	cd backend && cargo fmt --check
	cd backend && cargo clippy --all-targets -- -D warnings
	cd backend && cargo test
	cd frontend && npm run lint
	cd frontend && npm run typecheck
	cd frontend && npm run test
	cd frontend && npm run check:tokens
	cd frontend && npm run check:lifecycle
	cd frontend && npm run build
	cd frontend && npm run build-storybook
	cd frontend && npm run check:bundle
	cd frontend && npm run test:e2e

test:
	cd backend && cargo test
	cd frontend && npm run test

lint:
	cd backend && cargo clippy --all-targets -- -D warnings
	cd frontend && npm run lint

fmt:
	cd backend && cargo fmt

bench-persistence:
	cd backend && cargo bench --bench persistence

perf-baseline:
	bash bench/perf/run.sh

perf-profile:
	bash bench/perf/profile.sh $(or $(WORKLOAD),plan-compile)
