#!/usr/bin/env python3
"""Run the CI policy regression tests (scripts/tests/test_*.py) in parallel.

Each test module runs in its own `python -m unittest <module>` process, so the
suite is the same set `unittest discover -s scripts/tests -p "test_*.py"`
collects; only the scheduling differs. Exit status is non-zero when any module
fails or reports no tests.

Environment:
  LOCAL_CI_JOBS  maximum concurrent modules (default: CPU count)
"""

from concurrent.futures import ThreadPoolExecutor
import os
from pathlib import Path
import re
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
TEST_DIR = ROOT / "scripts" / "tests"
RAN = re.compile(r"^Ran (\d+) tests? in", re.MULTILINE)


def run(module):
    env = dict(os.environ)
    env["PYTHONPATH"] = os.pathsep.join(filter(None, [env.get("PYTHONPATH"), str(TEST_DIR)]))
    started = time.monotonic()
    result = subprocess.run(
        [sys.executable, "-m", "unittest", module],
        cwd=ROOT, env=env, capture_output=True, text=True,
    )
    match = RAN.search(result.stderr)
    count = int(match[1]) if match else 0
    return module, result.returncode, count, time.monotonic() - started, result.stdout + result.stderr


def main():
    modules = sorted(path.stem for path in TEST_DIR.glob("test_*.py"))
    if not modules:
        print(f"no test modules found in {TEST_DIR}", file=sys.stderr)
        return 1
    jobs = int(os.environ.get("LOCAL_CI_JOBS") or os.cpu_count() or 1)
    started = time.monotonic()
    with ThreadPoolExecutor(max_workers=max(1, jobs)) as pool:
        results = list(pool.map(run, modules))

    failed = [r for r in results if r[1] != 0 or r[2] == 0]
    for module, status, count, elapsed, output in failed:
        print(f"\n===== {module} (exit {status}, {count} tests, {elapsed:.1f}s) =====")
        print(output.rstrip())
    total = sum(r[2] for r in results)
    print(
        f"Ran {total} tests from {len(modules)} modules in {time.monotonic() - started:.1f}s "
        f"({jobs} jobs): {'FAILED' if failed else 'OK'}"
    )
    if failed:
        print("Failed modules: " + ", ".join(r[0] for r in failed))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
