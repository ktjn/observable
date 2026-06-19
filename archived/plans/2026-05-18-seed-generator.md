# Seed Generator Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a parameterized bulk historical telemetry seed generator that inserts realistic spans, logs, and metrics across many tenants and services directly into ClickHouse and Postgres, spread over configurable time windows.

**Architecture:** Topology-driven Python package at `scripts/seed/`. `world.py` builds a complete synthetic universe (tenants, service graphs, stories) from a profile JSON — all three signal generators then derive rows from that world so cross-signal coherence (correlated trace + log + metric for the same operation at the same time) falls out naturally. Rows are bulk-inserted via `clickhouse-connect`. Docker Compose `seed` profile service wires everything to the local stack.

**Tech Stack:** Python 3.12, `clickhouse-connect>=0.7.19`, `psycopg[binary]>=3.1.18`, `tqdm>=4.66`, `faker>=25`, `pytest>=8.2`

---

## File Map

| File | Responsibility |
|------|---------------|
| `scripts/seed.py` | Thin entry point (`python scripts/seed.py`) |
| `scripts/seed/__init__.py` | Empty package marker |
| `scripts/seed/__main__.py` | argparse CLI + time-walk orchestration loop |
| `scripts/seed/world.py` | All dataclasses, sampling helpers, profile loading, story application |
| `scripts/seed/pg_seeder.py` | INSERT tenants + api_keys into Postgres |
| `scripts/seed/inserter.py` | ClickHouse batch inserter with tqdm |
| `scripts/seed/traces.py` | Span + span_event row generation |
| `scripts/seed/logs.py` | Log record row generation |
| `scripts/seed/metrics.py` | metric_series + metric_point row generation |
| `scripts/seed/profiles/small.json` | 3 tenants, 5 services/tenant, 7 days |
| `scripts/seed/profiles/medium.json` | 15 tenants, 20 services/tenant, 30 days |
| `scripts/seed/profiles/large.json` | 50 tenants, 40 services/tenant, 90 days |
| `scripts/seed/tests/test_world.py` | World model + sampling + stories |
| `scripts/seed/tests/test_inserter.py` | Batch accumulation with mock client |
| `scripts/seed/tests/test_pg_seeder.py` | SQL correctness with mock connection |
| `scripts/seed/tests/test_traces.py` | Trace coherence, parent-child links |
| `scripts/seed/tests/test_logs.py` | Severity mapping, trace_id linkage |
| `scripts/seed/tests/test_metrics.py` | Series dedup, point structure |
| `scripts/seed/requirements.txt` | Python dependencies |
| `Dockerfile.seed` | Docker image for Compose service |
| `docker-compose.yml` | Add `seed` profile service |

---

### Task 1: Project scaffold

**Files:**
- Create: `scripts/seed/__init__.py`
- Create: `scripts/seed/tests/__init__.py`
- Create: `scripts/seed/requirements.txt`
- Create: `scripts/seed.py`
- Create: `Dockerfile.seed`

- [ ] **Step 1: Create directory structure**

```bash
mkdir -p scripts/seed/tests scripts/seed/profiles
touch scripts/seed/__init__.py scripts/seed/tests/__init__.py
```

- [ ] **Step 2: Write requirements.txt**

`scripts/seed/requirements.txt`:
```
clickhouse-connect>=0.7.19
psycopg[binary]>=3.1.18
tqdm>=4.66.4
faker>=25.0.0
pytest>=8.2.0
```

- [ ] **Step 3: Write entry point**

`scripts/seed.py`:
```python
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from seed.__main__ import main

if __name__ == "__main__":
    main()
```

- [ ] **Step 4: Write Dockerfile.seed**

`Dockerfile.seed`:
```dockerfile
FROM python:3.12-slim
WORKDIR /app
COPY scripts/seed/requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt
COPY scripts/seed/ ./seed/
CMD ["python", "-m", "seed"]
```

- [ ] **Step 5: Verify the package is importable**

```bash
cd scripts && pip install -r seed/requirements.txt -q && python -c "import seed; print('ok')"
```
Expected: `ok`

- [ ] **Step 6: Commit**

```bash
git add scripts/seed/ scripts/seed.py Dockerfile.seed
git commit -m "feat(seed): scaffold Python package and Dockerfile"
```

---

### Task 2: World model core dataclasses and sampling helpers

**Files:**
- Create: `scripts/seed/world.py`
- Create: `scripts/seed/tests/test_world.py`

- [ ] **Step 1: Write failing tests**

`scripts/seed/tests/test_world.py`:
```python
import math
import random
from datetime import datetime, timezone

import pytest

from seed.world import (
    Operation,
    Service,
    Story,
    Tenant,
    WorldModel,
    apply_stories,
    sample_duration_ms,
    traffic_multiplier,
)


def test_sample_duration_median_near_p50():
    rng = random.Random(42)
    samples = sorted(sample_duration_ms(100.0, 500.0, rng) for _ in range(10_000))
    assert 80 < samples[5000] < 120


def test_sample_duration_p99_near_target():
    rng = random.Random(42)
    samples = sorted(sample_duration_ms(100.0, 500.0, rng) for _ in range(10_000))
    assert 400 < samples[9900] < 650


def test_sample_duration_equal_p50_p99_returns_p50():
    rng = random.Random(0)
    assert sample_duration_ms(50.0, 50.0, rng) == 50.0


def test_traffic_multiplier_business_hours():
    dt = datetime(2024, 1, 15, 10, 0, tzinfo=timezone.utc)  # Monday 10:00
    assert traffic_multiplier(dt) == 1.0


def test_traffic_multiplier_evening():
    dt = datetime(2024, 1, 15, 20, 0, tzinfo=timezone.utc)  # Monday 20:00
    assert traffic_multiplier(dt) == 0.4


def test_traffic_multiplier_night():
    dt = datetime(2024, 1, 15, 3, 0, tzinfo=timezone.utc)  # Monday 03:00
    assert traffic_multiplier(dt) == 0.1


def test_traffic_multiplier_weekend():
    dt = datetime(2024, 1, 13, 10, 0, tzinfo=timezone.utc)  # Saturday 10:00
    assert traffic_multiplier(dt) == 0.2


def test_apply_stories_outside_window_no_effect():
    from datetime import timedelta
    base = datetime(2024, 1, 10, 12, 0, tzinfo=timezone.utc)
    story = Story(
        story_type="latency_spike",
        service_name="order-api",
        window_start=base + timedelta(hours=5),
        window_end=base + timedelta(hours=8),
        params={"latency_factor": 5.0},
    )
    tenant = Tenant("tid", "acme", "production", "key", [], [story])
    lat, err, ver = apply_stories(tenant, base, "order-api", 0.01)
    assert lat == 1.0
    assert err == 0.01
    assert ver is None


def test_apply_stories_latency_spike_inside_window():
    from datetime import timedelta
    base = datetime(2024, 1, 10, 12, 0, tzinfo=timezone.utc)
    story = Story(
        story_type="latency_spike",
        service_name="order-api",
        window_start=base,
        window_end=base + timedelta(hours=3),
        params={"latency_factor": 6.0},
    )
    tenant = Tenant("tid", "acme", "production", "key", [], [story])
    lat, err, ver = apply_stories(tenant, base + timedelta(hours=1), "order-api", 0.01)
    assert lat == 6.0


def test_apply_stories_error_burst_inside_window():
    from datetime import timedelta
    base = datetime(2024, 1, 10, 12, 0, tzinfo=timezone.utc)
    story = Story(
        story_type="error_burst",
        service_name="product-api",
        window_start=base,
        window_end=base + timedelta(hours=2),
        params={"error_rate_override": 0.3},
    )
    tenant = Tenant("tid", "acme", "production", "key", [], [story])
    _, err, _ = apply_stories(tenant, base + timedelta(minutes=30), "product-api", 0.01)
    assert err == pytest.approx(0.3)


def test_apply_stories_deployment_sets_version():
    from datetime import timedelta
    base = datetime(2024, 1, 10, 12, 0, tzinfo=timezone.utc)
    story = Story(
        story_type="deployment",
        service_name="user-api",
        window_start=base,
        window_end=base + timedelta(hours=1),
        params={"new_version": "2.0.0", "error_spike_factor": 3.0},
    )
    tenant = Tenant("tid", "acme", "production", "key", [], [story])
    _, _, ver = apply_stories(tenant, base + timedelta(minutes=5), "user-api", 0.01)
    assert ver == "2.0.0"


def test_apply_stories_wrong_service_no_effect():
    from datetime import timedelta
    base = datetime(2024, 1, 10, 12, 0, tzinfo=timezone.utc)
    story = Story("latency_spike", "order-api", base, base + timedelta(hours=1), {"latency_factor": 5.0})
    tenant = Tenant("tid", "acme", "production", "key", [], [story])
    lat, _, _ = apply_stories(tenant, base + timedelta(minutes=5), "product-api", 0.01)
    assert lat == 1.0
```

- [ ] **Step 2: Run to confirm failure**

```bash
cd scripts && python -m pytest seed/tests/test_world.py -v 2>&1 | head -20
```
Expected: `ModuleNotFoundError: No module named 'seed.world'`

- [ ] **Step 3: Write world.py with core types and helpers**

`scripts/seed/world.py`:
```python
from __future__ import annotations

import hashlib
import math
import random
from dataclasses import dataclass, field
from datetime import datetime, timezone


@dataclass
class Operation:
    name: str
    p50_ms: float
    p99_ms: float
    error_rate: float


@dataclass
class Service:
    service_name: str
    service_version: str
    service_type: str  # "api" | "worker" | "consumer" | "frontend"
    operations: list[Operation]
    downstream: list[str]  # service names this service calls
    rps: float  # baseline requests per second


@dataclass
class Story:
    story_type: str  # "deployment" | "latency_spike" | "error_burst"
    service_name: str
    window_start: datetime
    window_end: datetime
    params: dict = field(default_factory=dict)


@dataclass
class Tenant:
    tenant_id: str
    name: str
    environment: str
    api_key_plaintext: str
    services: list[Service]
    stories: list[Story] = field(default_factory=list)


@dataclass
class WorldModel:
    tenants: list[Tenant]
    start_time: datetime
    end_time: datetime
    seed: int


def sample_duration_ms(p50_ms: float, p99_ms: float, rng: random.Random) -> float:
    """Sample latency from log-normal distribution parameterized by p50 and p99."""
    if p99_ms <= p50_ms:
        return p50_ms
    mu = math.log(p50_ms)
    sigma = (math.log(p99_ms) - math.log(p50_ms)) / 2.326
    return math.exp(rng.gauss(mu, sigma))


def traffic_multiplier(dt: datetime) -> float:
    """RPS multiplier for the given datetime based on business hours and weekday."""
    hour = dt.hour
    if dt.weekday() >= 5:
        return 0.2
    if 9 <= hour < 18:
        return 1.0
    if 18 <= hour < 23:
        return 0.4
    return 0.1


def apply_stories(
    tenant: Tenant,
    dt: datetime,
    service_name: str,
    base_error_rate: float,
) -> tuple[float, float, str | None]:
    """Return (latency_multiplier, effective_error_rate, version_override) for dt."""
    latency_multiplier = 1.0
    effective_error_rate = base_error_rate
    version_override = None

    for story in tenant.stories:
        if story.service_name != service_name:
            continue
        if not (story.window_start <= dt < story.window_end):
            continue
        if story.story_type == "latency_spike":
            latency_multiplier = max(latency_multiplier, story.params.get("latency_factor", 3.0))
        elif story.story_type == "error_burst":
            effective_error_rate = max(effective_error_rate, story.params.get("error_rate_override", 0.2))
        elif story.story_type == "deployment":
            effective_error_rate = max(
                effective_error_rate,
                base_error_rate * story.params.get("error_spike_factor", 2.0),
            )
            version_override = story.params.get("new_version")

    return latency_multiplier, effective_error_rate, version_override


def hash_key(plaintext: str) -> str:
    return hashlib.sha256(plaintext.encode()).hexdigest()
```

