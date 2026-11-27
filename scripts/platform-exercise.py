#!/usr/bin/env python3
"""Platform exercises against a running compose stack (OBS-19-08, OBS-19-09).

  platform-exercise.py load     # 19-08: concurrent users on essential views, p95 vs. the approved figure
  platform-exercise.py growth   # 19-09: ingest and rules at the planning ceiling; detection,
                                #        notification and essential views must hold

Figures come only from config/exercises/platform-exercises.json. Each run writes a JSON report
(--report) and exits 1 when any approved figure is missed. Long runs: not part of every CI run
(.github/workflows/platform-exercises.yml).

Environment: EXERCISE_BASE_URL (default http://localhost:5173, the nginx front door),
EXERCISE_INGEST_URL (default http://localhost:4318), EXERCISE_API_KEY (default the dev key),
EXERCISE_SINK_FILE (growth: the notification sink's output, see docker-compose.yml exercise-sink).
"""

from __future__ import annotations

import argparse
import json
import math
import os
import random
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.request
import uuid
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
FIGURES = REPO / "config" / "exercises" / "platform-exercises.json"
DEV_TENANT = "00000000-0000-0000-0000-000000000001"


def figures() -> dict:
    return json.loads(FIGURES.read_text())


def p95(samples: list[float]) -> float:
    """Nearest-rank 95th percentile."""
    if not samples:
        return float("inf")
    s = sorted(samples)
    return s[max(0, math.ceil(0.95 * len(s)) - 1)]


class Views:
    """Concurrent users reading the essential views; records latency and failures per view."""

    def __init__(self, base: str, key: str, views: list[str], think: float) -> None:
        self.base, self.key, self.views, self.think = base.rstrip("/"), key, views, think
        self.lat: dict[str, list[float]] = {v: [] for v in views}
        self.errors: dict[str, int] = {v: 0 for v in views}
        self.lock = threading.Lock()

    def one(self, view: str) -> None:
        req = urllib.request.Request(self.base + view, headers={"authorization": f"Bearer {self.key}"})
        start = time.monotonic()
        ok = False
        try:
            with urllib.request.urlopen(req, timeout=30) as resp:
                resp.read()
                ok = 200 <= resp.status < 300
        except (urllib.error.URLError, OSError, TimeoutError):
            ok = False
        elapsed = time.monotonic() - start
        with self.lock:
            self.lat[view].append(elapsed)
            if not ok:
                self.errors[view] += 1

    def run(self, users: int, duration: float) -> None:
        deadline = time.monotonic() + duration

        def user(i: int) -> None:
            order = self.views[i % len(self.views):] + self.views[: i % len(self.views)]
            time.sleep(random.uniform(0, self.think))
            while time.monotonic() < deadline:
                for view in order:
                    if time.monotonic() >= deadline:
                        return
                    self.one(view)
                    time.sleep(self.think)

        threads = [threading.Thread(target=user, args=(i,), daemon=True) for i in range(users)]
        for t in threads:
            t.start()
        for t in threads:
            t.join(duration + 60)

    def verdict(self, budget: float, max_error_ratio: float) -> tuple[dict, list[str]]:
        report, misses = {}, []
        for v in self.views:
            n = len(self.lat[v])
            ratio = self.errors[v] / n if n else 1.0
            report[v] = {"requests": n, "errors": self.errors[v], "p95_seconds": round(p95(self.lat[v]), 4)}
            if n == 0:
                misses.append(f"{v}: no requests completed")
            if p95(self.lat[v]) > budget:
                misses.append(f"{v}: p95 {p95(self.lat[v]):.3f}s > {budget}s")
            if ratio > max_error_ratio:
                misses.append(f"{v}: error ratio {ratio:.4f} > {max_error_ratio}")
        return report, misses


def env(name: str, default: str) -> str:
    return os.environ.get(name, default)


