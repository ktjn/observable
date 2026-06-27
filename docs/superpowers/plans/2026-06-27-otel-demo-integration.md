# OTel Demo Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the custom testbench shop with the official OpenTelemetry Demo (v2.2.0), routing all of its telemetry through Observable's ingest-gateway instead of Jaeger/Prometheus/Grafana.

**Architecture:** A vendored `demos/otel-demo/docker-compose.yml` (published `ghcr.io/open-telemetry/demo:2.2.0-*` images, no build blocks) is pulled into the root compose via `include:`. The OTel demo's internal `otel-collector` service is reconfigured with a minimal custom config that forwards all signals (OTLP gRPC + HTTP) to `ingest-gateway:4317`. Jaeger, Grafana, Prometheus, and OpenSearch are dropped entirely. The five custom shop testbench services and the `testbench/` directory are deleted.

**Tech Stack:** Docker Compose `include:`, `ghcr.io/open-telemetry/demo:2.2.0-*`, `ghcr.io/open-telemetry/opentelemetry-collector-releases/opentelemetry-collector-contrib:0.142.0`, PostgreSQL 17.6, Valkey 9.0.1, flagd v0.12.9

## Global Constraints

- OTel demo pinned to `2.2.0` — use image tag `ghcr.io/open-telemetry/demo:2.2.0-<service>` everywhere
- API key plaintext: `otel-demo-api-key-0000`; SHA-256: `030125c5cc858af2101f76252b43d2584542ed00525857398835e21e91d126c7`
- No Docker Compose profiles — OTel demo starts with plain `docker compose up`, same as `crypto-demo`
- Collector must accept OTLP/gRPC on port 4317 **and** OTLP/HTTP on port 4318 — services use both
- The OTel demo has its own `postgresql` container (service name `postgresql`, separate from Observable's `postgres`) — no conflict
- The `otel-collector` service exposes container ports 4317/4318 only (no host port binding) — Observable's `ingest-gateway` already binds those host ports
- `crypto-demo` service in the root compose is untouched
- Follow existing postgres migration naming: next available is `034_`

---

## Files Created / Modified / Deleted

| File | Action |
|---|---|
| `migrations/postgres/034_add_otel_demo_tenant.sql` | Create |
| `demos/otel-demo/otelcol-config.yml` | Create |
| `demos/otel-demo/src/flagd/demo.flagd.json` | Create (vendored) |
| `demos/otel-demo/src/postgresql/init.sql` | Create (vendored) |
| `demos/otel-demo/docker-compose.yml` | Create |
| `docker-compose.yml` | Modify (add `include:`, remove shop services + volume) |
| `testbench/` | Delete entirely |

---

### Task 1: Postgres migration for otel-demo tenant

**Files:**
- Create: `migrations/postgres/034_add_otel_demo_tenant.sql`

**Interfaces:**
- Produces: tenant row `otel-demo` with UUID `00000000-0000-0000-0000-000000000004`; API key row for `otel-demo-api-key-0000` (SHA-256 `030125c5cc858af2101f76252b43d2584542ed00525857398835e21e91d126c7`)

- [ ] **Step 1: Create the migration file**

```sql
-- migrations/postgres/034_add_otel_demo_tenant.sql
-- Tenant and API key for the OpenTelemetry Demo app.
-- The otel-demo otel-collector sends signals to ingest-gateway using this key.

INSERT INTO tenants (id, name) VALUES
    ('00000000-0000-0000-0000-000000000004', 'otel-demo')
ON CONFLICT DO NOTHING;

-- API key for otel-demo OTel ingest
-- Plaintext: "otel-demo-api-key-0000"
-- SHA-256:   030125c5cc858af2101f76252b43d2584542ed00525857398835e21e91d126c7
INSERT INTO api_keys (tenant_id, key_hash, name, environment, role) VALUES (
    '00000000-0000-0000-0000-000000000004',
    '030125c5cc858af2101f76252b43d2584542ed00525857398835e21e91d126c7',
    'otel-demo-ingest',
    'otel-demo',
    'member'
) ON CONFLICT DO NOTHING;
```

- [ ] **Step 2: Verify the SHA-256 matches**

```bash
echo -n "otel-demo-api-key-0000" | sha256sum
# Expected: 030125c5cc858af2101f76252b43d2584542ed00525857398835e21e91d126c7
```

- [ ] **Step 3: Commit**

```bash
git add migrations/postgres/034_add_otel_demo_tenant.sql
git commit -m "feat(migrations): add otel-demo tenant and API key"
```

---

### Task 2: OTel demo support files

**Files:**
- Create: `demos/otel-demo/otelcol-config.yml`
- Create: `demos/otel-demo/src/flagd/demo.flagd.json`
- Create: `demos/otel-demo/src/postgresql/init.sql`

**Interfaces:**
- `otelcol-config.yml` consumed by `otel-collector` service in Task 3 (mounted at `/etc/otelcol-config.yml`)
- `src/flagd/demo.flagd.json` consumed by `flagd` service (mounted at `/etc/flagd/demo.flagd.json`)
- `src/postgresql/init.sql` consumed by `postgresql` service (Docker entrypoint init)

- [ ] **Step 1: Create the collector config**

```yaml
# demos/otel-demo/otelcol-config.yml
# Minimal collector: accepts OTLP from all demo services, forwards to Observable ingest-gateway.
# Replaces the upstream config that routes to Jaeger/Prometheus/OpenSearch.
receivers:
  otlp:
    protocols:
      grpc:
        endpoint: "0.0.0.0:4317"
      http:
        endpoint: "0.0.0.0:4318"
        cors:
          allowed_origins:
            - "http://*"
            - "https://*"

processors:
  memory_limiter:
    check_interval: 5s
    limit_percentage: 80
    spike_limit_percentage: 25
  batch:

exporters:
  otlp/observable:
    endpoint: "ingest-gateway:4317"
    headers:
      authorization: "Bearer otel-demo-api-key-0000"
    tls:
      insecure: true

service:
  pipelines:
    traces:
      receivers: [otlp]
      processors: [memory_limiter, batch]
      exporters: [otlp/observable]
    metrics:
      receivers: [otlp]
      processors: [memory_limiter, batch]
      exporters: [otlp/observable]
    logs:
      receivers: [otlp]
      processors: [memory_limiter, batch]
      exporters: [otlp/observable]
```

- [ ] **Step 2: Create the flagd feature flags config**

```json
{
  "$schema": "https://flagd.dev/schema/v0/flags.json",
  "flags": {
    "llmInaccurateResponse": {
      "defaultVariant": "off",
      "description": "LLM returns an inaccurate product summary for product ID L9ECAV7KIM",
      "state": "ENABLED",
      "variants": { "off": false, "on": true }
    },
    "llmRateLimitError": {
      "defaultVariant": "off",
      "description": "LLM intermittently returns a rate limit error",
      "state": "ENABLED",
      "variants": { "off": false, "on": true }
    },
    "productCatalogFailure": {
      "description": "Fail product catalog service on a specific product",
      "state": "ENABLED",
      "variants": { "on": true, "off": false },
      "defaultVariant": "off"
    },
    "recommendationCacheFailure": {
      "description": "Fail recommendation service cache",
      "state": "ENABLED",
      "variants": { "on": true, "off": false },
      "defaultVariant": "off"
    },
    "adManualGc": {
      "description": "Triggers full manual garbage collections in the ad service",
      "state": "ENABLED",
      "variants": { "on": true, "off": false },
      "defaultVariant": "off"
    },
    "adHighCpu": {
      "description": "Triggers high cpu load in the ad service",
      "state": "ENABLED",
      "variants": { "on": true, "off": false },
      "defaultVariant": "off"
    },
    "adFailure": {
      "description": "Fail ad service",
      "state": "ENABLED",
      "variants": { "on": true, "off": false },
      "defaultVariant": "off"
    },
    "kafkaQueueProblems": {
      "description": "Overloads Kafka queue while simultaneously introducing a consumer side delay leading to a lag spike",
      "state": "ENABLED",
      "variants": { "on": 100, "off": 0 },
      "defaultVariant": "off"
    },
    "cartFailure": {
      "description": "Fail cart service",
      "state": "ENABLED",
      "variants": { "on": true, "off": false },
      "defaultVariant": "off"
    },
    "paymentFailure": {
      "description": "Fail payment service charge requests n%",
      "state": "ENABLED",
      "variants": { "100%": 1, "90%": 0.95, "75%": 0.75, "50%": 0.5, "25%": 0.25, "10%": 0.1, "off": 0 },
      "defaultVariant": "off"
    },
    "paymentUnreachable": {
      "description": "Payment service is unavailable",
      "state": "ENABLED",
      "variants": { "on": true, "off": false },
      "defaultVariant": "off"
    },
    "loadGeneratorFloodHomepage": {
      "description": "Flood the frontend with a large amount of requests.",
      "state": "ENABLED",
      "variants": { "on": 100, "off": 0 },
      "defaultVariant": "off"
    },
    "imageSlowLoad": {
      "description": "slow loading images in the frontend",
      "state": "ENABLED",
      "variants": { "10sec": 10000, "5sec": 5000, "off": 0 },
      "defaultVariant": "off"
    },
    "failedReadinessProbe": {
      "description": "readiness probe failure for cart service",
      "state": "ENABLED",
      "variants": { "on": true, "off": false },
      "defaultVariant": "off"
    },
    "emailMemoryLeak": {
      "description": "Memory leak in the email service.",
      "state": "ENABLED",
      "variants": { "off": 0, "1x": 1, "10x": 10, "100x": 100, "1000x": 1000, "10000x": 10000 },
      "defaultVariant": "off"
    }
  }
}
```
Save to `demos/otel-demo/src/flagd/demo.flagd.json`

- [ ] **Step 3: Create the PostgreSQL init script for the OTel demo's own database**

This initialises the OTel demo's private `postgresql` container (separate from Observable's `postgres`).