- [ ] **Step 4: Run tests**

```bash
cd scripts && python -m pytest seed/tests/test_world.py -v
```
Expected: All 12 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add scripts/seed/world.py scripts/seed/tests/test_world.py
git commit -m "feat(seed): world model dataclasses, sampling helpers, story application"
```

---

### Task 3: Profile JSON files and world builder

**Files:**
- Modify: `scripts/seed/world.py` — add `load_profile()` and `build_world()`
- Create: `scripts/seed/profiles/small.json`
- Create: `scripts/seed/profiles/medium.json`
- Create: `scripts/seed/profiles/large.json`
- Modify: `scripts/seed/tests/test_world.py` — add profile tests

- [ ] **Step 1: Write failing tests**

Add to `scripts/seed/tests/test_world.py`:
```python
from seed.world import load_profile, build_world


def test_small_profile_tenant_count():
    world = load_profile("small", seed=42)
    assert len(world.tenants) == 3


def test_small_profile_days():
    world = load_profile("small", seed=42)
    assert (world.end_time - world.start_time).days == 7


def test_medium_profile_tenant_count():
    world = load_profile("medium", seed=42)
    assert len(world.tenants) == 15


def test_tenant_ids_are_unique():
    world = load_profile("medium", seed=42)
    ids = [t.tenant_id for t in world.tenants]
    assert len(ids) == len(set(ids))


def test_api_keys_are_unique():
    world = load_profile("small", seed=42)
    keys = [t.api_key_plaintext for t in world.tenants]
    assert len(keys) == len(set(keys))


def test_build_world_override_tenants():
    world = build_world(profile="small", tenants=7, services_per_tenant=3, days=14, seed=99)
    assert len(world.tenants) == 7
    assert (world.end_time - world.start_time).days == 14


def test_services_have_operations():
    world = load_profile("small", seed=42)
    for tenant in world.tenants:
        for svc in tenant.services:
            assert len(svc.operations) >= 1, f"{svc.service_name} has no operations"


def test_stories_windows_are_within_range():
    world = load_profile("small", seed=42)
    for tenant in world.tenants:
        for story in tenant.stories:
            assert story.window_start >= world.start_time
            assert story.window_end <= world.end_time
