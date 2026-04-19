.PHONY: dev dev-down test lint smoke-test ci

dev:
	docker compose up -d
	@echo "Stack ready. Run 'docker compose up smoke-test' to verify."

dev-down:
	docker compose down

test:
	cargo test --workspace
	npm run typecheck --workspace=apps/frontend

lint:
	cargo fmt --check
	cargo clippy --all-targets -- -D warnings
	npm run lint --workspace=apps/frontend

smoke-test:
	docker compose up smoke-test --abort-on-container-exit

ci:
	bash scripts/ci.sh
