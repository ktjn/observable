.PHONY: dev dev-down test lint migrate

dev:
	cp -n .env.local.example .env.local 2>/dev/null || true
	docker compose --env-file .env.local up -d --wait
	@echo "Stack ready. Run 'cargo run -p <service>' or 'npm run dev --workspace=apps/frontend'"

dev-down:
	docker compose down

test:
	cargo test --workspace
	npm run typecheck --workspace=apps/frontend

lint:
	cargo fmt --check
	cargo clippy --all-targets -- -D warnings
	npm run lint --workspace=apps/frontend

migrate:
	@echo "Running ClickHouse migrations..."
	for f in migrations/clickhouse/*.sql; do \
	  clickhouse-client --host localhost --query "$$(cat $$f)"; \
	done
	@echo "Running PostgreSQL migrations..."
	DATABASE_URL=$$(grep DATABASE_URL .env.local | cut -d= -f2) \
	  sqlx migrate run --source migrations/postgres
