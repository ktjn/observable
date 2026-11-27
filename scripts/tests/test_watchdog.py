#!/usr/bin/env python3
"""OBS-29-06: the out-of-band watchdog (scripts/watchdog.py) as separate processes.

The monitored stack, the watchdog and its notification route are three processes. Stopping
the monitored process must produce a loss notification on the watchdog's own route; the
compose drill (scripts/watchdog-drill.sh) does the same against the real stack.
"""

from __future__ import annotations

import json
import socket
import subprocess
import sys
import tempfile
import time
import unittest
import urllib.request
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
WATCHDOG = REPO / "scripts" / "watchdog.py"
sys.path.insert(0, str(REPO / "scripts"))
import watchdog  # noqa: E402


def free_port() -> int:
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def wait_http(url: str, timeout: float = 10.0) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            urllib.request.urlopen(url, timeout=1).close()
            return
        except OSError:
            time.sleep(0.1)
    raise AssertionError(f"{url} never came up")


class Processes:
    def __init__(self) -> None:
        self.procs: list[subprocess.Popen] = []

    def start(self, *args: str, **kw) -> subprocess.Popen:
        p = subprocess.Popen([sys.executable, *args], stdout=subprocess.DEVNULL,
                             stderr=subprocess.DEVNULL, **kw)
        self.procs.append(p)
        return p

    def stop_all(self) -> None:
        for p in self.procs:
            if p.poll() is None:
                p.terminate()
                try:
                    p.wait(5)
                except subprocess.TimeoutExpired:
                    p.kill()


def monitored(procs: Processes, root: Path, port: int) -> subprocess.Popen:
    (root / "health").write_text("ok")
    p = procs.start("-m", "http.server", str(port), "--bind", "127.0.0.1", "--directory", str(root))
    wait_http(f"http://127.0.0.1:{port}/health")
    return p


def received(path: Path) -> list[dict]:
    if not path.exists():
        return []
    return [json.loads(line)["payload"] for line in path.read_text().splitlines() if line]


def wait_for(fn, timeout: float, what: str):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        got = fn()
        if got:
            return got
        time.sleep(0.1)
    raise AssertionError(f"timed out waiting for {what}")


class WatchdogDrillTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = Path(tempfile.mkdtemp())
        self.procs = Processes()
        self.addCleanup(self.procs.stop_all)

    def test_loss_of_monitored_stack_is_detected_and_notified_out_of_band(self) -> None:
        stack_port, sink_port = free_port(), free_port()
        sink_file = self.tmp / "notifications.jsonl"
        stack = monitored(self.procs, self.tmp, stack_port)
        self.procs.start(str(WATCHDOG), "receive", "--port", str(sink_port), "--out", str(sink_file))
        notify_url = f"http://127.0.0.1:{sink_port}/"
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            try:
                socket.create_connection(("127.0.0.1", sink_port), 1).close()
                break
            except OSError:
                time.sleep(0.1)
        interval, failures = 0.2, 3
        self.procs.start(
            str(WATCHDOG), "watch",
            "--target", f"query-api=http://127.0.0.1:{stack_port}/health",
            "--notify-url", notify_url,
            "--interval", str(interval), "--failures", str(failures),
            "--timeout", "0.5", "--observer", "drill-observer",
        )

        # Healthy: nothing is notified.
        time.sleep(interval * (failures + 2))
        self.assertEqual(received(sink_file), [])

        # Lose the monitored stack: the watchdog's own route is told, within its detection budget.
        lost_at = time.monotonic()
        stack.terminate()
        stack.wait(5)
        got = wait_for(lambda: [n for n in received(sink_file) if n["event"] == "monitored_target_lost"],
                       timeout=interval * failures + 5, what="loss notification")
        elapsed = time.monotonic() - lost_at
        self.assertEqual(len(got), 1, "one notification per loss")
        self.assertEqual(got[0]["target"], "query-api")
        self.assertEqual(got[0]["source"], "observable-watchdog")
        self.assertEqual(got[0]["observer"], "drill-observer")
        self.assertGreaterEqual(got[0]["consecutive_failures"], failures)
        self.assertLess(elapsed, interval * failures + 5)

        # Still down: no repeat notifications.
        time.sleep(interval * (failures + 2))
        self.assertEqual(len([n for n in received(sink_file) if n["event"] == "monitored_target_lost"]), 1)

        # Recovery is reported through the same route.
        monitored(self.procs, self.tmp, stack_port)
        wait_for(lambda: [n for n in received(sink_file) if n["event"] == "monitored_target_recovered"],
                 timeout=5, what="recovery notification")

    def test_a_notify_route_on_the_monitored_endpoint_is_refused(self) -> None:
        port = free_port()
        same = f"http://127.0.0.1:{port}"
        p = subprocess.run(
            [sys.executable, str(WATCHDOG), "watch", "--target", f"alerting={same}/health",
             "--notify-url", f"{same}/notify"],
            capture_output=True, text=True, timeout=10,
        )
        self.assertEqual(p.returncode, 2, p.stderr)
        self.assertIn("shares an endpoint", p.stderr)
        for bad in ("file:///etc/passwd", "ftp://example.com/x"):
            with self.assertRaises(ValueError):
                watchdog.endpoint(bad)

    def test_state_machine_reports_each_transition_once(self) -> None:
        t = watchdog.Target("svc", "http://127.0.0.1:1/health")
        self.assertIsNone(watchdog.step(t, None, 2))
        self.assertIsNone(watchdog.step(t, "refused", 2))
        self.assertEqual(watchdog.step(t, "refused", 2), "lost")
        self.assertIsNone(watchdog.step(t, "refused", 2))
        self.assertEqual(watchdog.step(t, None, 2), "recovered")
        self.assertIsNone(watchdog.step(t, None, 2))


if __name__ == "__main__":
    unittest.main()
