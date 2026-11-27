#!/usr/bin/env python3
"""Out-of-band watchdog for the monitored Observable stack (OBS-29-06, assurance-relocations E4).

Runs as a separate observer deployment. It probes the monitored stack's health endpoints and,
when a target stays unreachable, notifies through its own route. It shares nothing with the
platform it watches: no platform alerting, no platform storage, no platform credentials.

  watchdog.py watch --target NAME=URL [--target ...] --notify-url URL [--interval S] [--failures N]
  watchdog.py receive --port P --out FILE      # a notification sink for drills

Events are written to stdout as JSON lines. Exit codes: 0 normal stop, 2 configuration refused.
"""

from __future__ import annotations

import argparse
import json
import signal
import socket
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

MAX_NOTIFY_BODY = 64 * 1024


def now() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="milliseconds").replace("+00:00", "Z")


def emit(event: dict) -> None:
    print(json.dumps(event, sort_keys=True), flush=True)


def endpoint(url: str) -> tuple[str, int]:
    parsed = urllib.parse.urlsplit(url)
    if parsed.scheme not in ("http", "https") or not parsed.hostname:
        raise ValueError(f"not an http(s) URL: {url}")
    port = parsed.port or (443 if parsed.scheme == "https" else 80)
    return parsed.hostname.lower(), port


def resolved(host: str) -> set[str]:
    try:
        return {info[4][0] for info in socket.getaddrinfo(host, None)}
    except socket.gaierror:
        return {host}


def refuse_shared_route(targets: dict[str, str], notify_url: str) -> None:
    """The notification route must not be one of the endpoints it reports on."""
    n_host, n_port = endpoint(notify_url)
    n_addrs = resolved(n_host)
    for name, url in targets.items():
        t_host, t_port = endpoint(url)
        if t_port == n_port and (t_host == n_host or resolved(t_host) & n_addrs):
            raise ValueError(f"notify route {notify_url} shares an endpoint with target {name}")


@dataclass
class Target:
    name: str
    url: str
    failures: int = 0
    down: bool = False
    down_since: str | None = None
    last_error: str | None = None


def probe(url: str, timeout: float) -> str | None:
    """None when healthy, otherwise the reason."""
    try:
        with urllib.request.urlopen(url, timeout=timeout) as resp:
            if 200 <= resp.status < 300:
                return None
            return f"status {resp.status}"
    except urllib.error.HTTPError as e:
        return f"status {e.code}"
    except (urllib.error.URLError, OSError, TimeoutError) as e:
        return type(e).__name__ + ": " + str(getattr(e, "reason", e))


def notify(url: str, payload: dict, attempts: int, timeout: float) -> bool:
    body = json.dumps(payload).encode()
    for attempt in range(attempts):
        req = urllib.request.Request(
            url, data=body, method="POST", headers={"content-type": "application/json"}
        )
        try:
            with urllib.request.urlopen(req, timeout=timeout) as resp:
                if 200 <= resp.status < 300:
                    return True
        except (urllib.error.URLError, OSError, TimeoutError):
            pass
        time.sleep(min(2**attempt, 10))
    return False


def step(t: Target, error: str | None, threshold: int) -> str | None:
    """Advances one target's state; returns 'lost' or 'recovered' on a transition."""
    if error is None:
        t.failures, t.last_error = 0, None
        if t.down:
            t.down, t.down_since = False, None
            return "recovered"
        return None
    t.failures += 1
    t.last_error = error
    if t.failures == 1:
        t.down_since = now()
    if not t.down and t.failures >= threshold:
        t.down = True
        return "lost"
    return None


def watch(args: argparse.Namespace) -> int:
    targets: dict[str, str] = {}
    for spec in args.target:
        name, sep, url = spec.partition("=")
        if not sep or not name or name in targets:
            print(f"watchdog: bad or duplicate --target {spec!r}", file=sys.stderr)
            return 2
        targets[name] = url
    try:
        for url in [*targets.values(), args.notify_url]:
            endpoint(url)
        refuse_shared_route(targets, args.notify_url)
    except ValueError as e:
        print(f"watchdog: refused: {e}", file=sys.stderr)
        return 2
    if args.failures < 1 or args.interval <= 0:
        print("watchdog: refused: --failures >= 1 and --interval > 0", file=sys.stderr)
        return 2

    state = [Target(n, u) for n, u in targets.items()]
    stop = False

    def halt(*_: object) -> None:
        nonlocal stop
        stop = True

    signal.signal(signal.SIGTERM, halt)
    signal.signal(signal.SIGINT, halt)
    emit({"event": "watchdog_started", "at": now(), "targets": sorted(targets),
          "interval_s": args.interval, "failures": args.failures})
    while not stop:
        started = time.monotonic()
        for t in state:
            error = probe(t.url, args.timeout)
            emit({"event": "probe", "at": now(), "target": t.name, "ok": error is None,
                  **({"error": error} if error else {})})
            change = step(t, error, args.failures)
            if change:
                payload = {
                    "event": f"monitored_target_{change}",
                    "source": "observable-watchdog",
                    "observer": args.observer,
                    "target": t.name,
                    "url": t.url,
                    "at": now(),
                    **({"down_since": t.down_since, "consecutive_failures": t.failures,
                        "last_error": t.last_error} if change == "lost" else {}),
                }
                delivered = notify(args.notify_url, payload, args.notify_attempts, args.timeout)
                emit({**payload, "notified": delivered})
        time.sleep(max(0.0, args.interval - (time.monotonic() - started)))
    emit({"event": "watchdog_stopped", "at": now()})
    return 0


def receive(args: argparse.Namespace) -> int:
    out = open(args.out, "a", encoding="utf-8")

    class Sink(BaseHTTPRequestHandler):
        def do_POST(self) -> None:  # noqa: N802
            length = int(self.headers.get("content-length") or 0)
            if length <= 0 or length > MAX_NOTIFY_BODY:
                self.send_response(413 if length > MAX_NOTIFY_BODY else 400)
                self.end_headers()
                return
            try:
                payload = json.loads(self.rfile.read(length))
            except ValueError:
                self.send_response(400)
                self.end_headers()
                return
            out.write(json.dumps({"received_at": now(), "payload": payload}) + "\n")
            out.flush()
            self.send_response(202)
            self.end_headers()

        def log_message(self, *_: object) -> None:
            pass

    server = ThreadingHTTPServer((args.bind, args.port), Sink)
    signal.signal(signal.SIGTERM, lambda *_: sys.exit(0))
    emit({"event": "receiver_started", "at": now(), "port": server.server_address[1]})
    try:
        server.serve_forever()
    finally:
        out.close()
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="cmd", required=True)
    w = sub.add_parser("watch")
    w.add_argument("--target", action="append", required=True, metavar="NAME=URL")
    w.add_argument("--notify-url", required=True)
    w.add_argument("--interval", type=float, default=10.0)
    w.add_argument("--failures", type=int, default=3)
    w.add_argument("--timeout", type=float, default=3.0)
    w.add_argument("--notify-attempts", type=int, default=5)
    w.add_argument("--observer", default=socket.gethostname())
    r = sub.add_parser("receive")
    r.add_argument("--port", type=int, required=True)
    r.add_argument("--bind", default="127.0.0.1")
    r.add_argument("--out", required=True)
    args = parser.parse_args(argv)
    return watch(args) if args.cmd == "watch" else receive(args)


if __name__ == "__main__":
    sys.exit(main())