def load(args: argparse.Namespace) -> int:
    f = figures()
    lf = f["load"]
    views = Views(env("EXERCISE_BASE_URL", "http://localhost:5173"),
                  env("EXERCISE_API_KEY", "dev-api-key-0000"), f["essential_views"], lf["think_time_seconds"])
    views.run(lf["concurrent_users"], lf["duration_seconds"])
    per_view, misses = views.verdict(lf["p95_seconds"], lf["max_error_ratio"])
    return finish(args.report, "OBS-19-08", {"figures": lf, "views": per_view}, misses)


# ── growth ──────────────────────────────────────────────────────────────────────

def compose(*cmd: str, stdin: str | None = None) -> str:
    out = subprocess.run(["docker", "compose", *cmd], input=stdin, capture_output=True, text=True,
                         cwd=REPO, check=True)
    return out.stdout


def psql(sql: str) -> str:
    return compose("exec", "-T", "postgres", "psql", "-v", "ON_ERROR_STOP=1", "-qAt",
                   "-U", env("PG_USER", "observable"), "-d", env("PG_DB", "observable"), stdin=sql)


def clickhouse(sql: str) -> str:
    return compose("exec", "-T", "clickhouse", "clickhouse-client",
                   "--user", env("CH_USER", "default"), "--password", env("CH_PASSWORD", "observable"),
                   "--query", sql)


def provision_rules(run: str, total: int, canaries: int, sink_url: str) -> list[str]:
    """Seeds the planning-ceiling rule set; returns the canary metric names.

    Rules are seeded as operator fixtures: rule CRUD needs a signed-in tenant admin, and the
    exercise measures detection and notification, not rule authoring.
    """
    channel = str(uuid.uuid4())
    rows, metrics = [], []
    for i in range(total):
        canary = i < canaries
        metric = f"w5c_{run}_{'canary' if canary else 'quiet'}_{i}"
        if canary:
            metrics.append(metric)
        cond = json.dumps({"metric_name": metric, "operator": "gt", "threshold": 100.0})
        chans = f"ARRAY['{channel}']::uuid[]" if canary else "'{}'::uuid[]"
        rows.append(f"('{uuid.uuid4()}', '{DEV_TENANT}', 'w5c {run} {i}', 'threshold', 'critical', "
                    f"'{cond}'::jsonb, {chans})")
    psql(
        "BEGIN;\n"
        f"INSERT INTO alerting.notification_channels (channel_id, tenant_id, name, type, config) VALUES "
        f"('{channel}', '{DEV_TENANT}', 'w5c {run} sink', 'webhook', "
        f"'{sql_text(json.dumps({'url': sink_url}))}'::jsonb);\n"
        "INSERT INTO alerting.alert_rules (rule_id, tenant_id, name, alert_type, severity, condition, "
        "notification_channels) VALUES\n" + ",\n".join(rows) + ";\nCOMMIT;\n"
    )
    return metrics


def sql_text(value: str) -> str:
    return value.replace("'", "''")


def cleanup_rules(run: str) -> None:
    """Silences the run's rules (fired ones are referenced by their firings) and drops the rest."""
    psql(f"UPDATE alerting.alert_rules SET silenced = true WHERE tenant_id = '{DEV_TENANT}' "
         f"AND name LIKE 'w5c {run} %';\n"
         f"DELETE FROM alerting.alert_rules r WHERE r.tenant_id = '{DEV_TENANT}' AND r.name LIKE 'w5c {run} %' "
         f"AND NOT EXISTS (SELECT 1 FROM alerting.alert_firings f WHERE f.rule_id = r.rule_id);")


