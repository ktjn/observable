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
