.PHONY: dev dev-down reset-volumes db-migrate test lint smoke-test ci

dev:
	docker compose rm -sf postgres-setup clickhouse-setup
	docker compose up -d --build
	@echo "Stack ready. Run 'docker compose up smoke-test' to verify."

dev-down:
	docker compose down

reset-volumes:
	bash scripts/reset-dev-volumes.sh

db-migrate:
	docker compose rm -sf postgres-setup clickhouse-setup
	docker compose up postgres-setup clickhouse-setup
	@echo "Migrations applied."

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