```sql
-- demos/otel-demo/src/postgresql/init.sql
-- Copyright The OpenTelemetry Authors
-- SPDX-License-Identifier: Apache-2.0

CREATE USER otelu WITH PASSWORD 'otelp';

CREATE SCHEMA accounting;
GRANT USAGE ON SCHEMA accounting TO otelu;

CREATE TABLE accounting."order" (
    order_id TEXT PRIMARY KEY
);

CREATE TABLE accounting.shipping (
    shipping_tracking_id TEXT PRIMARY KEY,
    shipping_cost_currency_code TEXT NOT NULL,
    shipping_cost_units BIGINT NOT NULL,
    shipping_cost_nanos INT NOT NULL,
    street_address TEXT,
    city TEXT,
    state TEXT,
    country TEXT,
    zip_code TEXT,
    order_id TEXT NOT NULL,
    FOREIGN KEY (order_id) REFERENCES accounting."order"(order_id) ON DELETE CASCADE
);

CREATE TABLE accounting.orderitem (
    item_cost_currency_code TEXT NOT NULL,
    item_cost_units BIGINT NOT NULL,
    item_cost_nanos INT NOT NULL,
    product_id TEXT NOT NULL,
    quantity INT NOT NULL,
    order_id TEXT NOT NULL,
    PRIMARY KEY (order_id, product_id),
    FOREIGN KEY (order_id) REFERENCES accounting."order"(order_id) ON DELETE CASCADE
);

GRANT SELECT, INSERT, UPDATE ON ALL TABLES IN SCHEMA accounting TO otelu;

CREATE SCHEMA reviews;
GRANT USAGE ON SCHEMA reviews TO otelu;

CREATE TABLE reviews.productreviews (
    id INTEGER GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
    product_id VARCHAR(16) NOT NULL,
    username VARCHAR(64) NOT NULL,
    description VARCHAR(1024),
    score NUMERIC(2,1) NOT NULL
);

CREATE INDEX product_id_index ON reviews.productreviews (product_id);
GRANT SELECT, INSERT, UPDATE ON ALL TABLES IN SCHEMA reviews TO otelu;

INSERT INTO reviews.productreviews (product_id, username, description, score) VALUES
    ('OLJCESPC7Z', 'stargazer_mike', 'Great entry-level telescope! Easy to set up and provides clear views of the moon and brighter planets. Highly recommend for new astronomers.', '4.5'),
    ('OLJCESPC7Z', 'nightskylover', 'For the price, this Explorascope delivers excellent performance. I was able to see Jupiter''s moons clearly. A fantastic purchase for casual viewing.', '4.0'),
    ('66VCHSJNUP', 'tech_astro', 'The StarSense app is revolutionary! It made finding celestial objects incredibly easy. This telescope is a game-changer for beginners.', '5.0'),
    ('66VCHSJNUP', 'app_user', 'Amazing technology, the smartphone integration works flawlessly. I''ve never had so much fun exploring the night sky. Worth every penny.', '4.5'),
    ('1YMWWN1N4O', 'solar_viewer', 'Perfect for solar observations! The Solar Safe filter gives peace of mind. I used it for the last partial eclipse and it was fantastic.', '5.0'),
    ('L9ECAV7KIM', 'clean_optics', 'This kit is a lifesaver for all my optics. The brush and wipes work perfectly without leaving any residue. My lenses have never been cleaner.', '5.0'),
    ('2ZYFJ3GM2N', 'bird_watcher', 'Incredible clarity and brightness, perfect for bird watching. The ED glass really makes a difference. I can spot the subtlest field markings.', '5.0'),
    ('0PUK6V6EV0', 'astro_photog', 'This imager is a fantastic step up for planetary photography. The color quality is superb. Easy to use with my existing telescope setup.', '5.0'),
    ('LS4PSXUNUM', 'night_walker', 'The red light is perfect for preserving night vision during astronomy sessions. The hand warmer is an unexpected bonus. Very practical device.', '5.0'),
    ('9SIQT8TOJO', 'deep_sky_master', 'The RASA V2 is a dream come true for deep-sky imaging. The f/2.2 speed drastically cuts down exposure times. My best astrophotography investment yet.', '5.0'),
    ('6E92ZMYYFZ', 'solar_safety', 'Essential for safe solar viewing with my 8-inch telescope. The Velcro straps ensure it stays securely in place. Peace of mind during solar observations.', '5.0'),
    ('HQTGWGPNH4', 'history_buff', 'A fascinating glimpse into historical astronomical thought. The content is incredibly insightful. A must-read for anyone interested in the history of science.', '5.0');

CREATE SCHEMA catalog;
GRANT USAGE ON SCHEMA catalog TO otelu;

CREATE TABLE catalog.products (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    picture TEXT,
    price_currency_code TEXT NOT NULL,
    price_units BIGINT NOT NULL,
    price_nanos INT NOT NULL,
    categories TEXT
);

GRANT SELECT ON ALL TABLES IN SCHEMA catalog TO otelu;

INSERT INTO catalog.products (id, name, description, picture, price_currency_code, price_units, price_nanos, categories) VALUES
    ('OLJCESPC7Z', 'National Park Foundation Explorascope', 'The NPF Explorascope 60AZ is a manual alt-azimuth refractor telescope perfect for celestial viewing on the go.', 'NationalParkFoundationExplorascope.jpg', 'USD', 101, 960000000, 'telescopes'),
    ('66VCHSJNUP', 'Starsense Explorer Refractor Telescope', 'The first telescope that uses your smartphone to analyze the night sky and calculate its position in real time.', 'StarsenseExplorer.jpg', 'USD', 349, 950000000, 'telescopes'),
    ('1YMWWN1N4O', 'Eclipsmart Travel Refractor Telescope', 'Dedicated white-light solar scope for the observer on the go.', 'EclipsmartTravelRefractorTelescope.jpg', 'USD', 129, 950000000, 'telescopes,travel'),
    ('L9ECAV7KIM', 'Lens Cleaning Kit', 'Wipe away dust, dirt, fingerprints and other particles on your lenses to see clearly.', 'LensCleaningKit.jpg', 'USD', 21, 950000000, 'accessories'),
    ('2ZYFJ3GM2N', 'Roof Binoculars', 'This versatile, all-around binocular is a great choice for the trail, the stadium, the arena, or just about anywhere.', 'RoofBinoculars.jpg', 'USD', 209, 950000000, 'binoculars'),
    ('0PUK6V6EV0', 'Solar System Color Imager', 'The NexImage 10 Solar System Imager is the perfect solution for planetary photography.', 'SolarSystemColorImager.jpg', 'USD', 175, 0, 'accessories,telescopes'),
    ('LS4PSXUNUM', 'Red Flashlight', 'This 3-in-1 device features a 3-mode red flashlight, a hand warmer, and a portable power bank.', 'RedFlashlight.jpg', 'USD', 57, 80000000, 'accessories,flashlights'),
    ('9SIQT8TOJO', 'Optical Tube Assembly', 'Capturing impressive deep-sky astroimages is easier than ever with Rowe-Ackermann Schmidt Astrograph V2.', 'OpticalTubeAssembly.jpg', 'USD', 3599, 0, 'accessories,telescopes,assembly'),
    ('6E92ZMYYFZ', 'Solar Filter', 'Enhance your viewing experience with EclipSmart Solar Filter for 8" telescopes.', 'SolarFilter.jpg', 'USD', 69, 950000000, 'accessories,telescopes'),
    ('HQTGWGPNH4', 'The Comet Book', 'A 16th-century treatise on comets, created anonymously in Flanders and now held at the Universitätsbibliothek Kassel.', 'TheCometBook.jpg', 'USD', 0, 990000000, 'books');
```

