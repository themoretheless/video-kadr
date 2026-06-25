.PHONY: dev backend frontend build check test lint fmt

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
	cd frontend && npm run build

test:
	cd backend && cargo test
	cd frontend && npm run test

lint:
	cd backend && cargo clippy --all-targets -- -D warnings
	cd frontend && npm run lint

fmt:
	cd backend && cargo fmt