```

- [ ] **Step 2: Run to confirm failure**

```bash
cd scripts && python -m pytest seed/tests/test_world.py -k "profile or build_world" -v 2>&1 | head -10
```
Expected: `ImportError: cannot import name 'load_profile'`

- [ ] **Step 3: Write small.json**

`scripts/seed/profiles/small.json`:
```json
{
  "tenants": 3,
  "services_per_tenant": 5,
  "days": 7,
  "stories_per_tenant": 2,
  "service_catalog": [
    {
      "service_name": "order-api",
      "service_type": "api",
      "service_version": "1.2.0",
      "rps": 12.0,
      "operations": [
        {"name": "POST /orders", "p50_ms": 80, "p99_ms": 400, "error_rate": 0.02},
        {"name": "GET /orders/{id}", "p50_ms": 30, "p99_ms": 150, "error_rate": 0.01}
      ],
      "downstream": ["product-api", "user-api"]
    },
    {
      "service_name": "product-api",
      "service_type": "api",
      "service_version": "2.0.1",
      "rps": 20.0,
      "operations": [
        {"name": "GET /products", "p50_ms": 20, "p99_ms": 100, "error_rate": 0.005},
        {"name": "GET /products/{id}", "p50_ms": 15, "p99_ms": 80, "error_rate": 0.005}
      ],
      "downstream": []
    },
    {
      "service_name": "user-api",
      "service_type": "api",
      "service_version": "1.0.5",
      "rps": 8.0,
      "operations": [
        {"name": "GET /users/{id}", "p50_ms": 25, "p99_ms": 120, "error_rate": 0.01},
        {"name": "POST /users", "p50_ms": 60, "p99_ms": 300, "error_rate": 0.015}
      ],
      "downstream": []
    },
    {
      "service_name": "notification-worker",
      "service_type": "worker",
      "service_version": "1.1.0",
      "rps": 4.0,
      "operations": [
        {"name": "worker.send_email", "p50_ms": 200, "p99_ms": 1500, "error_rate": 0.03},
        {"name": "worker.send_sms", "p50_ms": 150, "p99_ms": 800, "error_rate": 0.02}
      ],
      "downstream": []
    },
    {
      "service_name": "web-frontend",
      "service_type": "frontend",
      "service_version": "3.1.0",
      "rps": 30.0,
      "operations": [
        {"name": "GET /", "p50_ms": 45, "p99_ms": 250, "error_rate": 0.008},
        {"name": "GET /checkout", "p50_ms": 90, "p99_ms": 500, "error_rate": 0.02}
      ],
      "downstream": ["order-api", "product-api"]
    }
  ]
}
```

- [ ] **Step 4: Write medium.json**

`scripts/seed/profiles/medium.json`:
```json
{
  "tenants": 15,
  "services_per_tenant": 20,
  "days": 30,
  "stories_per_tenant": 4,
  "service_catalog": [
    {"service_name": "order-api", "service_type": "api", "service_version": "1.2.0", "rps": 12.0, "operations": [{"name": "POST /orders", "p50_ms": 80, "p99_ms": 400, "error_rate": 0.02}, {"name": "GET /orders/{id}", "p50_ms": 30, "p99_ms": 150, "error_rate": 0.01}], "downstream": ["product-api", "user-api"]},
    {"service_name": "product-api", "service_type": "api", "service_version": "2.0.1", "rps": 20.0, "operations": [{"name": "GET /products", "p50_ms": 20, "p99_ms": 100, "error_rate": 0.005}, {"name": "GET /products/{id}", "p50_ms": 15, "p99_ms": 80, "error_rate": 0.005}, {"name": "POST /products", "p50_ms": 50, "p99_ms": 200, "error_rate": 0.01}], "downstream": []},
    {"service_name": "user-api", "service_type": "api", "service_version": "1.0.5", "rps": 8.0, "operations": [{"name": "GET /users/{id}", "p50_ms": 25, "p99_ms": 120, "error_rate": 0.01}, {"name": "POST /users", "p50_ms": 60, "p99_ms": 300, "error_rate": 0.015}], "downstream": []},
    {"service_name": "payment-api", "service_type": "api", "service_version": "1.5.2", "rps": 6.0, "operations": [{"name": "POST /payments", "p50_ms": 300, "p99_ms": 2000, "error_rate": 0.03}, {"name": "GET /payments/{id}", "p50_ms": 40, "p99_ms": 200, "error_rate": 0.01}], "downstream": []},
    {"service_name": "inventory-api", "service_type": "api", "service_version": "1.0.0", "rps": 15.0, "operations": [{"name": "GET /inventory", "p50_ms": 35, "p99_ms": 180, "error_rate": 0.008}, {"name": "PUT /inventory/{id}", "p50_ms": 60, "p99_ms": 300, "error_rate": 0.012}], "downstream": []},
    {"service_name": "search-api", "service_type": "api", "service_version": "2.1.0", "rps": 25.0, "operations": [{"name": "GET /search", "p50_ms": 50, "p99_ms": 300, "error_rate": 0.003}], "downstream": ["product-api"]},
    {"service_name": "cart-api", "service_type": "api", "service_version": "1.3.0", "rps": 18.0, "operations": [{"name": "GET /cart", "p50_ms": 20, "p99_ms": 100, "error_rate": 0.005}, {"name": "POST /cart/items", "p50_ms": 40, "p99_ms": 200, "error_rate": 0.01}], "downstream": ["product-api", "inventory-api"]},
    {"service_name": "notification-worker", "service_type": "worker", "service_version": "1.1.0", "rps": 4.0, "operations": [{"name": "worker.send_email", "p50_ms": 200, "p99_ms": 1500, "error_rate": 0.03}], "downstream": []},
    {"service_name": "order-worker", "service_type": "worker", "service_version": "1.0.2", "rps": 5.0, "operations": [{"name": "worker.process_order", "p50_ms": 500, "p99_ms": 3000, "error_rate": 0.025}], "downstream": ["inventory-api", "payment-api"]},
    {"service_name": "report-worker", "service_type": "worker", "service_version": "1.0.0", "rps": 0.5, "operations": [{"name": "worker.generate_report", "p50_ms": 2000, "p99_ms": 10000, "error_rate": 0.01}], "downstream": []},
    {"service_name": "audit-consumer", "service_type": "consumer", "service_version": "1.0.0", "rps": 8.0, "operations": [{"name": "consumer.audit_event", "p50_ms": 30, "p99_ms": 150, "error_rate": 0.005}], "downstream": []},
    {"service_name": "event-consumer", "service_type": "consumer", "service_version": "1.1.0", "rps": 10.0, "operations": [{"name": "consumer.process_event", "p50_ms": 50, "p99_ms": 250, "error_rate": 0.01}], "downstream": ["notification-worker"]},
    {"service_name": "web-frontend", "service_type": "frontend", "service_version": "3.1.0", "rps": 30.0, "operations": [{"name": "GET /", "p50_ms": 45, "p99_ms": 250, "error_rate": 0.008}, {"name": "GET /checkout", "p50_ms": 90, "p99_ms": 500, "error_rate": 0.02}], "downstream": ["order-api", "product-api", "cart-api"]},
    {"service_name": "mobile-bff", "service_type": "frontend", "service_version": "2.0.0", "rps": 20.0, "operations": [{"name": "GET /api/home", "p50_ms": 60, "p99_ms": 350, "error_rate": 0.01}, {"name": "POST /api/order", "p50_ms": 100, "p99_ms": 600, "error_rate": 0.02}], "downstream": ["order-api", "product-api"]},
    {"service_name": "analytics-api", "service_type": "api", "service_version": "1.0.0", "rps": 5.0, "operations": [{"name": "GET /analytics/dashboard", "p50_ms": 500, "p99_ms": 3000, "error_rate": 0.02}], "downstream": []},
    {"service_name": "recommendation-api", "service_type": "api", "service_version": "1.2.0", "rps": 15.0, "operations": [{"name": "GET /recommendations", "p50_ms": 80, "p99_ms": 400, "error_rate": 0.01}], "downstream": ["product-api"]},
    {"service_name": "pricing-api", "service_type": "api", "service_version": "1.0.3", "rps": 10.0, "operations": [{"name": "GET /prices", "p50_ms": 25, "p99_ms": 120, "error_rate": 0.005}], "downstream": []},
    {"service_name": "review-api", "service_type": "api", "service_version": "1.0.0", "rps": 7.0, "operations": [{"name": "GET /reviews/{product_id}", "p50_ms": 40, "p99_ms": 200, "error_rate": 0.007}, {"name": "POST /reviews", "p50_ms": 70, "p99_ms": 350, "error_rate": 0.015}], "downstream": []},
    {"service_name": "shipping-api", "service_type": "api", "service_version": "2.0.0", "rps": 6.0, "operations": [{"name": "GET /shipping/estimate", "p50_ms": 150, "p99_ms": 800, "error_rate": 0.02}], "downstream": []},
    {"service_name": "auth-service", "service_type": "api", "service_version": "1.4.0", "rps": 22.0, "operations": [{"name": "POST /auth/login", "p50_ms": 100, "p99_ms": 500, "error_rate": 0.05}, {"name": "POST /auth/refresh", "p50_ms": 30, "p99_ms": 150, "error_rate": 0.01}], "downstream": []}
  ]
}
```

- [ ] **Step 5: Write large.json**

`scripts/seed/profiles/large.json`:
```json
{
  "tenants": 50,
  "services_per_tenant": 40,
  "days": 90,
  "stories_per_tenant": 6,
  "service_catalog": [
    {"service_name": "order-api", "service_type": "api", "service_version": "1.2.0", "rps": 12.0, "operations": [{"name": "POST /orders", "p50_ms": 80, "p99_ms": 400, "error_rate": 0.02}, {"name": "GET /orders/{id}", "p50_ms": 30, "p99_ms": 150, "error_rate": 0.01}], "downstream": ["product-api", "user-api"]},
    {"service_name": "product-api", "service_type": "api", "service_version": "2.0.1", "rps": 20.0, "operations": [{"name": "GET /products", "p50_ms": 20, "p99_ms": 100, "error_rate": 0.005}, {"name": "GET /products/{id}", "p50_ms": 15, "p99_ms": 80, "error_rate": 0.005}], "downstream": []},
    {"service_name": "user-api", "service_type": "api", "service_version": "1.0.5", "rps": 8.0, "operations": [{"name": "GET /users/{id}", "p50_ms": 25, "p99_ms": 120, "error_rate": 0.01}], "downstream": []},
    {"service_name": "payment-api", "service_type": "api", "service_version": "1.5.2", "rps": 6.0, "operations": [{"name": "POST /payments", "p50_ms": 300, "p99_ms": 2000, "error_rate": 0.03}], "downstream": []},
    {"service_name": "inventory-api", "service_type": "api", "service_version": "1.0.0", "rps": 15.0, "operations": [{"name": "GET /inventory", "p50_ms": 35, "p99_ms": 180, "error_rate": 0.008}], "downstream": []},
    {"service_name": "search-api", "service_type": "api", "service_version": "2.1.0", "rps": 25.0, "operations": [{"name": "GET /search", "p50_ms": 50, "p99_ms": 300, "error_rate": 0.003}], "downstream": ["product-api"]},
    {"service_name": "cart-api", "service_type": "api", "service_version": "1.3.0", "rps": 18.0, "operations": [{"name": "POST /cart/items", "p50_ms": 40, "p99_ms": 200, "error_rate": 0.01}], "downstream": ["product-api"]},
    {"service_name": "notification-worker", "service_type": "worker", "service_version": "1.1.0", "rps": 4.0, "operations": [{"name": "worker.send_email", "p50_ms": 200, "p99_ms": 1500, "error_rate": 0.03}], "downstream": []},
    {"service_name": "order-worker", "service_type": "worker", "service_version": "1.0.2", "rps": 5.0, "operations": [{"name": "worker.process_order", "p50_ms": 500, "p99_ms": 3000, "error_rate": 0.025}], "downstream": []},
    {"service_name": "report-worker", "service_type": "worker", "service_version": "1.0.0", "rps": 0.5, "operations": [{"name": "worker.generate_report", "p50_ms": 2000, "p99_ms": 10000, "error_rate": 0.01}], "downstream": []},
    {"service_name": "audit-consumer", "service_type": "consumer", "service_version": "1.0.0", "rps": 8.0, "operations": [{"name": "consumer.audit_event", "p50_ms": 30, "p99_ms": 150, "error_rate": 0.005}], "downstream": []},
    {"service_name": "event-consumer", "service_type": "consumer", "service_version": "1.1.0", "rps": 10.0, "operations": [{"name": "consumer.process_event", "p50_ms": 50, "p99_ms": 250, "error_rate": 0.01}], "downstream": []},
    {"service_name": "web-frontend", "service_type": "frontend", "service_version": "3.1.0", "rps": 30.0, "operations": [{"name": "GET /", "p50_ms": 45, "p99_ms": 250, "error_rate": 0.008}], "downstream": ["order-api", "product-api"]},
    {"service_name": "mobile-bff", "service_type": "frontend", "service_version": "2.0.0", "rps": 20.0, "operations": [{"name": "GET /api/home", "p50_ms": 60, "p99_ms": 350, "error_rate": 0.01}], "downstream": ["order-api"]},
    {"service_name": "analytics-api", "service_type": "api", "service_version": "1.0.0", "rps": 5.0, "operations": [{"name": "GET /analytics/dashboard", "p50_ms": 500, "p99_ms": 3000, "error_rate": 0.02}], "downstream": []},
    {"service_name": "recommendation-api", "service_type": "api", "service_version": "1.2.0", "rps": 15.0, "operations": [{"name": "GET /recommendations", "p50_ms": 80, "p99_ms": 400, "error_rate": 0.01}], "downstream": ["product-api"]},
    {"service_name": "pricing-api", "service_type": "api", "service_version": "1.0.3", "rps": 10.0, "operations": [{"name": "GET /prices", "p50_ms": 25, "p99_ms": 120, "error_rate": 0.005}], "downstream": []},
    {"service_name": "review-api", "service_type": "api", "service_version": "1.0.0", "rps": 7.0, "operations": [{"name": "GET /reviews/{product_id}", "p50_ms": 40, "p99_ms": 200, "error_rate": 0.007}], "downstream": []},
    {"service_name": "shipping-api", "service_type": "api", "service_version": "2.0.0", "rps": 6.0, "operations": [{"name": "GET /shipping/estimate", "p50_ms": 150, "p99_ms": 800, "error_rate": 0.02}], "downstream": []},
    {"service_name": "auth-service", "service_type": "api", "service_version": "1.4.0", "rps": 22.0, "operations": [{"name": "POST /auth/login", "p50_ms": 100, "p99_ms": 500, "error_rate": 0.05}], "downstream": []},
    {"service_name": "catalog-api", "service_type": "api", "service_version": "1.0.0", "rps": 18.0, "operations": [{"name": "GET /catalog", "p50_ms": 30, "p99_ms": 150, "error_rate": 0.005}], "downstream": []},
    {"service_name": "discount-api", "service_type": "api", "service_version": "1.1.0", "rps": 9.0, "operations": [{"name": "GET /discounts", "p50_ms": 20, "p99_ms": 100, "error_rate": 0.003}], "downstream": []},
    {"service_name": "loyalty-api", "service_type": "api", "service_version": "1.0.0", "rps": 5.0, "operations": [{"name": "GET /loyalty/points", "p50_ms": 35, "p99_ms": 180, "error_rate": 0.005}], "downstream": []},
    {"service_name": "wishlist-api", "service_type": "api", "service_version": "1.0.0", "rps": 7.0, "operations": [{"name": "GET /wishlist", "p50_ms": 20, "p99_ms": 100, "error_rate": 0.004}], "downstream": []},
    {"service_name": "subscription-api", "service_type": "api", "service_version": "1.2.0", "rps": 4.0, "operations": [{"name": "POST /subscriptions", "p50_ms": 80, "p99_ms": 400, "error_rate": 0.015}], "downstream": ["payment-api"]},
    {"service_name": "webhook-service", "service_type": "api", "service_version": "1.0.0", "rps": 6.0, "operations": [{"name": "POST /webhooks/deliver", "p50_ms": 150, "p99_ms": 1000, "error_rate": 0.04}], "downstream": []},
    {"service_name": "export-worker", "service_type": "worker", "service_version": "1.0.0", "rps": 0.3, "operations": [{"name": "worker.export_data", "p50_ms": 5000, "p99_ms": 30000, "error_rate": 0.02}], "downstream": []},
    {"service_name": "sync-worker", "service_type": "worker", "service_version": "1.1.0", "rps": 2.0, "operations": [{"name": "worker.sync_external", "p50_ms": 800, "p99_ms": 5000, "error_rate": 0.03}], "downstream": []},
    {"service_name": "cleanup-worker", "service_type": "worker", "service_version": "1.0.0", "rps": 0.1, "operations": [{"name": "worker.cleanup", "p50_ms": 1000, "p99_ms": 8000, "error_rate": 0.01}], "downstream": []},
    {"service_name": "billing-worker", "service_type": "worker", "service_version": "1.0.0", "rps": 1.0, "operations": [{"name": "worker.process_billing", "p50_ms": 300, "p99_ms": 2000, "error_rate": 0.015}], "downstream": ["payment-api"]},
    {"service_name": "payment-consumer", "service_type": "consumer", "service_version": "1.0.0", "rps": 6.0, "operations": [{"name": "consumer.payment_event", "p50_ms": 40, "p99_ms": 200, "error_rate": 0.008}], "downstream": []},
    {"service_name": "inventory-consumer", "service_type": "consumer", "service_version": "1.0.0", "rps": 8.0, "operations": [{"name": "consumer.inventory_update", "p50_ms": 25, "p99_ms": 120, "error_rate": 0.005}], "downstream": []},
    {"service_name": "fraud-detection-api", "service_type": "api", "service_version": "1.0.0", "rps": 6.0, "operations": [{"name": "POST /fraud/check", "p50_ms": 200, "p99_ms": 1200, "error_rate": 0.01}], "downstream": []},
    {"service_name": "compliance-api", "service_type": "api", "service_version": "1.0.0", "rps": 2.0, "operations": [{"name": "GET /compliance/report", "p50_ms": 800, "p99_ms": 5000, "error_rate": 0.008}], "downstream": []},
    {"service_name": "tax-api", "service_type": "api", "service_version": "1.1.0", "rps": 5.0, "operations": [{"name": "POST /tax/calculate", "p50_ms": 60, "p99_ms": 300, "error_rate": 0.005}], "downstream": []},
    {"service_name": "geolocation-api", "service_type": "api", "service_version": "1.0.0", "rps": 12.0, "operations": [{"name": "GET /geo/location", "p50_ms": 30, "p99_ms": 150, "error_rate": 0.003}], "downstream": []},
    {"service_name": "media-api", "service_type": "api", "service_version": "1.0.0", "rps": 10.0, "operations": [{"name": "POST /media/upload", "p50_ms": 400, "p99_ms": 2500, "error_rate": 0.02}], "downstream": []},
    {"service_name": "config-api", "service_type": "api", "service_version": "1.0.0", "rps": 3.0, "operations": [{"name": "GET /config", "p50_ms": 10, "p99_ms": 50, "error_rate": 0.001}], "downstream": []},
    {"service_name": "feature-flags-api", "service_type": "api", "service_version": "1.0.0", "rps": 15.0, "operations": [{"name": "GET /flags", "p50_ms": 8, "p99_ms": 40, "error_rate": 0.001}], "downstream": []},
    {"service_name": "api-gateway", "service_type": "api", "service_version": "1.0.0", "rps": 40.0, "operations": [{"name": "ANY /*", "p50_ms": 5, "p99_ms": 25, "error_rate": 0.002}], "downstream": ["auth-service", "order-api", "product-api"]}
  ]
}
```

- [ ] **Step 6: Add `load_profile()` and `build_world()` to world.py**

Add these imports and functions to `scripts/seed/world.py` (append after `hash_key`):

```python
import json
import uuid as _uuid
from datetime import timedelta
from pathlib import Path

