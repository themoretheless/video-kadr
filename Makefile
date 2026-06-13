.PHONY: dev backend frontend build check test

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

check:
	cd backend && cargo check
	cd frontend && npm run build

test:
	cd backend && cargo test
