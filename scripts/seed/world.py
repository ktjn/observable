from __future__ import annotations

import hashlib
import json
import math
import random
import uuid as _uuid
from dataclasses import dataclass, field
from datetime import datetime, timedelta, timezone
from pathlib import Path


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