_PROFILES_DIR = Path(__file__).parent / "profiles"

_TENANT_NAMES = [
    "acme-corp", "globex-inc", "initech", "umbrella-corp", "wayne-enterprises",
    "stark-industries", "oscorp", "cyberdyne", "weyland-yutani", "tyrell-corp",
    "soylent-corp", "omni-consumer", "planitir", "veridian-dynamics",
    "massive-dynamics", "blue-sun-corp", "buy-n-large", "rekall-inc",
    "luthorcorp", "axiom-air", "virtucon", "nakatomi-corp", "yoyodyne",
    "dharma-initiative", "los-pollos", "sabre-corp", "bluth-company",
    "wernham-hogg", "dundler-mifflin", "pied-piper", "hooli", "bachmanity",
    "piedmont-global", "hemisphere-media", "apex-unlimited", "krusty-krab",
    "facespace", "chatterbox", "knowitall", "springfield-nuclear",
    "mogul-corp", "belcher-burgers", "cryptex-security", "atlas-telemetry",
    "nexus-analytics", "photon-systems", "meridian-tech", "polaris-cloud",
    "aurora-infra", "quantum-labs",
]


def load_profile(name: str, seed: int = 42, **overrides: int) -> WorldModel:
    """Load a named profile (small/medium/large) and build a world model."""
    config = json.loads((_PROFILES_DIR / f"{name}.json").read_text())
    kwargs = {
        "tenants": config["tenants"],
        "services_per_tenant": config["services_per_tenant"],
        "days": config["days"],
    }
    kwargs.update({k: v for k, v in overrides.items() if v is not None})
    return build_world(profile=name, seed=seed, **kwargs)


def build_world(
    profile: str = "small",
    tenants: int = 3,
    services_per_tenant: int = 5,
    days: int = 7,
    seed: int = 42,
) -> WorldModel:
    """Build a deterministic WorldModel from parameters."""
    rng = random.Random(seed)
    config = json.loads((_PROFILES_DIR / f"{profile}.json").read_text())
    catalog = config["service_catalog"]
    stories_per_tenant = config.get("stories_per_tenant", 2)
    service_name_set = {s["service_name"] for s in catalog}

    end_time = datetime.now(timezone.utc).replace(minute=0, second=0, microsecond=0)
    start_time = end_time - timedelta(days=days)

    built: list[Tenant] = []
    for i in range(tenants):
        # Deterministic UUID from seed + index
        tenant_id = str(_uuid.UUID(int=(seed * 100_000 + i) % (2**128), version=4))
        name = _TENANT_NAMES[i % len(_TENANT_NAMES)]
        environment = "staging" if i % 4 == 0 else "production"
        api_key = f"seed-key-{name}-{seed:04d}-{i:04d}"

        svc_cfgs = [catalog[j % len(catalog)] for j in range(services_per_tenant)]
        svc_name_subset = {c["service_name"] for c in svc_cfgs}
        services = [
            Service(
                service_name=c["service_name"],
                service_version=c["service_version"],
                service_type=c["service_type"],
                operations=[Operation(**op) for op in c["operations"]],
                downstream=[d for d in c["downstream"] if d in svc_name_subset],
                rps=c["rps"],
            )
            for c in svc_cfgs
        ]

        stories = _make_stories(services, start_time, end_time, stories_per_tenant, rng)
        built.append(Tenant(tenant_id, name, environment, api_key, services, stories))

    return WorldModel(built, start_time, end_time, seed)


def _make_stories(
    services: list[Service],
    start_time: datetime,
    end_time: datetime,
    count: int,
    rng: random.Random,
) -> list[Story]:
    total_hours = (end_time - start_time).total_seconds() / 3600
    story_types = ["deployment", "latency_spike", "error_burst"]
    stories = []
    for _ in range(count):
        svc = rng.choice(services)
        stype = rng.choice(story_types)
        offset_h = rng.uniform(1.0, total_hours - 7.0)
        duration_h = rng.uniform(1.0, 6.0)
        window_start = start_time + timedelta(hours=offset_h)
        window_end = window_start + timedelta(hours=duration_h)

        if stype == "deployment":
            parts = svc.service_version.split(".")
            new_minor = int(parts[1]) + 1 if len(parts) > 1 else 1
            params = {"new_version": f"{parts[0]}.{new_minor}.0", "error_spike_factor": 2.0}
        elif stype == "latency_spike":
            params = {"latency_factor": rng.uniform(3.0, 8.0)}
        else:
            params = {"error_rate_override": rng.uniform(0.1, 0.4)}

        stories.append(Story(stype, svc.service_name, window_start, window_end, params))
    return stories
```

- [ ] **Step 7: Run all world tests**

```bash
cd scripts && python -m pytest seed/tests/test_world.py -v
```
Expected: All tests PASS (including the new profile tests).

- [ ] **Step 8: Commit**

```bash
git add scripts/seed/world.py scripts/seed/profiles/ scripts/seed/tests/test_world.py
git commit -m "feat(seed): profile loading, world builder, story generation"
```

---

### Task 4: ClickHouse inserter

**Files:**
- Create: `scripts/seed/inserter.py`
- Create: `scripts/seed/tests/test_inserter.py`

- [ ] **Step 1: Write failing tests**

`scripts/seed/tests/test_inserter.py`:
```python
from unittest.mock import MagicMock, patch, call
from seed.inserter import Inserter

COLS = ["a", "b"]


def _make_inserter(batch_size=3):
    with patch("seed.inserter.clickhouse_connect.get_client", return_value=MagicMock()) as mock_get:
        ins = Inserter("localhost", 8123, "default", "pw", batch_size=batch_size)
        ins._client = MagicMock()
        return ins


def test_flush_calls_client_insert():
    ins = _make_inserter()
    rows = [[1, "x"], [2, "y"]]
    ins.flush("observable.spans", COLS, rows)
    ins._client.insert.assert_called_once_with("observable.spans", rows, column_names=COLS)


def test_flush_empty_rows_is_noop():
    ins = _make_inserter()
    ins.flush("observable.spans", COLS, [])
    ins._client.insert.assert_not_called()


def test_insert_in_batches_splits_correctly():
    ins = _make_inserter(batch_size=2)
    rows = [[i, str(i)] for i in range(5)]
    total = ins.insert_in_batches("observable.spans", COLS, rows, desc="test")
    assert total == 5
    assert ins._client.insert.call_count == 3  # batches of 2, 2, 1


def test_insert_in_batches_returns_zero_for_empty():
    ins = _make_inserter()
    total = ins.insert_in_batches("observable.spans", COLS, [], desc="empty")
    assert total == 0
    ins._client.insert.assert_not_called()
```

- [ ] **Step 2: Run to confirm failure**

```bash
cd scripts && python -m pytest seed/tests/test_inserter.py -v 2>&1 | head -10
```
Expected: `ModuleNotFoundError: No module named 'seed.inserter'`

- [ ] **Step 3: Write inserter.py**

`scripts/seed/inserter.py`:
```python
from __future__ import annotations

import clickhouse_connect
from tqdm import tqdm


class Inserter:
    def __init__(self, host: str, port: int, user: str, password: str, batch_size: int = 10_000):
        self._client = clickhouse_connect.get_client(
            host=host,
            port=port,
            username=user,
            password=password,
            database="observable",
        )
        self._batch_size = batch_size

    def flush(self, table: str, column_names: list[str], rows: list[list]) -> int:
        """Insert rows into table. Returns number of rows inserted."""
        if not rows:
            return 0
        self._client.insert(table, rows, column_names=column_names)
        return len(rows)

    def insert_in_batches(
        self,
        table: str,
        column_names: list[str],
        rows: list[list],
        desc: str = "",
    ) -> int:
        """Insert all rows in batches, showing tqdm progress. Returns total inserted."""
        if not rows:
            return 0
        total = 0
        with tqdm(total=len(rows), desc=desc or table, unit="rows", leave=False) as pbar:
            for i in range(0, len(rows), self._batch_size):
                batch = rows[i : i + self._batch_size]
                self.flush(table, column_names, batch)
                total += len(batch)
                pbar.update(len(batch))
        return total

    def row_count(self, table: str, tenant_id: str) -> int:
        """Return row count for a tenant in the given table (used for --resume)."""
        result = self._client.query(
            f"SELECT count() FROM {table} WHERE tenant_id = %(tid)s",
            parameters={"tid": tenant_id},
        )
        return result.first_row[0]

    def close(self) -> None:
        self._client.close()
```

- [ ] **Step 4: Run tests**

```bash
cd scripts && python -m pytest seed/tests/test_inserter.py -v
```
Expected: All 4 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add scripts/seed/inserter.py scripts/seed/tests/test_inserter.py
git commit -m "feat(seed): ClickHouse batch inserter with tqdm progress"
```

---

### Task 5: Postgres seeder

**Files:**
- Create: `scripts/seed/pg_seeder.py`
- Create: `scripts/seed/tests/test_pg_seeder.py`

- [ ] **Step 1: Write failing tests**

`scripts/seed/tests/test_pg_seeder.py`:
```python
from unittest.mock import MagicMock, patch, call
from seed.world import Tenant, Service, Operation
from seed.pg_seeder import seed_postgres, _hash_key
import hashlib


def _make_tenant(idx: int) -> Tenant:
    op = Operation("GET /health", 5.0, 50.0, 0.001)
    svc = Service("health-api", "1.0.0", "api", [op], [], 1.0)
    return Tenant(f"tid-{idx}", f"tenant-{idx}", "production", f"plaintext-key-{idx}", [svc])


def test_hash_key_is_sha256():
    expected = hashlib.sha256(b"hello").hexdigest()
    assert _hash_key("hello") == expected


def test_seed_postgres_dry_run_does_not_connect():
    tenants = [_make_tenant(0)]
    with patch("seed.pg_seeder.psycopg.connect") as mock_connect:
        seed_postgres(tenants, "postgresql://x/y", dry_run=True)
        mock_connect.assert_not_called()


def test_seed_postgres_inserts_tenant_and_api_key():
    tenants = [_make_tenant(0)]
    mock_conn = MagicMock()
    mock_cur = MagicMock()
    mock_conn.__enter__ = MagicMock(return_value=mock_conn)
    mock_conn.__exit__ = MagicMock(return_value=False)
    mock_conn.cursor.return_value.__enter__ = MagicMock(return_value=mock_cur)
    mock_conn.cursor.return_value.__exit__ = MagicMock(return_value=False)

    with patch("seed.pg_seeder.psycopg.connect", return_value=mock_conn):
        seed_postgres(tenants, "postgresql://x/y")

    calls = mock_cur.execute.call_args_list
    # First call inserts tenant
    assert "INSERT INTO tenants" in calls[0][0][0]
    assert "tid-0" in calls[0][0][1]
    # Second call inserts api_key
    assert "INSERT INTO api_keys" in calls[1][0][0]
    assert _hash_key("plaintext-key-0") in calls[1][0][1]


def test_seed_postgres_uses_on_conflict_do_nothing():
    tenants = [_make_tenant(0)]
    mock_conn = MagicMock()
    mock_cur = MagicMock()
    mock_conn.__enter__ = MagicMock(return_value=mock_conn)
    mock_conn.__exit__ = MagicMock(return_value=False)
    mock_conn.cursor.return_value.__enter__ = MagicMock(return_value=mock_cur)
    mock_conn.cursor.return_value.__exit__ = MagicMock(return_value=False)

    with patch("seed.pg_seeder.psycopg.connect", return_value=mock_conn):
        seed_postgres(tenants, "postgresql://x/y")

    for c in mock_cur.execute.call_args_list:
        assert "ON CONFLICT" in c[0][0]
```