- [ ] **Step 4: Commit**

```bash
git add demos/otel-demo/otelcol-config.yml demos/otel-demo/src/flagd/demo.flagd.json demos/otel-demo/src/postgresql/init.sql
git commit -m "feat(otel-demo): add collector config and vendored support files"
```

---

### Task 3: Vendored docker-compose for the OTel demo

**Files:**
- Create: `demos/otel-demo/docker-compose.yml`

**Interfaces:**
- Consumes: `otelcol-config.yml`, `src/flagd/demo.flagd.json`, `src/postgresql/init.sql` from Task 2
- Produces: 21 services that Task 4 wires in via `include:`; `otel-collector` service (name used as `depends_on` target in Task 4 if needed)
- `otel-collector` references `ingest-gateway` by hostname — requires the root compose's default network (achieved by omitting any `networks:` block, so Docker Compose `include:` merges everything onto one network)

Key modifications from upstream `opentelemetry-demo v2.2.0`:
- All `build:` blocks removed — uses published images only
- `networks:` block removed — joins Observable's default project network
- Services removed: `jaeger`, `grafana`, `prometheus`, `opensearch`
- `otel-collector`: `depends_on` changed from `jaeger`/`opensearch` to `ingest-gateway`; config volume replaced with `./otelcol-config.yml`; `user: 0:0` and host filesystem mounts removed; command simplified to single config file
- `frontend-proxy`: `depends_on` on `jaeger`/`grafana` removed; dummy `GRAFANA_HOST=localhost`, `JAEGER_HOST=localhost` set so Envoy config resolves without those services
- `postgresql` (OTel demo's own postgres): adds named volume `otel_demo_postgresql_data` for data persistence; note service name is `postgresql`, completely separate from Observable's `postgres`

- [ ] **Step 1: Create `demos/otel-demo/docker-compose.yml`**

```yaml
# Vendored from opentelemetry-demo v2.2.0
# Modifications: removed build blocks, removed jaeger/grafana/prometheus/opensearch,
# otel-collector routes to ingest-gateway, networks block removed to join main project network.

x-logging: &logging
  driver: "json-file"
  options:
    max-size: "5m"
    max-file: "2"

x-otel-env: &otel-env
  OTEL_EXPORTER_OTLP_ENDPOINT: http://otel-collector:4317
  OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE: cumulative
  OTEL_RESOURCE_ATTRIBUTES: "service.namespace=opentelemetry-demo,service.version=2.2.0"

services:
  accounting:
    image: ghcr.io/open-telemetry/demo:2.2.0-accounting
    deploy:
      resources:
        limits:
          memory: 160M
    restart: unless-stopped
    environment:
      <<: *otel-env
      KAFKA_ADDR: kafka:9092
      OTEL_EXPORTER_OTLP_ENDPOINT: http://otel-collector:4318
      OTEL_SERVICE_NAME: accounting
      DB_CONNECTION_STRING: "Host=postgresql;Username=otelu;Password=otelp;Database=otel"
      OTEL_DOTNET_AUTO_TRACES_ENTITYFRAMEWORKCORE_INSTRUMENTATION_ENABLED: "false"
    depends_on:
      otel-collector:
        condition: service_started
      kafka:
        condition: service_healthy
    logging: *logging

  ad:
    image: ghcr.io/open-telemetry/demo:2.2.0-ad
    deploy:
      resources:
        limits:
          memory: 300M
    restart: unless-stopped
    ports:
      - "9555"
    environment:
      AD_PORT: "9555"
      FLAGD_HOST: flagd
      FLAGD_PORT: "8013"
      OTEL_EXPORTER_OTLP_ENDPOINT: http://otel-collector:4318
      OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE: cumulative
      OTEL_RESOURCE_ATTRIBUTES: "service.namespace=opentelemetry-demo,service.version=2.2.0"
      OTEL_LOGS_EXPORTER: otlp
      OTEL_SERVICE_NAME: ad
    depends_on:
      otel-collector:
        condition: service_started
      flagd:
        condition: service_started
    logging: *logging

  cart:
    image: ghcr.io/open-telemetry/demo:2.2.0-cart
    deploy:
      resources:
        limits:
          memory: 160M
    restart: unless-stopped
    ports:
      - "7070"
    environment:
      <<: *otel-env
      CART_PORT: "7070"
      FLAGD_HOST: flagd
      FLAGD_PORT: "8013"
      VALKEY_ADDR: valkey-cart:6379
      OTEL_SERVICE_NAME: cart
      ASPNETCORE_URLS: "http://*:7070"
    depends_on:
      valkey-cart:
        condition: service_started
      otel-collector:
        condition: service_started
      flagd:
        condition: service_started
    logging: *logging

  checkout:
    image: ghcr.io/open-telemetry/demo:2.2.0-checkout
    deploy:
      resources:
        limits:
          memory: 20M
    restart: unless-stopped
    ports:
      - "5050"
    environment:
      <<: *otel-env
      FLAGD_HOST: flagd
      FLAGD_PORT: "8013"
      CHECKOUT_PORT: "5050"
      CART_ADDR: cart:7070
      CURRENCY_ADDR: currency:7001
      EMAIL_ADDR: http://email:6060
      PAYMENT_ADDR: payment:50051
      PRODUCT_CATALOG_ADDR: product-catalog:3550
      SHIPPING_ADDR: http://shipping:50050
      KAFKA_ADDR: kafka:9092
      GOMEMLIMIT: 16MiB
      OTEL_SERVICE_NAME: checkout
    depends_on:
      cart:
        condition: service_started
      currency:
        condition: service_started
      email:
        condition: service_started
      payment:
        condition: service_started
      product-catalog:
        condition: service_started
      shipping:
        condition: service_started
      otel-collector:
        condition: service_started
      kafka:
        condition: service_healthy
      flagd:
        condition: service_started
    logging: *logging

  currency:
    image: ghcr.io/open-telemetry/demo:2.2.0-currency
    deploy:
      resources:
        limits:
          memory: 20M
    restart: unless-stopped
    ports:
      - "7001"
    environment:
      <<: *otel-env
      CURRENCY_PORT: "7001"
      IPV6_ENABLED: "false"
      VERSION: "2.2.0"
      OTEL_SERVICE_NAME: currency
    depends_on:
      otel-collector:
        condition: service_started
    logging: *logging

  email:
    image: ghcr.io/open-telemetry/demo:2.2.0-email
    deploy:
      resources:
        limits:
          memory: 100M
    restart: unless-stopped
    ports:
      - "6060"
    environment:
      APP_ENV: production
      EMAIL_PORT: "6060"
      FLAGD_HOST: flagd
      FLAGD_PORT: "8013"
      OTEL_EXPORTER_OTLP_ENDPOINT: http://otel-collector:4318
      OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE: cumulative
      OTEL_RESOURCE_ATTRIBUTES: "service.namespace=opentelemetry-demo,service.version=2.2.0"
      OTEL_SERVICE_NAME: email
    depends_on:
      otel-collector:
        condition: service_started
    logging: *logging

  fraud-detection:
    image: ghcr.io/open-telemetry/demo:2.2.0-fraud-detection
    deploy:
      resources:
        limits:
          memory: 300M
    restart: unless-stopped
    environment:
      FLAGD_HOST: flagd
      FLAGD_PORT: "8013"
      KAFKA_ADDR: kafka:9092
      OTEL_EXPORTER_OTLP_ENDPOINT: http://otel-collector:4318
      OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE: cumulative
      OTEL_INSTRUMENTATION_KAFKA_EXPERIMENTAL_SPAN_ATTRIBUTES: "true"
      OTEL_INSTRUMENTATION_MESSAGING_EXPERIMENTAL_RECEIVE_TELEMETRY_ENABLED: "true"
      OTEL_RESOURCE_ATTRIBUTES: "service.namespace=opentelemetry-demo,service.version=2.2.0"
      OTEL_SERVICE_NAME: fraud-detection
    depends_on:
      otel-collector:
        condition: service_started
      kafka:
        condition: service_healthy
    logging: *logging

  frontend:
    image: ghcr.io/open-telemetry/demo:2.2.0-frontend
    deploy:
      resources:
        limits:
          memory: 250M
    restart: unless-stopped
    ports:
      - "8080"
    environment:
      <<: *otel-env
      PORT: "8080"
      FRONTEND_ADDR: frontend:8080
      AD_ADDR: ad:9555
      CART_ADDR: cart:7070
      CHECKOUT_ADDR: checkout:5050
      CURRENCY_ADDR: currency:7001
      PRODUCT_CATALOG_ADDR: product-catalog:3550
      PRODUCT_REVIEWS_ADDR: product-reviews:3551
      RECOMMENDATION_ADDR: recommendation:9001
      SHIPPING_ADDR: http://shipping:50050
      ENV_PLATFORM: local
      OTEL_SERVICE_NAME: frontend
      PUBLIC_OTEL_EXPORTER_OTLP_TRACES_ENDPOINT: http://localhost:8080/otlp-http/v1/traces
      WEB_OTEL_SERVICE_NAME: frontend-web
      OTEL_COLLECTOR_HOST: otel-collector
      FLAGD_HOST: flagd
      FLAGD_PORT: "8013"
    depends_on:
      ad:
        condition: service_started
      cart:
        condition: service_started
      checkout:
        condition: service_started
      currency:
        condition: service_started
      product-catalog:
        condition: service_started
      quote:
        condition: service_started
      recommendation:
        condition: service_started
      shipping:
        condition: service_started
      otel-collector:
        condition: service_started
      image-provider:
        condition: service_started
      flagd:
        condition: service_started
    logging: *logging

  frontend-proxy:
    image: ghcr.io/open-telemetry/demo:2.2.0-frontend-proxy
    deploy:
      resources:
        limits:
          memory: 65M
    restart: unless-stopped
    ports:
      - "8080:8080"
      - "10000:10000"
    environment:
      FRONTEND_PORT: "8080"
      FRONTEND_HOST: frontend
      LOCUST_WEB_HOST: load-generator
      LOCUST_WEB_PORT: "8089"
      GRAFANA_PORT: "3000"
      GRAFANA_HOST: "localhost"
      JAEGER_UI_PORT: "16686"
      JAEGER_HOST: "localhost"
      OTEL_COLLECTOR_HOST: otel-collector
      IMAGE_PROVIDER_HOST: image-provider
      IMAGE_PROVIDER_PORT: "8081"
      OTEL_COLLECTOR_PORT_GRPC: "4317"
      OTEL_COLLECTOR_PORT_HTTP: "4318"
      OTEL_RESOURCE_ATTRIBUTES: "service.namespace=opentelemetry-demo,service.version=2.2.0"
      OTEL_SERVICE_NAME: frontend-proxy
      ENVOY_PORT: "8080"
      ENVOY_ADDR: "0.0.0.0"
      ENVOY_ADMIN_PORT: "10000"
      FLAGD_HOST: flagd
      FLAGD_PORT: "8013"
      FLAGD_UI_HOST: flagd-ui
      FLAGD_UI_PORT: "4000"
    depends_on:
      frontend:
        condition: service_started
      load-generator:
        condition: service_started
      flagd-ui:
        condition: service_started
    dns_search: ""

  image-provider:
    image: ghcr.io/open-telemetry/demo:2.2.0-image-provider
    deploy:
      resources:
        limits:
          memory: 120M
    restart: unless-stopped
    ports:
      - "8081"
    environment:
      IMAGE_PROVIDER_PORT: "8081"
      OTEL_COLLECTOR_HOST: otel-collector
      OTEL_COLLECTOR_PORT_GRPC: "4317"
      OTEL_RESOURCE_ATTRIBUTES: "service.namespace=opentelemetry-demo,service.version=2.2.0"
      OTEL_SERVICE_NAME: image-provider
    depends_on:
      otel-collector:
        condition: service_started
    logging: *logging

  load-generator:
    image: ghcr.io/open-telemetry/demo:2.2.0-load-generator
    deploy:
      resources:
        limits:
          memory: 1500M
    restart: unless-stopped
    ports:
      - "8089"
    environment:
      <<: *otel-env
      LOCUST_WEB_PORT: "8089"
      LOCUST_USERS: "5"
      LOCUST_HOST: http://frontend-proxy:8080
      LOCUST_HEADLESS: "false"
      LOCUST_AUTOSTART: "true"
      LOCUST_BROWSER_TRAFFIC_ENABLED: "true"
      OTEL_SERVICE_NAME: load-generator
      PROTOCOL_BUFFERS_PYTHON_IMPLEMENTATION: python
      LOCUST_WEB_HOST: "0.0.0.0"
      FLAGD_HOST: flagd
      FLAGD_PORT: "8013"
      FLAGD_OFREP_PORT: "8016"
    depends_on:
      frontend:
        condition: service_started
      flagd:
        condition: service_started
    logging: *logging

  payment:
    image: ghcr.io/open-telemetry/demo:2.2.0-payment
    deploy:
      resources:
        limits:
          memory: 140M
    restart: unless-stopped
    ports:
      - "50051"
    environment:
      <<: *otel-env
      IPV6_ENABLED: "false"
      PAYMENT_PORT: "50051"
      FLAGD_HOST: flagd
      FLAGD_PORT: "8013"
      OTEL_SERVICE_NAME: payment
    depends_on:
      otel-collector:
        condition: service_started
      flagd:
        condition: service_started
    logging: *logging

  product-catalog:
    image: ghcr.io/open-telemetry/demo:2.2.0-product-catalog
    deploy:
      resources:
        limits:
          memory: 20M
    restart: unless-stopped
    ports:
      - "3550"
    environment:
      <<: *otel-env
      PRODUCT_CATALOG_PORT: "3550"
      FLAGD_HOST: flagd
      FLAGD_PORT: "8013"
      GOMEMLIMIT: 16MiB
      OTEL_SERVICE_NAME: product-catalog
      OTEL_SEMCONV_STABILITY_OPT_IN: database
      DB_CONNECTION_STRING: "postgres://otelu:otelp@postgresql/otel?sslmode=disable"
    depends_on:
      otel-collector:
        condition: service_started
      flagd:
        condition: service_started
      postgresql:
        condition: service_started
    logging: *logging

  product-reviews:
    image: ghcr.io/open-telemetry/demo:2.2.0-product-reviews
    deploy:
      resources:
        limits:
          memory: 100M
    restart: unless-stopped
    ports:
      - "3551"
    environment:
      <<: *otel-env
      PRODUCT_REVIEWS_PORT: "3551"
      OTEL_PYTHON_LOG_CORRELATION: "true"
      OTEL_SERVICE_NAME: product-reviews
      OTEL_INSTRUMENTATION_GENAI_CAPTURE_MESSAGE_CONTENT: "true"
      PROTOCOL_BUFFERS_PYTHON_IMPLEMENTATION: python
      DB_CONNECTION_STRING: "host=postgresql user=otelu password=otelp dbname=otel"
      LLM_BASE_URL: http://llm:8000/v1
      OPENAI_API_KEY: dummy
      LLM_MODEL: astronomy-llm
      PRODUCT_CATALOG_ADDR: product-catalog:3550
      FLAGD_HOST: flagd
      FLAGD_PORT: "8013"
      LLM_HOST: llm
      LLM_PORT: "8000"
    depends_on:
      product-catalog:
        condition: service_started
      llm:
        condition: service_started
      postgresql:
        condition: service_started
      otel-collector:
        condition: service_started
    logging: *logging

  quote:
    image: ghcr.io/open-telemetry/demo:2.2.0-quote
    deploy:
      resources:
        limits:
          memory: 40M
    restart: unless-stopped
    ports:
      - "8090"
    environment:
      IPV6_ENABLED: "false"
      OTEL_EXPORTER_OTLP_ENDPOINT: http://otel-collector:4318
      OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE: cumulative
      OTEL_PHP_AUTOLOAD_ENABLED: "true"
      QUOTE_PORT: "8090"
      OTEL_PHP_INTERNAL_METRICS_ENABLED: "true"
      OTEL_RESOURCE_ATTRIBUTES: "service.namespace=opentelemetry-demo,service.version=2.2.0"
      OTEL_SERVICE_NAME: quote
    depends_on:
      otel-collector:
        condition: service_started
    logging: *logging

  recommendation:
    image: ghcr.io/open-telemetry/demo:2.2.0-recommendation
    deploy:
      resources:
        limits:
          memory: 500M
    restart: unless-stopped
    ports:
      - "9001"
    environment:
      <<: *otel-env
      RECOMMENDATION_PORT: "9001"
      PRODUCT_CATALOG_ADDR: product-catalog:3550
      FLAGD_HOST: flagd
      FLAGD_PORT: "8013"
      OTEL_PYTHON_LOG_CORRELATION: "true"
      OTEL_SERVICE_NAME: recommendation
      PROTOCOL_BUFFERS_PYTHON_IMPLEMENTATION: python
    depends_on:
      product-catalog:
        condition: service_started
      otel-collector:
        condition: service_started
      flagd:
        condition: service_started
    logging: *logging

  shipping:
    image: ghcr.io/open-telemetry/demo:2.2.0-shipping
    deploy:
      resources:
        limits:
          memory: 20M
    restart: unless-stopped
    ports:
      - "50050"
    environment:
      <<: *otel-env
      IPV6_ENABLED: "false"
      SHIPPING_PORT: "50050"
      QUOTE_ADDR: http://quote:8090
      OTEL_SERVICE_NAME: shipping
    depends_on:
      otel-collector:
        condition: service_started
    logging: *logging

  flagd:
    image: ghcr.io/open-feature/flagd:v0.12.9
    deploy:
      resources:
        limits:
          memory: 75M
    restart: unless-stopped
    environment:
      FLAGD_OTEL_COLLECTOR_URI: otel-collector:4317
      FLAGD_METRICS_EXPORTER: otel
      GOMEMLIMIT: 60MiB
      OTEL_RESOURCE_ATTRIBUTES: "service.namespace=opentelemetry-demo,service.version=2.2.0"
      OTEL_SERVICE_NAME: flagd
    command:
      - start
      - --uri
      - file:./etc/flagd/demo.flagd.json
    ports:
      - "8013"
      - "8016"
    volumes:
      - ./src/flagd:/etc/flagd
    logging: *logging

  flagd-ui:
    image: ghcr.io/open-telemetry/demo:2.2.0-flagd-ui
    deploy:
      resources:
        limits:
          memory: 200M
    restart: always
    environment:
      FLAGD_UI_PORT: "4000"
      OTEL_EXPORTER_OTLP_ENDPOINT: http://otel-collector:4318
      OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE: cumulative
      OTEL_RESOURCE_ATTRIBUTES: "service.namespace=opentelemetry-demo,service.version=2.2.0"
      OTEL_SERVICE_NAME: flagd-ui
      SECRET_KEY_BASE: yYrECL4qbNwleYInGJYvVnSkwJuSQJ4ijPTx5tirGUXrbznFIBFVJdPl5t6O9ASw
      PHX_HOST: localhost
    ports:
      - "4000"
    depends_on:
      otel-collector:
        condition: service_started
      flagd:
        condition: service_started
    volumes:
      - ./src/flagd:/app/data

  kafka:
    image: ghcr.io/open-telemetry/demo:2.2.0-kafka
    deploy:
      resources:
        limits:
          memory: 620M
    restart: unless-stopped
    environment:
      KAFKA_ADVERTISED_LISTENERS: PLAINTEXT://kafka:9092
      KAFKA_LISTENERS: PLAINTEXT://kafka:9092,CONTROLLER://kafka:9093
      KAFKA_CONTROLLER_QUORUM_VOTERS: 1@kafka:9093
      OTEL_EXPORTER_OTLP_ENDPOINT: http://otel-collector:4318
      OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE: cumulative
      OTEL_RESOURCE_ATTRIBUTES: "service.namespace=opentelemetry-demo,service.version=2.2.0"
      OTEL_SERVICE_NAME: kafka
      KAFKA_HEAP_OPTS: "-Xmx400m -Xms400m"
    healthcheck:
      test: nc -z kafka 9092
      start_period: 10s
      interval: 5s
      timeout: 10s
      retries: 10
    logging: *logging

  llm:
    image: ghcr.io/open-telemetry/demo:2.2.0-llm
    deploy:
      resources:
        limits:
          memory: 50M
    restart: unless-stopped
    environment:
      FLAGD_HOST: flagd
      FLAGD_PORT: "8013"
    ports:
      - "8000"
    depends_on:
      flagd:
        condition: service_started
    logging: *logging

  postgresql:
    image: postgres:17.6
    deploy:
      resources:
        limits:
          memory: 80M
    restart: unless-stopped
    ports:
      - "5432"
    environment:
      POSTGRES_USER: root
      POSTGRES_PASSWORD: otel
      POSTGRES_DB: otel
    volumes:
      - ./src/postgresql/init.sql:/docker-entrypoint-initdb.d/init.sql
      - otel_demo_postgresql_data:/var/lib/postgresql/data
    logging: *logging

  valkey-cart:
    image: valkey/valkey:9.0.1-alpine3.23
    user: valkey
    deploy:
      resources:
        limits:
          memory: 20M
    restart: unless-stopped
    ports:
      - "6379"
    logging: *logging

  otel-collector:
    image: ghcr.io/open-telemetry/opentelemetry-collector-releases/opentelemetry-collector-contrib:0.142.0
    deploy:
      resources:
        limits:
          memory: 200M
    restart: unless-stopped
    command: ["--config=/etc/otelcol-config.yml"]
    volumes:
      - ./otelcol-config.yml:/etc/otelcol-config.yml
    ports:
      - "4317"
      - "4318"
    depends_on:
      ingest-gateway:
        condition: service_healthy
    environment:
      GOMEMLIMIT: 160MiB
    logging: *logging

volumes:
  otel_demo_postgresql_data:
```

- [ ] **Step 2: Commit**

```bash
git add demos/otel-demo/docker-compose.yml
git commit -m "feat(otel-demo): add vendored docker-compose (v2.2.0, no observability backends)"
```

---

### Task 4: Update root docker-compose and delete testbench

**Files:**
- Modify: `docker-compose.yml`
- Delete: `testbench/` directory

**Interfaces:**
- Consumes: `demos/otel-demo/docker-compose.yml` from Task 3 via `include:`
- `ingest-gateway` service (already present) is used by `otel-collector` as a `depends_on` target — no changes needed to `ingest-gateway`

- [ ] **Step 1: Add `include:` to the root docker-compose.yml**

Add the following block at the very top of `docker-compose.yml`, before the `x-platform-self-observability-env` anchor:

```yaml
include:
  - demos/otel-demo/docker-compose.yml
```

- [ ] **Step 2: Remove the five shop services from `docker-compose.yml`**

Delete the entire `# --- TESTBENCH ---` section (lines from `# --- TESTBENCH ---` through the end of `shop-loadgen`). The services to remove are:
- `shop-db` (lines ~371–386)
- `shop-queue` (lines ~388–397)
- `shop-api` (lines ~399–416)
- `shop-worker` (lines ~418–428)
- `shop-loadgen` (lines ~430–439)

- [ ] **Step 3: Remove the `shop_db_data` volume from the `volumes:` block at the bottom of `docker-compose.yml`**

The `volumes:` section currently ends with:
```yaml
volumes:
  clickhouse_data:
  redpanda_data:
  postgres_data:
  shop_db_data:
  zitadel-bootstrap:
```

Remove the `shop_db_data:` line so it becomes:
```yaml
volumes:
  clickhouse_data:
  redpanda_data:
  postgres_data:
  zitadel-bootstrap:
```

- [ ] **Step 4: Delete the testbench directory**

```bash
rm -rf testbench/
```

- [ ] **Step 5: Verify the root compose parses cleanly**

```bash
docker compose config --quiet
# Expected: no output, exit code 0
```

If this fails with an error about services referencing missing networks or volumes, re-check the `include:` path and that the `shop_db_data` volume was fully removed.

- [ ] **Step 6: Commit**

```bash
git add docker-compose.yml
git rm -r testbench/
git commit -m "feat: integrate otel-demo via compose include, remove testbench shop"
```

---

## Verification

After all four tasks are committed, run:

```bash
docker compose pull
docker compose up -d
```

Check that:
1. `docker compose ps` shows no `shop-*` services
2. `docker compose ps` shows `otel-collector`, `frontend-proxy`, `kafka`, `flagd`, `postgresql`, and all 17 microservices starting
3. The astronomy shop is reachable at `http://localhost:8080`
4. Within ~60 seconds, traces appear in Observable's trace explorer (`http://localhost:5173`) with `service.namespace=opentelemetry-demo`
5. `docker compose logs otel-collector` shows no persistent export errors to `ingest-gateway:4317`
