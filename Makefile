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
	bash scripts/migrate.sh