def post(url: str, key: str, body: dict) -> bool:
    req = urllib.request.Request(url, data=json.dumps(body).encode(), method="POST",
                                 headers={"authorization": f"Bearer {key}", "content-type": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            return 200 <= resp.status < 300
    except (urllib.error.URLError, OSError, TimeoutError):
        return False


def spans(run: str, services: int, n: int, offset: int) -> dict:
    now = time.time_ns()
    by_service: dict[int, list[dict]] = {}
    for k in range(n):
        s = (offset + k) % services
        by_service.setdefault(s, []).append({
            "traceId": uuid.uuid4().hex, "spanId": uuid.uuid4().hex[:16], "name": "GET /essential",
            "kind": 2, "startTimeUnixNano": str(now - 20_000_000), "endTimeUnixNano": str(now),
            "status": {"code": 1},
        })
    return {"resourceSpans": [
        {"resource": {"attributes": [{"key": "service.name", "value": {"stringValue": f"w5c-{run}-svc-{s}"}}]},
         "scopeSpans": [{"spans": batch}]}
        for s, batch in by_service.items()
    ]}


def gauge(metric: str, value: float) -> dict:
    return {"resourceMetrics": [{
        "resource": {"attributes": [{"key": "service.name", "value": {"stringValue": "w5c-growth-canary"}}]},
        "scopeMetrics": [{"metrics": [{"name": metric, "gauge": {"dataPoints": [
            {"asDouble": value, "timeUnixNano": str(time.time_ns())}]}}]}],
    }]}


def ingest(ingest_url: str, key: str, run: str, g: dict, stop: threading.Event, stats: dict) -> None:
    """Paced span ingest at the planning ceiling, spread over the planned service count."""
    per_req = g["spans_per_request"]
    rps = g["spans_per_second"] / per_req
    workers = max(4, int(rps * 2))
    sent, failed, lock = [0], [0], threading.Lock()
    tick = 1.0 / rps
    next_at = [time.monotonic()]

    def worker() -> None:
        while not stop.is_set():
            with lock:
                at = next_at[0]
                next_at[0] += tick
                offset = sent[0] + failed[0]
            delay = at - time.monotonic()
            if delay > 0:
                time.sleep(delay)
            ok = post(f"{ingest_url}/v1/traces", key, spans(run, g["services"], per_req, offset))
            with lock:
                (sent if ok else failed)[0] += per_req

    started = time.monotonic()
    threads = [threading.Thread(target=worker, daemon=True) for _ in range(workers)]
    for t in threads:
        t.start()
    stop.wait()
    for t in threads:
        t.join(60)
    stats.update(sent=sent[0], failed=failed[0], seconds=time.monotonic() - started)


def sink_events(path: Path) -> list[dict]:
    if not path.exists():
        return []
    return [json.loads(line) for line in path.read_text().splitlines() if line.strip()]


def growth(args: argparse.Namespace) -> int:
    f = figures()
    g = f["growth"]
    key = env("EXERCISE_API_KEY", "dev-api-key-0000")
    ingest_url = env("EXERCISE_INGEST_URL", "http://localhost:4318").rstrip("/")
    sink_file = Path(env("EXERCISE_SINK_FILE", str(REPO / "target" / "exercises" / "notifications.jsonl")))
    run = uuid.uuid4().hex[:8]
    misses: list[str] = []
    budget = g["evaluation_interval_seconds"] * g["detection_within_evaluation_intervals"] \
        + g["notification_dispatch_seconds"]

    canaries = provision_rules(run, g["rules"], g["canary_rules"], env("EXERCISE_SINK_URL", "http://exercise-sink:9099/"))
    try:
        stop, stats = threading.Event(), {}
        feeder = threading.Thread(target=ingest, args=(ingest_url, key, run, g, stop, stats), daemon=True)
        feeder.start()
        views = Views(env("EXERCISE_BASE_URL", "http://localhost:5173"), key, f["essential_views"], 1.0)
        viewer = threading.Thread(target=views.run, args=(g["view_users"], g["duration_seconds"]), daemon=True)
        viewer.start()

        # Midway through the ceiling load, every canary crosses its threshold.
        time.sleep(g["duration_seconds"] / 3)
        crossed: dict[str, float] = {}
        for m in canaries:
            if not post(f"{ingest_url}/v1/metrics", key, gauge(m, 500.0)):
                misses.append(f"{m}: canary point not accepted")
        # Detection is timed from when the evidence is queryable, so ingest lag is not hidden in it.
        deadline = time.monotonic() + g["duration_seconds"]
        while len(crossed) < len(canaries) and time.monotonic() < deadline:
            names = ",".join(f"'{m}'" for m in canaries if m not in crossed)
            got = clickhouse(f"SELECT DISTINCT metric_name FROM observable.metric_points WHERE "
                             f"tenant_id = '{DEV_TENANT}' AND metric_name IN ({names}) FORMAT TSV")
            for m in got.split():
                crossed.setdefault(m, time.monotonic())
            time.sleep(1)
        for m in canaries:
            if m not in crossed:
                misses.append(f"{m}: canary point never became queryable")

        notified: dict[str, float] = {}
        rule_names = {f"w5c {run} {i}": canaries[i] for i in range(len(canaries))}
        wait_until = max(crossed.values(), default=time.monotonic()) + budget
        while time.monotonic() < wait_until and len(notified) < len(canaries):
            for e in sink_events(sink_file):
                m = rule_names.get(e.get("payload", {}).get("rule_name", ""))
                if m and m not in notified:
                    notified[m] = time.monotonic()
            time.sleep(0.5)

        viewer.join(g["duration_seconds"] + 120)
        stop.set()
        feeder.join(120)

        detections = {}
        for m in canaries:
            if m in crossed and m in notified:
                detections[m] = round(notified[m] - crossed[m], 2)
                if detections[m] > budget:
                    misses.append(f"{m}: notified {detections[m]}s after evidence, budget {budget}s")
        lost = len(canaries) - len(notified)
        if lost > g["max_notification_loss"]:
            misses.append(f"{lost} of {len(canaries)} canary notifications lost")
        quiet = [e for e in sink_events(sink_file)
                 if e.get("payload", {}).get("rule_name", "").startswith(f"w5c {run} ")
                 and e["payload"]["rule_name"] not in rule_names]
        if quiet:
            misses.append(f"{len(quiet)} notifications from rules that never crossed a threshold")

        achieved = stats.get("sent", 0) / max(stats.get("seconds", 1), 1)
        if achieved < g["spans_per_second"] * g["min_achieved_rate_ratio"]:
            misses.append(f"ingest achieved {achieved:.0f} spans/s, ceiling {g['spans_per_second']}")
        if stats.get("failed", 0):
            misses.append(f"{stats['failed']} spans refused during the exercise")
        per_view, view_misses = views.verdict(g["p95_seconds"], 0.0)
        misses += view_misses
        rule_count = psql(f"SELECT count(*) FROM alerting.alert_rules WHERE tenant_id = '{DEV_TENANT}' "
                          f"AND name LIKE 'w5c {run} %';").strip()
        report = {"figures": g, "rules_active": int(rule_count), "ingest": {**stats, "spans_per_second": round(achieved)},
                  "detection_seconds": detections, "notifications_lost": lost, "views": per_view}
    finally:
        cleanup_rules(run)
    return finish(args.report, "OBS-19-09", report, misses)


def finish(path: str | None, req: str, report: dict, misses: list[str]) -> int:
    report = {"requirement": req, "passed": not misses, "misses": misses, **report}
    text = json.dumps(report, indent=2, sort_keys=True)
    if path:
        Path(path).parent.mkdir(parents=True, exist_ok=True)
        Path(path).write_text(text + "\n")
    print(text)
    return 0 if not misses else 1


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("exercise", choices=["load", "growth"])
    parser.add_argument("--report")
    args = parser.parse_args(argv)
    return load(args) if args.exercise == "load" else growth(args)


if __name__ == "__main__":
    sys.exit(main())