- [ ] **Step 2: Run to confirm failure**

```bash
cd scripts && python -m pytest seed/tests/test_pg_seeder.py -v 2>&1 | head -10
```
Expected: `ModuleNotFoundError: No module named 'seed.pg_seeder'`

- [ ] **Step 3: Write pg_seeder.py**

`scripts/seed/pg_seeder.py`:
```python
from __future__ import annotations

import psycopg

from seed.world import Tenant, hash_key


def _hash_key(plaintext: str) -> str:
    return hash_key(plaintext)


def seed_postgres(tenants: list[Tenant], pg_url: str, dry_run: bool = False) -> None:
    """Insert tenants and api_keys into Postgres. Idempotent via ON CONFLICT DO NOTHING."""
    if dry_run:
        print(f"[dry-run] Would insert {len(tenants)} tenants + {len(tenants)} api_keys")
        return

    with psycopg.connect(pg_url) as conn:
        with conn.cursor() as cur:
            for tenant in tenants:
                cur.execute(
                    "INSERT INTO tenants (id, name) VALUES (%s, %s) ON CONFLICT (id) DO NOTHING",
                    (tenant.tenant_id, tenant.name),
                )
                cur.execute(
                    """
                    INSERT INTO api_keys (tenant_id, key_hash, name, role, environment)
                    VALUES (%s, %s, %s, %s, %s)
                    ON CONFLICT (key_hash) DO NOTHING
                    """,
                    (
                        tenant.tenant_id,
                        hash_key(tenant.api_key_plaintext),
                        f"seed-key-{tenant.name}",
                        "member",
                        tenant.environment,
                    ),
                )
        conn.commit()
```

- [ ] **Step 4: Run tests**

```bash
cd scripts && python -m pytest seed/tests/test_pg_seeder.py -v
```
Expected: All 4 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add scripts/seed/pg_seeder.py scripts/seed/tests/test_pg_seeder.py
git commit -m "feat(seed): Postgres seeder for tenants and api_keys"
```

---

### Task 6: Trace generator

**Files:**
- Create: `scripts/seed/traces.py`
- Create: `scripts/seed/tests/test_traces.py`

The `generate_trace` function returns a 6-tuple:
`(span_rows: list[list], event_rows: list[list], trace_id: str, root_span_id: str, is_error: bool, root_duration_ms: float)`

Column order matches the ClickHouse schema exactly (see `migrations/clickhouse/001_create_spans.sql` and `004_create_span_events.sql`).

- [ ] **Step 1: Write failing tests**

`scripts/seed/tests/test_traces.py`:
```python
import json
import random
from datetime import datetime, timezone

from seed.world import Operation, Service, Story, Tenant
from seed.traces import SPAN_COLS, EVENT_COLS, generate_trace


def _tenant() -> Tenant:
    op = Operation("POST /orders", 80.0, 400.0, 0.0)  # error_rate=0 for predictable tests
    downstream_op = Operation("GET /products", 20.0, 100.0, 0.0)
    root_svc = Service("order-api", "1.0.0", "api", [op], ["product-api"], 10.0)
    down_svc = Service("product-api", "2.0.0", "api", [downstream_op], [], 20.0)
    return Tenant("tid-001", "acme", "production", "key", [root_svc, down_svc])


def test_span_row_length_matches_cols():
    tenant = _tenant()
    svc = tenant.services[0]
    op = svc.operations[0]
    dt = datetime(2024, 1, 10, 12, 0, tzinfo=timezone.utc)
    rng = random.Random(42)
    span_rows, event_rows, trace_id, root_id, is_error, dur = generate_trace(
        tenant, svc, op, dt, rng
    )
    for row in span_rows:
        assert len(row) == len(SPAN_COLS), f"span row has {len(row)} fields, expected {len(SPAN_COLS)}"


def test_event_row_length_matches_cols():
    tenant = _tenant()
    # Force an error span
    op = Operation("POST /orders", 80.0, 400.0, 1.0)  # error_rate=1.0
    svc = Service("order-api", "1.0.0", "api", [op], [], 10.0)
    tenant.services[0] = svc
    dt = datetime(2024, 1, 10, 12, 0, tzinfo=timezone.utc)
    rng = random.Random(42)
    span_rows, event_rows, *_ = generate_trace(tenant, svc, op, dt, rng)
    for row in event_rows:
        assert len(row) == len(EVENT_COLS)


def test_all_spans_share_trace_id():
    tenant = _tenant()
    svc = tenant.services[0]
    op = svc.operations[0]
    dt = datetime(2024, 1, 10, 12, 0, tzinfo=timezone.utc)
    span_rows, _, trace_id, *_ = generate_trace(tenant, svc, op, dt, random.Random(1))
    for row in span_rows:
        col_idx = SPAN_COLS.index("trace_id")
        assert row[col_idx] == trace_id


def test_root_span_has_no_parent():
    tenant = _tenant()
    svc = tenant.services[0]
    op = svc.operations[0]
    dt = datetime(2024, 1, 10, 12, 0, tzinfo=timezone.utc)
    span_rows, _, _, root_id, *_ = generate_trace(tenant, svc, op, dt, random.Random(2))
    root_row = next(r for r in span_rows if r[SPAN_COLS.index("span_id")] == root_id)
    assert root_row[SPAN_COLS.index("parent_span_id")] is None


def test_child_spans_reference_root_as_parent():
    tenant = _tenant()
    svc = tenant.services[0]
    op = svc.operations[0]
    dt = datetime(2024, 1, 10, 12, 0, tzinfo=timezone.utc)
    span_rows, _, _, root_id, *_ = generate_trace(tenant, svc, op, dt, random.Random(3))
    children = [r for r in span_rows if r[SPAN_COLS.index("span_id")] != root_id]
    for child in children:
        assert child[SPAN_COLS.index("parent_span_id")] == root_id


def test_error_span_has_status_code_error():
    op = Operation("POST /orders", 80.0, 400.0, 1.0)
    svc = Service("order-api", "1.0.0", "api", [op], [], 10.0)
    tenant = Tenant("tid", "acme", "production", "key", [svc])
    dt = datetime(2024, 1, 10, 12, 0, tzinfo=timezone.utc)
    span_rows, event_rows, _, root_id, is_error, _ = generate_trace(
        tenant, svc, op, dt, random.Random(0)
    )
    assert is_error
    root_row = next(r for r in span_rows if r[SPAN_COLS.index("span_id")] == root_id)
    assert root_row[SPAN_COLS.index("status_code")] == "ERROR"
    assert len(event_rows) >= 1


def test_ok_span_produces_no_events():
    op = Operation("GET /health", 5.0, 20.0, 0.0)
    svc = Service("health-api", "1.0.0", "api", [op], [], 2.0)
    tenant = Tenant("tid", "acme", "production", "key", [svc])
    dt = datetime(2024, 1, 10, 12, 0, tzinfo=timezone.utc)
    _, event_rows, *_ = generate_trace(tenant, svc, op, dt, random.Random(0))
    assert len(event_rows) == 0


def test_tenant_id_stamped_on_all_rows():
    tenant = _tenant()
    svc = tenant.services[0]
    op = svc.operations[0]
    dt = datetime(2024, 1, 10, 12, 0, tzinfo=timezone.utc)
    span_rows, event_rows, *_ = generate_trace(tenant, svc, op, dt, random.Random(9))
    for row in span_rows:
        assert row[SPAN_COLS.index("tenant_id")] == "tid-001"
    for row in event_rows:
        assert row[EVENT_COLS.index("tenant_id")] == "tid-001"
```

- [ ] **Step 2: Run to confirm failure**

```bash
cd scripts && python -m pytest seed/tests/test_traces.py -v 2>&1 | head -10
```
Expected: `ModuleNotFoundError: No module named 'seed.traces'`

- [ ] **Step 3: Write traces.py**

`scripts/seed/traces.py`:
```python
from __future__ import annotations

import json
import random
import uuid
from datetime import datetime

from seed.world import Operation, Service, Tenant, apply_stories, sample_duration_ms

SPAN_COLS = [
    "tenant_id", "trace_id", "span_id", "parent_span_id",
    "service_name", "service_namespace", "service_version",
    "operation_name", "span_kind",
    "start_time_unix_nano", "end_time_unix_nano", "duration_ns",
    "status_code", "status_message",
    "attributes", "resource_attributes",
    "environment", "host_id", "workload", "deployment_id",
]

EVENT_COLS = [
    "tenant_id", "trace_id", "span_id", "event_index",
    "name", "timestamp_unix_nano", "attributes",
]

_SPAN_KIND = {"api": "SERVER", "frontend": "SERVER", "worker": "INTERNAL", "consumer": "CONSUMER"}


