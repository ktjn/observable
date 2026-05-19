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