def generate_trace(
    tenant: Tenant,
    service: Service,
    operation: Operation,
    dt: datetime,
    rng: random.Random,
) -> tuple[list[list], list[list], str, str, bool, float]:
    """
    Generate span rows and span_event rows for one request.

    Returns (span_rows, event_rows, trace_id, root_span_id, is_error, root_duration_ms).
    """
    latency_mult, error_rate, version_override = apply_stories(
        tenant, dt, service.service_name, operation.error_rate
    )
    version = version_override or service.service_version
    is_error = rng.random() < error_rate

    trace_id = uuid.uuid4().hex + uuid.uuid4().hex  # 32 hex chars, no dashes
    root_span_id = uuid.uuid4().hex[:16]
    dt_ns = int(dt.timestamp() * 1e9)
    host = f"host-{rng.randint(1, 10)}"

    root_duration_ms = sample_duration_ms(operation.p50_ms, operation.p99_ms, rng) * latency_mult
    root_duration_ns = int(root_duration_ms * 1e6)

    span_rows: list[list] = []
    event_rows: list[list] = []

    # Root span
    span_rows.append([
        tenant.tenant_id,
        trace_id,
        root_span_id,
        None,
        service.service_name,
        "",
        version,
        operation.name,
        _SPAN_KIND.get(service.service_type, "INTERNAL"),
        dt_ns,
        dt_ns + root_duration_ns,
        root_duration_ns,
        "ERROR" if is_error else "OK",
        "request failed" if is_error else "",
        json.dumps({"http.method": operation.name.split()[0] if " " in operation.name else operation.name}),
        json.dumps({"service.name": service.service_name, "service.version": version}),
        tenant.environment,
        host,
        service.service_name,
        "",
    ])

    if is_error:
        event_rows.append([
            tenant.tenant_id,
            trace_id,
            root_span_id,
            0,
            "exception",
            dt_ns + root_duration_ns // 2,
            json.dumps({
                "exception.type": "RuntimeError",
                "exception.message": f"{operation.name} failed",
            }),
        ])

    # Child spans for downstream services
    child_offset_ns = rng.randint(1_000_000, 5_000_000)  # 1–5 ms processing before first call
    service_map = {s.service_name: s for s in tenant.services}

    for i, downstream_name in enumerate(service.downstream):
        down_svc = service_map.get(downstream_name)
        if down_svc is None:
            continue
        child_op = rng.choice(down_svc.operations)
        child_duration_ms = sample_duration_ms(child_op.p50_ms, child_op.p99_ms, rng)
        child_duration_ns = int(child_duration_ms * 1e6)
        child_start_ns = dt_ns + child_offset_ns
        child_span_id = uuid.uuid4().hex[:16]
        child_is_error = is_error and rng.random() < 0.5

        span_rows.append([
            tenant.tenant_id,
            trace_id,
            child_span_id,
            root_span_id,
            down_svc.service_name,
            "",
            down_svc.service_version,
            child_op.name,
            "CLIENT",
            child_start_ns,
            child_start_ns + child_duration_ns,
            child_duration_ns,
            "ERROR" if child_is_error else "OK",
            "downstream failure" if child_is_error else "",
            json.dumps({}),
            json.dumps({"service.name": down_svc.service_name}),
            tenant.environment,
            host,
            service.service_name,
            "",
        ])

        child_offset_ns += child_duration_ns + rng.randint(500_000, 2_000_000)

    return span_rows, event_rows, trace_id, root_span_id, is_error, root_duration_ms
```

- [ ] **Step 4: Run tests**

```bash
cd scripts && python -m pytest seed/tests/test_traces.py -v
```
Expected: All 8 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add scripts/seed/traces.py scripts/seed/tests/test_traces.py
git commit -m "feat(seed): trace generator — spans and span_events"
```

---

### Task 7: Log generator

**Files:**
- Create: `scripts/seed/logs.py`
- Create: `scripts/seed/tests/test_logs.py`

- [ ] **Step 1: Write failing tests**

`scripts/seed/tests/test_logs.py`:
```python
import random
from datetime import datetime, timezone

from seed.world import Operation, Service, Tenant
from seed.logs import LOG_COLS, generate_request_log, generate_background_log


def _tenant() -> Tenant:
    op = Operation("POST /orders", 80.0, 400.0, 0.02)
    svc = Service("order-api", "1.0.0", "api", [op], [], 10.0)
    return Tenant("tid-001", "acme", "production", "key", [svc])


def test_request_log_row_length():
    tenant = _tenant()
    svc = tenant.services[0]
    op = svc.operations[0]
    dt = datetime(2024, 1, 10, 12, 0, tzinfo=timezone.utc)
    row = generate_request_log(tenant, svc, op, "trace-abc", "span-xyz", "OK", 80.0, dt, random.Random(0))
    assert len(row) == len(LOG_COLS)


def test_background_log_row_length():
    tenant = _tenant()
    svc = tenant.services[0]
    dt = datetime(2024, 1, 10, 12, 0, tzinfo=timezone.utc)
    row = generate_background_log(tenant, svc, dt, random.Random(0))
    assert len(row) == len(LOG_COLS)


def test_error_span_produces_error_log():
    tenant = _tenant()
    svc = tenant.services[0]
    op = svc.operations[0]
    dt = datetime(2024, 1, 10, 12, 0, tzinfo=timezone.utc)
    row = generate_request_log(tenant, svc, op, "trace-abc", "span-xyz", "ERROR", 80.0, dt, random.Random(0))
    sev_num = row[LOG_COLS.index("severity_number")]
    sev_text = row[LOG_COLS.index("severity_text")]
    assert sev_num == 17
    assert sev_text == "ERROR"


def test_ok_fast_span_produces_info_log():
    tenant = _tenant()
    svc = tenant.services[0]
    op = svc.operations[0]
    dt = datetime(2024, 1, 10, 12, 0, tzinfo=timezone.utc)
    row = generate_request_log(tenant, svc, op, "trace-abc", "span-xyz", "OK", 30.0, dt, random.Random(0))
    assert row[LOG_COLS.index("severity_number")] == 9
    assert row[LOG_COLS.index("severity_text")] == "INFO"


def test_ok_slow_span_produces_warn_log():
    tenant = _tenant()
    svc = tenant.services[0]
    op = svc.operations[0]
    dt = datetime(2024, 1, 10, 12, 0, tzinfo=timezone.utc)
    row = generate_request_log(tenant, svc, op, "trace-abc", "span-xyz", "OK", 600.0, dt, random.Random(0))
    assert row[LOG_COLS.index("severity_number")] == 13
    assert row[LOG_COLS.index("severity_text")] == "WARN"


def test_request_log_links_trace_and_span():
    tenant = _tenant()
    svc = tenant.services[0]
    op = svc.operations[0]
    dt = datetime(2024, 1, 10, 12, 0, tzinfo=timezone.utc)
    row = generate_request_log(tenant, svc, op, "mytrace", "myspan", "OK", 50.0, dt, random.Random(0))
    assert row[LOG_COLS.index("trace_id")] == "mytrace"
    assert row[LOG_COLS.index("span_id")] == "myspan"


def test_background_log_has_no_trace_link():
    tenant = _tenant()
    svc = tenant.services[0]
    dt = datetime(2024, 1, 10, 12, 0, tzinfo=timezone.utc)
    row = generate_background_log(tenant, svc, dt, random.Random(0))
    assert row[LOG_COLS.index("trace_id")] is None
    assert row[LOG_COLS.index("span_id")] is None


def test_tenant_id_stamped():
    tenant = _tenant()
    svc = tenant.services[0]
    op = svc.operations[0]
    dt = datetime(2024, 1, 10, 12, 0, tzinfo=timezone.utc)
    row = generate_request_log(tenant, svc, op, "t", "s", "OK", 50.0, dt, random.Random(0))
    assert row[LOG_COLS.index("tenant_id")] == "tid-001"
```

- [ ] **Step 2: Run to confirm failure**

```bash
cd scripts && python -m pytest seed/tests/test_logs.py -v 2>&1 | head -10
```
Expected: `ModuleNotFoundError: No module named 'seed.logs'`

- [ ] **Step 3: Write logs.py**

`scripts/seed/logs.py`:
```python
from __future__ import annotations

import json
import random
import uuid
from datetime import datetime

from seed.world import Service, Tenant

LOG_COLS = [
    "tenant_id", "log_id", "timestamp_unix_nano", "observed_timestamp_unix_nano",
    "severity_number", "severity_text", "body",
    "trace_id", "span_id",
    "attributes", "resource_attributes",
    "service_name", "environment", "host_id", "fingerprint",
]

_SLOW_THRESHOLD_MS = 500.0

_BACKGROUND_MESSAGES = [
    "health check OK",
    "connection pool size 10",
    "GC pause {}ms",
    "cache hit ratio {}%",
    "metrics flushed",
    "keepalive sent",
    "config reloaded",
]


def generate_request_log(
    tenant: Tenant,
    service: Service,
    operation,
    trace_id: str,
    span_id: str,
    status_code: str,
    duration_ms: float,
    dt: datetime,
    rng: random.Random,
) -> list:
    """Generate one log record correlated to a root span."""
    ts_ns = int(dt.timestamp() * 1e9)
    is_error = status_code == "ERROR"
    is_slow = not is_error and duration_ms > _SLOW_THRESHOLD_MS

    if is_error:
        sev_num, sev_text = 17, "ERROR"
    elif is_slow:
        sev_num, sev_text = 13, "WARN"
    else:
        sev_num, sev_text = 9, "INFO"

    method = operation.name.split()[0] if " " in operation.name else operation.name
    path = operation.name.split()[-1] if " " in operation.name else ""
    code = "500" if is_error else "200"
    body = f"{method} {path} {code} {duration_ms:.0f}ms".strip()

    return [
        tenant.tenant_id,
        str(uuid.uuid4()),
        ts_ns,
        ts_ns + rng.randint(0, 1_000_000),
        sev_num,
        sev_text,
        body,
        trace_id,
        span_id,
        json.dumps({"operation": operation.name}),
        json.dumps({"service.name": service.service_name, "service.version": service.service_version}),
        service.service_name,
        tenant.environment,
        f"host-{rng.randint(1, 10)}",
        None,
    ]


def generate_background_log(
    tenant: Tenant,
    service: Service,
    dt: datetime,
    rng: random.Random,
) -> list:
    """Generate one unlinked background log (heartbeat, GC, etc.)."""
    ts_ns = int(dt.timestamp() * 1e9)
    tmpl = rng.choice(_BACKGROUND_MESSAGES)
    body = tmpl.format(rng.randint(2, 30)) if "{}" in tmpl else tmpl

    return [
        tenant.tenant_id,
        str(uuid.uuid4()),
        ts_ns,
        ts_ns,
        9,
        "INFO",
        body,
        None,
        None,
        json.dumps({}),
        json.dumps({"service.name": service.service_name}),
        service.service_name,
        tenant.environment,
        f"host-{rng.randint(1, 10)}",
        None,
    ]
```

- [ ] **Step 4: Run tests**

```bash
cd scripts && python -m pytest seed/tests/test_logs.py -v
```
Expected: All 8 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add scripts/seed/logs.py scripts/seed/tests/test_logs.py
git commit -m "feat(seed): log generator with severity mapping and trace correlation"
```

---

### Task 8: Metrics generator

**Files:**
- Create: `scripts/seed/metrics.py`
- Create: `scripts/seed/tests/test_metrics.py`

`metric_series` uses `ReplacingMergeTree` — one row per unique `(tenant_id, service_name, metric_name, attributes)`. `metric_points` has one row per 15s cadence tick per series.

- [ ] **Step 1: Write failing tests**

`scripts/seed/tests/test_metrics.py`:
```python
import json
import random
from datetime import datetime, timezone

from seed.world import Operation, Service, Tenant
from seed.metrics import SERIES_COLS, POINT_COLS, make_series_rows, generate_metric_points


def _tenant() -> Tenant:
    op = Operation("POST /orders", 80.0, 400.0, 0.02)
    svc = Service("order-api", "1.0.0", "api", [op], [], 10.0)
    return Tenant("tid-001", "acme", "production", "key", [svc])


def test_series_row_length():
    tenant = _tenant()
    svc = tenant.services[0]
    rows = make_series_rows(tenant, svc)
    for row in rows:
        assert len(row) == len(SERIES_COLS)


def test_series_has_four_metrics_per_service():
    tenant = _tenant()
    svc = tenant.services[0]
    rows = make_series_rows(tenant, svc)
    assert len(rows) == 4


def test_series_tenant_id_stamped():
    tenant = _tenant()
    svc = tenant.services[0]
    rows = make_series_rows(tenant, svc)
    for row in rows:
        assert row[SERIES_COLS.index("tenant_id")] == "tid-001"


def test_point_row_length():
    tenant = _tenant()
    svc = tenant.services[0]
    series_rows = make_series_rows(tenant, svc)
    dt = datetime(2024, 1, 10, 12, 0, tzinfo=timezone.utc)
    points = generate_metric_points(tenant, svc, series_rows, dt, 100, random.Random(0))
    for row in points:
        assert len(row) == len(POINT_COLS)


def test_four_points_per_tick():
    tenant = _tenant()
    svc = tenant.services[0]
    series_rows = make_series_rows(tenant, svc)
    dt = datetime(2024, 1, 10, 12, 0, tzinfo=timezone.utc)
    points = generate_metric_points(tenant, svc, series_rows, dt, 100, random.Random(0))
    assert len(points) == 4


def test_request_count_non_negative():
    tenant = _tenant()
    svc = tenant.services[0]
    series_rows = make_series_rows(tenant, svc)
    dt = datetime(2024, 1, 10, 12, 0, tzinfo=timezone.utc)
    points = generate_metric_points(tenant, svc, series_rows, dt, 50, random.Random(0))
    req_metric = next(p for p in points if p[POINT_COLS.index("metric_name")] == "http.server.request_count")
    assert req_metric[POINT_COLS.index("value_int")] >= 0


def test_series_ids_are_unique():
    tenant = _tenant()
    svc = tenant.services[0]
    rows = make_series_rows(tenant, svc)
    ids = [row[SERIES_COLS.index("metric_series_id")] for row in rows]
    assert len(ids) == len(set(ids))
```

- [ ] **Step 2: Run to confirm failure**

```bash
cd scripts && python -m pytest seed/tests/test_metrics.py -v 2>&1 | head -10
```
Expected: `ModuleNotFoundError: No module named 'seed.metrics'`

- [ ] **Step 3: Write metrics.py**

`scripts/seed/metrics.py`:
```python
from __future__ import annotations

import json
import random
import uuid
from datetime import datetime

from seed.world import Service, Tenant

SERIES_COLS = [
    "tenant_id", "metric_series_id", "metric_name", "description", "unit",
    "metric_type", "is_monotonic", "aggregation_temporality",
    "attributes", "resource_attributes", "service_name", "environment", "created_at",
]

POINT_COLS = [
    "tenant_id", "metric_series_id", "metric_name", "service_name",
    "time_unix_nano", "start_time_unix_nano",
    "value_double", "value_int",
    "histogram_count", "histogram_sum",
    "histogram_bucket_counts", "histogram_explicit_bounds",
]

# Fixed metric definitions emitted by every service
_METRIC_DEFS = [
    ("http.server.request_count", "Total HTTP requests", "requests", "sum", 1, "DELTA"),
    ("http.server.error_count",   "Total HTTP errors",   "requests", "sum", 1, "DELTA"),
    ("http.server.duration",      "Request duration",    "ms",       "histogram", None, "DELTA"),
    ("process.memory.usage",      "Memory usage",        "bytes",    "gauge",     None, None),
]


def make_series_rows(tenant: Tenant, service: Service) -> list[list]:
    """Return one metric_series row per metric definition for this service.

    Series IDs are deterministic from tenant_id + service_name + metric_name
    so repeated calls produce the same UUIDs (idempotent for ReplacingMergeTree).
    """
    rows = []
    now_str = datetime.utcnow().strftime("%Y-%m-%d %H:%M:%S")
    for name, desc, unit, mtype, is_mono, temporality in _METRIC_DEFS:
        series_id = str(uuid.uuid5(
            uuid.NAMESPACE_OID,
            f"{tenant.tenant_id}:{service.service_name}:{name}",
        ))
        rows.append([
            tenant.tenant_id,
            series_id,
            name,
            desc,
            unit,
            mtype,
            is_mono,
            temporality,
            json.dumps({}),
            json.dumps({"service.name": service.service_name}),
            service.service_name,
            tenant.environment,
            now_str,
        ])
    return rows


def generate_metric_points(
    tenant: Tenant,
    service: Service,
    series_rows: list[list],
    dt: datetime,
    request_count: int,
    rng: random.Random,
) -> list[list]:
    """Return one metric_point row per series for the given 15-second tick."""
    ts_ns = int(dt.timestamp() * 1e9)
    error_count = max(0, int(request_count * rng.uniform(0.005, 0.05)))
    mem_bytes = rng.randint(64 * 1024 * 1024, 512 * 1024 * 1024)

    series_map = {r[SERIES_COLS.index("metric_name")]: r for r in series_rows}
    rows = []

    def _sid(name: str) -> str:
        return series_map[name][SERIES_COLS.index("metric_series_id")]

    # http.server.request_count — counter (value_int)
    rows.append([
        tenant.tenant_id, _sid("http.server.request_count"), "http.server.request_count", service.service_name,
        ts_ns, ts_ns, None, request_count, None, None, [], [],
    ])

    # http.server.error_count — counter (value_int)
    rows.append([
        tenant.tenant_id, _sid("http.server.error_count"), "http.server.error_count", service.service_name,
        ts_ns, ts_ns, None, error_count, None, None, [], [],
    ])

    # http.server.duration — histogram
    bounds = [5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0, 2500.0, 5000.0]
    bucket_counts = [rng.randint(0, max(1, request_count // 10)) for _ in range(len(bounds) + 1)]
    total_dur = sum(b * c for b, c in zip(bounds, bucket_counts[:-1]))
    rows.append([
        tenant.tenant_id, _sid("http.server.duration"), "http.server.duration", service.service_name,
        ts_ns, ts_ns, None, None,
        sum(bucket_counts), total_dur,
        bucket_counts, bounds,
    ])

    # process.memory.usage — gauge (value_int)
    rows.append([
        tenant.tenant_id, _sid("process.memory.usage"), "process.memory.usage", service.service_name,
        ts_ns, None, None, mem_bytes, None, None, [], [],
    ])

    return rows
```

- [ ] **Step 4: Run tests**

```bash
cd scripts && python -m pytest seed/tests/test_metrics.py -v
```
Expected: All 7 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add scripts/seed/metrics.py scripts/seed/tests/test_metrics.py
git commit -m "feat(seed): metrics generator — series and points"
```

---

### Task 9: CLI and orchestration loop

**Files:**
- Create: `scripts/seed/__main__.py`
- Create: `scripts/seed/tests/test_cli.py`

This is the time-walk loop that ties everything together. It walks time in 1-minute buckets, calls all generators, and flushes periodically.

- [ ] **Step 1: Write failing test**

`scripts/seed/tests/test_cli.py`:
```python
import sys
from unittest.mock import MagicMock, patch


def test_dry_run_exits_zero():
    with patch("seed.pg_seeder.seed_postgres") as mock_pg, \
         patch("seed.inserter.clickhouse_connect.get_client", return_value=MagicMock()):
        from seed.__main__ import run
        run([
            "--profile", "small",
            "--dry-run",
            "--clickhouse-host", "localhost",
            "--clickhouse-port", "8123",
            "--clickhouse-user", "default",
            "--clickhouse-password", "pw",
            "--postgres-url", "postgresql://x/y",
        ])
        mock_pg.assert_called_once()


def test_no_postgres_flag_skips_pg_seeder():
    with patch("seed.pg_seeder.seed_postgres") as mock_pg, \
         patch("seed.inserter.clickhouse_connect.get_client", return_value=MagicMock()):
        from seed.__main__ import run
        run([
            "--profile", "small",
            "--dry-run",
            "--no-postgres",
            "--clickhouse-host", "localhost",
            "--clickhouse-port", "8123",
            "--clickhouse-user", "default",
            "--clickhouse-password", "pw",
            "--postgres-url", "postgresql://x/y",
        ])
        mock_pg.assert_not_called()
```

- [ ] **Step 2: Run to confirm failure**

```bash
cd scripts && python -m pytest seed/tests/test_cli.py -v 2>&1 | head -10
```
Expected: `ModuleNotFoundError: No module named 'seed.__main__'`

- [ ] **Step 3: Write __main__.py**

`scripts/seed/__main__.py`:
```python
from __future__ import annotations

import argparse
import math
import random
import sys
from datetime import datetime, timedelta, timezone

from seed.inserter import Inserter
from seed.logs import LOG_COLS, generate_background_log, generate_request_log
from seed.metrics import POINT_COLS, SERIES_COLS, generate_metric_points, make_series_rows
from seed.pg_seeder import seed_postgres
from seed.traces import EVENT_COLS, SPAN_COLS, generate_trace
from seed.world import WorldModel, build_world, load_profile, traffic_multiplier

# How often to emit background logs (per service, per minute bucket)
_BACKGROUND_LOG_PROB = 0.05
# Metrics cadence: emit one tick per N seconds of simulated time
_METRIC_CADENCE_S = 15


def _parse(argv: list[str] | None) -> argparse.Namespace:
    p = argparse.ArgumentParser(description="Seed Observable ClickHouse + Postgres with historical telemetry")
    p.add_argument("--profile", default="small", choices=["small", "medium", "large"])
    p.add_argument("--tenants", type=int, default=None)
    p.add_argument("--services", type=int, default=None, dest="services_per_tenant")
    p.add_argument("--days", type=int, default=None)
    p.add_argument("--seed", type=int, default=42)
    p.add_argument("--batch-size", type=int, default=10_000)
    p.add_argument("--resume", action="store_true", help="Skip tenants that already have span data")
    p.add_argument("--no-postgres", action="store_true", help="Skip Postgres seeding")
    p.add_argument("--dry-run", action="store_true", help="Print world summary, insert nothing")
    p.add_argument("--clickhouse-host", default="localhost")
    p.add_argument("--clickhouse-port", type=int, default=8123)
    p.add_argument("--clickhouse-user", default="default")
    p.add_argument("--clickhouse-password", default="observable")
    p.add_argument("--postgres-url", default="postgresql://observable:observable@localhost:5432/observable")
    return p.parse_args(argv)


def run(argv: list[str] | None = None) -> None:
    args = _parse(argv)

    overrides = {k: v for k, v in {
        "tenants": args.tenants,
        "services_per_tenant": args.services_per_tenant,
        "days": args.days,
    }.items() if v is not None}

    world = load_profile(args.profile, seed=args.seed, **overrides)

    print(f"World: {len(world.tenants)} tenants, "
          f"{sum(len(t.services) for t in world.tenants)} total services, "
          f"{(world.end_time - world.start_time).days} days")
    print(f"Window: {world.start_time.isoformat()} → {world.end_time.isoformat()}")

    if not args.no_postgres:
        seed_postgres(world.tenants, args.postgres_url, dry_run=args.dry_run)

    if args.dry_run:
        print("[dry-run] Skipping ClickHouse inserts.")
        return

    inserter = Inserter(
        host=args.clickhouse_host,
        port=args.clickhouse_port,
        user=args.clickhouse_user,
        password=args.clickhouse_password,
        batch_size=args.batch_size,
    )

    try:
        _run_inserts(world, inserter, args.resume)
    finally:
        inserter.close()


def _run_inserts(world: WorldModel, inserter: Inserter, resume: bool) -> None:
    # Build per-tenant series rows upfront (idempotent for ReplacingMergeTree)
    all_series: list[list] = []
    for tenant in world.tenants:
        for svc in tenant.services:
            all_series.extend(make_series_rows(tenant, svc))
    inserter.insert_in_batches("observable.metric_series", SERIES_COLS, all_series, desc="metric_series")

    # Time walk: 1-minute buckets
    span_buf: list[list] = []
    event_buf: list[list] = []
    log_buf: list[list] = []
    point_buf: list[list] = []

    # Pre-build series_rows lookup: (tenant_id, service_name) -> list[series_row]
    series_lookup: dict[tuple[str, str], list[list]] = {}
    for tenant in world.tenants:
        for svc in tenant.services:
            series_lookup[(tenant.tenant_id, svc.service_name)] = make_series_rows(tenant, svc)

    # Determine tenants to skip (--resume)
    skip_tenant_ids: set[str] = set()
    if resume:
        for tenant in world.tenants:
            count = inserter.row_count("observable.spans", tenant.tenant_id)
            if count > 0:
                print(f"[resume] Skipping tenant {tenant.name} ({count} spans exist)")
                skip_tenant_ids.add(tenant.tenant_id)

    active_tenants = [t for t in world.tenants if t.tenant_id not in skip_tenant_ids]
    if not active_tenants:
        print("Nothing to insert (all tenants already have data).")
        return

    total_minutes = int((world.end_time - world.start_time).total_seconds() / 60)
    print(f"Walking {total_minutes:,} minute-buckets × {len(active_tenants)} tenants...")

    metric_tick_counter: dict[tuple[str, str], int] = {}

    current = world.start_time
    while current < world.end_time:
        multiplier = traffic_multiplier(current)
        bucket_seed = int(current.timestamp()) ^ world.seed
        rng = random.Random(bucket_seed)

        for tenant in active_tenants:
            for svc in tenant.services:
                expected = svc.rps * 60 * multiplier
                count = int(expected) + (1 if rng.random() < (expected % 1) else 0)

                for _ in range(max(0, count)):
                    op = rng.choice(svc.operations)
                    offset_s = rng.uniform(0, 60)
                    dt = current + timedelta(seconds=offset_s)

                    sp_rows, ev_rows, trace_id, root_sid, is_error, dur_ms = generate_trace(
                        tenant, svc, op, dt, rng
                    )
                    log_row = generate_request_log(
                        tenant, svc, op, trace_id, root_sid,
                        "ERROR" if is_error else "OK", dur_ms, dt, rng
                    )
                    span_buf.extend(sp_rows)
                    event_buf.extend(ev_rows)
                    log_buf.append(log_row)

                # Background log (low rate)
                if rng.random() < _BACKGROUND_LOG_PROB:
                    log_buf.append(generate_background_log(tenant, svc, current, rng))

                # Metric tick every _METRIC_CADENCE_S seconds of simulated time
                key = (tenant.tenant_id, svc.service_name)
                tick = metric_tick_counter.get(key, 0)
                elapsed_s = int((current - world.start_time).total_seconds())
                new_tick = elapsed_s // _METRIC_CADENCE_S
                if new_tick > tick:
                    metric_tick_counter[key] = new_tick
                    s_rows = series_lookup[(tenant.tenant_id, svc.service_name)]
                    point_buf.extend(generate_metric_points(tenant, svc, s_rows, current, count, rng))

        current += timedelta(minutes=1)

        # Flush when buffers get large to avoid OOM
        if len(span_buf) >= 50_000:
            inserter.insert_in_batches("observable.spans", SPAN_COLS, span_buf, desc="spans")
            inserter.insert_in_batches("observable.span_events", EVENT_COLS, event_buf, desc="span_events")
            inserter.insert_in_batches("observable.logs", LOG_COLS, log_buf, desc="logs")
            inserter.insert_in_batches("observable.metric_points", POINT_COLS, point_buf, desc="metric_points")
            span_buf.clear(); event_buf.clear(); log_buf.clear(); point_buf.clear()

    # Final flush
    if span_buf:
        inserter.insert_in_batches("observable.spans", SPAN_COLS, span_buf, desc="spans (final)")
    if event_buf:
        inserter.insert_in_batches("observable.span_events", EVENT_COLS, event_buf, desc="span_events (final)")
    if log_buf:
        inserter.insert_in_batches("observable.logs", LOG_COLS, log_buf, desc="logs (final)")
    if point_buf:
        inserter.insert_in_batches("observable.metric_points", POINT_COLS, point_buf, desc="metric_points (final)")

    print("Done.")


def main() -> None:
    run(sys.argv[1:])


if __name__ == "__main__":
    main()
```

- [ ] **Step 4: Run tests**

```bash
cd scripts && python -m pytest seed/tests/test_cli.py -v
```
Expected: Both tests PASS.

- [ ] **Step 5: Run full test suite**

```bash
cd scripts && python -m pytest seed/tests/ -v
```
Expected: All tests PASS, no failures.

- [ ] **Step 6: Commit**

```bash
git add scripts/seed/__main__.py scripts/seed/tests/test_cli.py
git commit -m "feat(seed): CLI argparse and time-walk orchestration loop"
```

---

### Task 10: Docker Compose integration

**Files:**
- Modify: `docker-compose.yml` — add `seed` profile service

- [ ] **Step 1: Add seed service to docker-compose.yml**

Open `docker-compose.yml` and add the following block **before** the `volumes:` section at the bottom:

```yaml
  seed:
    build:
      context: .
      dockerfile: Dockerfile.seed
    profiles: ["seed"]
    environment:
      CLICKHOUSE_HOST: clickhouse
      CLICKHOUSE_PORT: "8123"
      CLICKHOUSE_USER: ${CH_USER:-default}
      CLICKHOUSE_PASSWORD: ${CH_PASSWORD:-observable}
      POSTGRES_URL: postgresql://${PG_USER:-observable}:${PG_PASSWORD:-observable}@postgres:5432/${PG_DB:-observable}
      SEED_PROFILE: ${SEED_PROFILE:-small}
    command: >
      python -m seed
      --profile ${SEED_PROFILE:-small}
      --clickhouse-host clickhouse
      --clickhouse-port 8123
      --clickhouse-user ${CH_USER:-default}
      --clickhouse-password ${CH_PASSWORD:-observable}
      --postgres-url postgresql://${PG_USER:-observable}:${PG_PASSWORD:-observable}@postgres:5432/${PG_DB:-observable}
    depends_on:
      clickhouse: { condition: service_healthy }
      postgres: { condition: service_healthy }
```

- [ ] **Step 2: Verify compose config renders without errors**

```bash
docker compose config --quiet
```
Expected: exits 0, no errors.

- [ ] **Step 3: Verify seed service is not started by default**

```bash
docker compose config --services | grep seed
```
Expected: `seed` appears in output (service exists but has `profiles: [seed]` so it won't start with plain `docker compose up`).

- [ ] **Step 4: Commit**

```bash
git add docker-compose.yml
git commit -m "feat(seed): add seed profile service to docker-compose.yml"
```

---

### Task 11: End-to-end smoke test

This task validates the generator works against the real local stack. Requires `docker compose up -d` to be running.

- [ ] **Step 1: Start the local stack**

```bash
docker compose up -d clickhouse postgres
```
Expected: Both services reach healthy state within 30 seconds.

- [ ] **Step 2: Run migrations**

```bash
bash scripts/migrate.sh
```
Expected: exits 0, all ClickHouse and Postgres migrations applied.

- [ ] **Step 3: Install seed dependencies locally**

```bash
cd scripts && pip install -r seed/requirements.txt -q
```

- [ ] **Step 4: Run dry-run to verify world model**

```bash
cd scripts && python seed.py --profile small --dry-run
```
Expected output (exact numbers may vary):
```
World: 3 tenants, 15 total services, 7 days
Window: 2024-... → 2024-...
[dry-run] Would insert 3 tenants + 3 api_keys
[dry-run] Skipping ClickHouse inserts.
```

- [ ] **Step 5: Run small profile against local stack**

```bash
cd scripts && python seed.py --profile small \
  --clickhouse-host localhost --clickhouse-port 8123 \
  --clickhouse-user default --clickhouse-password observable \
  --postgres-url postgresql://observable:observable@localhost:5432/observable
```
Expected: Completes without error. tqdm bars show progress for `metric_series`, `spans`, `logs`, `metric_points`.

- [ ] **Step 6: Verify data landed in ClickHouse**

```bash
docker compose exec clickhouse clickhouse-client --query \
  "SELECT count() FROM observable.spans"
```
Expected: a number > 0 (for `small` profile, roughly 100,000–600,000 spans).

```bash
docker compose exec clickhouse clickhouse-client --query \
  "SELECT count() FROM observable.logs"
```
Expected: a number > 0.

```bash
docker compose exec clickhouse clickhouse-client --query \
  "SELECT count() FROM observable.metric_points"
```
Expected: a number > 0.

- [ ] **Step 7: Verify tenant isolation — no cross-tenant rows**

```bash
docker compose exec clickhouse clickhouse-client --query \
  "SELECT tenant_id, count() FROM observable.spans GROUP BY tenant_id ORDER BY tenant_id"
```
Expected: exactly 3 distinct `tenant_id` values with non-zero counts each.

- [ ] **Step 8: Run via Docker Compose service**

```bash
docker compose --profile seed run --rm -e SEED_PROFILE=small seed
```
Expected: same outcome as step 5 (idempotent — ON CONFLICT + --resume not needed for clean run).

- [ ] **Step 9: Commit**

```bash
git add .
git commit -m "feat(seed): end-to-end smoke verified — seed generator complete"
```

---

## Self-review checklist

- **Spec coverage:**
  - [x] World model (tenants, service graphs, stories) — Task 2, 3
  - [x] Profiles small/medium/large — Task 3
  - [x] Postgres seeder (tenants + api_keys) — Task 5
  - [x] ClickHouse inserter with batching + tqdm — Task 4
  - [x] Trace generator (spans + span_events, parent-child) — Task 6
  - [x] Log generator (severity mapping, trace correlation, background) — Task 7
  - [x] Metrics generator (series + points, 15s cadence) — Task 8
  - [x] Time distribution (business hours, weekends) — world.py `traffic_multiplier` in Task 2, used in Task 9
  - [x] Stories (latency_spike, error_burst, deployment) — Task 2 + 3
  - [x] CLI with all flags from spec — Task 9
  - [x] `--resume` — Task 9 (`Inserter.row_count` + skip logic in `_run_inserts`)
  - [x] `--dry-run` — Task 9
  - [x] `--no-postgres` — Task 9
  - [x] Docker Compose `seed` profile service — Task 10

- **Type consistency:**
  - `apply_stories(tenant, dt, service_name, base_error_rate) -> (float, float, str|None)` — defined Task 2, used in traces.py Task 6
  - `generate_trace(...) -> (list[list], list[list], str, str, bool, float)` — defined Task 6, consumed in Task 9
  - `SPAN_COLS`, `EVENT_COLS`, `LOG_COLS`, `SERIES_COLS`, `POINT_COLS` — each defined in their module, imported in Task 9
  - `Inserter.insert_in_batches(table, column_names, rows, desc) -> int` — defined Task 4, used Task 9
  - `make_series_rows(tenant, svc) -> list[list]` — defined Task 8, used Task 9
  - `generate_metric_points(tenant, svc, series_rows, dt, count, rng) -> list[list]` — defined Task 8, used Task 9
