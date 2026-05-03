#!/usr/bin/env python3
"""
NLQ Eval Harness — measure NLQ accuracy and drive prompt engineering iterations.

Runs a battery of NLQ queries against the Observable API and checks whether:
  - The correct response type is returned (frame / decline / capabilities / invalid_response)
  - The LLM chose the expected IR operation
  - The result data is non-empty when expected
  - Required fields appear in result rows
  - Numeric value assertions hold (e.g. p95 > 0)

Output:
  Terminal: colored PASS/FAIL summary per case
  File:     tests/nlq/last-run.json — structured report for Copilot analysis

Feedback loop:
  After a failed run read the report with:
    cat tests/nlq/last-run.json | python3 -m json.tool | grep -A30 '"result": "fail"'
  Then ask Copilot to analyze it and propose system prompt fixes.

Usage:
  python3 scripts/nlq-eval.py [options]

Options:
  --url URL         Base URL for the Observable API  (default: http://localhost:8080)
  --cases FILE      Path to test cases JSON           (default: tests/nlq/cases.json)
  --output FILE     Path for results JSON             (default: tests/nlq/last-run.json)
  --tenant-id UUID  Tenant ID header value            (default: dev tenant)
  --filter PATTERN  Only run cases whose id/tags contain PATTERN
  -v, --verbose     Print IR and SQL for every case
  --no-color        Disable ANSI color output

Prerequisites:
  The Observable API must be reachable at --url.
  With the kind testbench: start with 'bash scripts/testbench.sh --keep-cluster'
  then access at http://localhost:8080.
"""

from __future__ import annotations

import argparse
import datetime
import json
import os
import sys
import urllib.error
import urllib.request
from typing import Any

# ── Constants ────────────────────────────────────────────────────────────────

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DEFAULT_CASES = os.path.join(REPO_ROOT, "tests", "nlq", "cases.json")
DEFAULT_OUTPUT = os.path.join(REPO_ROOT, "tests", "nlq", "last-run.json")
DEFAULT_TENANT_ID = "00000000-0000-0000-0000-000000000001"
REQUEST_TIMEOUT_S = 120  # LLM inference can be slow on CPU

# ANSI colors
GREEN = "\033[32m"
RED = "\033[31m"
YELLOW = "\033[33m"
CYAN = "\033[36m"
BOLD = "\033[1m"
RESET = "\033[0m"

# ── CLI ──────────────────────────────────────────────────────────────────────


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(
        description="NLQ eval harness — checks NLQ query accuracy against the live API."
    )
    p.add_argument("--url", default="http://localhost:8080", help="Base API URL")
    p.add_argument("--cases", default=DEFAULT_CASES, help="Path to cases.json")
    p.add_argument("--output", default=DEFAULT_OUTPUT, help="Path for last-run.json")
    p.add_argument("--tenant-id", default=DEFAULT_TENANT_ID, help="X-Tenant-ID header")
    p.add_argument(
        "--filter",
        default=None,
        metavar="PATTERN",
        help="Only run cases whose id, tags, or description contain PATTERN",
    )
    p.add_argument("-v", "--verbose", action="store_true", help="Print IR and SQL per case")
    p.add_argument("--no-color", action="store_true", help="Disable ANSI colors")
    return p.parse_args()


# ── Color helpers ─────────────────────────────────────────────────────────────


class Printer:
    def __init__(self, color: bool) -> None:
        self.color = color and sys.stdout.isatty()

    def _c(self, code: str, text: str) -> str:
        return f"{code}{text}{RESET}" if self.color else text

    def green(self, t: str) -> str:
        return self._c(GREEN, t)

    def red(self, t: str) -> str:
        return self._c(RED, t)

    def yellow(self, t: str) -> str:
        return self._c(YELLOW, t)

    def cyan(self, t: str) -> str:
        return self._c(CYAN, t)

    def bold(self, t: str) -> str:
        return self._c(BOLD, t)


# ── API client ────────────────────────────────────────────────────────────────


def call_nlq(base_url: str, tenant_id: str, question: str | None, service_name: str | None, base_ir: dict | None = None, mode: str = "execute") -> dict[str, Any]:
    url = base_url.rstrip("/") + "/v1/nlq"
    body: dict[str, Any] = {}
    if question:
        body["question"] = question
    if service_name:
        body["service_name"] = service_name
    if base_ir:
        body["base_ir"] = base_ir
    if mode != "execute":
        body["mode"] = mode
    data = json.dumps(body).encode()
    req = urllib.request.Request(
        url,
        data=data,
        headers={
            "Content-Type": "application/json",
            "X-Tenant-ID": tenant_id,
        },
        method="POST",
    )
    try:
        with urllib.request.urlopen(req, timeout=REQUEST_TIMEOUT_S) as resp:
            return json.loads(resp.read())
    except urllib.error.HTTPError as e:
        body_text = e.read().decode(errors="replace")
        if e.code == 503:
            raise RuntimeError(
                f"HTTP 503 — NLQ service not configured. "
                f"Configure an LLM endpoint on the Setup page: {body_text[:120]}"
            ) from e
        raise RuntimeError(f"HTTP {e.code}: {body_text[:200]}") from e


# ── Value assertion evaluation ────────────────────────────────────────────────


def eval_value_check(check: dict[str, Any], row: dict[str, Any]) -> tuple[bool, str]:
    """Evaluate a single value_check assertion against a data row.
    Returns (passed, detail_message)."""
    field = check["field"]
    op = check["op"]
    expected = check["value"]

    if field not in row:
        return False, f"field '{field}' not in row"

    actual = row[field]

    try:
        if op == ">":
            ok = float(actual) > float(expected)
        elif op == ">=":
            ok = float(actual) >= float(expected)
        elif op == "<":
            ok = float(actual) < float(expected)
        elif op == "<=":
            ok = float(actual) <= float(expected)
        elif op == "=":
            ok = str(actual) == str(expected)
        elif op == "in":
            ok = actual in expected
        elif op == "contains":
            ok = str(expected) in str(actual)
        else:
            return False, f"unknown op '{op}'"
    except (TypeError, ValueError) as exc:
        return False, f"comparison error: {exc}"

    detail = f"{field}={actual!r} {op} {expected!r}"
    return ok, detail


# ── Single case evaluation ────────────────────────────────────────────────────


def evaluate_case(
    case: dict[str, Any],
    base_url: str,
    tenant_id: str,
    verbose: bool,
    printer: Printer,
) -> dict[str, Any]:
    case_id = case["id"]
    query = case["query"]
    service_name = case.get("service_name")
    base_ir = case.get("base_ir")
    mode = case.get("mode", "execute")
    expect = case["expect"]

    result_record: dict[str, Any] = {
        "id": case_id,
        "query": query,
        "result": "error",
        "checks": [],
        "response_type": None,
        "actual_operation": None,
        "row_count": None,
        "raw_ir": None,
        "source_sql": None,
        "raw_llm_response": None,
        "error": None,
    }

    # --- Call API ---
    try:
        resp = call_nlq(base_url, tenant_id, query, service_name, base_ir=base_ir, mode=mode)
    except Exception as exc:
        result_record["error"] = str(exc)
        print(f"  {printer.red('ERR')}  {case_id}: {exc}")
        return result_record

    resp_type = resp.get("type")
    result_record["response_type"] = resp_type

    # Populate extra fields from frame or ir response
    if resp_type == "frame":
        frame = resp.get("frame", {})
        ir = frame.get("nlq_ir", {})
        result_record["actual_operation"] = ir.get("operation")
        result_record["raw_ir"] = ir
        result_record["source_sql"] = frame.get("source_sql")
        result_record["row_count"] = len(frame.get("data", []))
    elif resp_type == "ir":
        ir = resp.get("ir", {})
        result_record["actual_operation"] = ir.get("operation")
        result_record["raw_ir"] = ir
    elif resp_type == "invalid_response":
        result_record["raw_llm_response"] = resp.get("raw_llm_response")

    checks: list[dict[str, Any]] = []

    # Check 1: response type
    expected_type = expect["type"]
    type_ok = resp_type == expected_type
    checks.append({
        "name": "response_type",
        "passed": type_ok,
        "detail": f"expected={expected_type} actual={resp_type}",
    })

    # Check 2: IR operation (for frame or ir responses)
    # Supports both "operation": "catalog" (exact) and "operation_any_of": ["histogram","distribution"] (set)
    expected_op = expect.get("operation")
    expected_op_any = expect.get("operation_any_of")
    if resp_type in ("frame", "ir") and (expected_op is not None or expected_op_any is not None):
        actual_op = result_record["actual_operation"]
        if expected_op_any is not None:
            op_ok = actual_op in expected_op_any
            detail = f"expected one of {expected_op_any} actual={actual_op}"
        else:
            op_ok = actual_op == expected_op
            detail = f"expected={expected_op} actual={actual_op}"
        checks.append({
            "name": "ir_operation",
            "passed": op_ok,
            "detail": detail,
        })
    elif (expected_op is not None or expected_op_any is not None) and resp_type not in ("frame", "ir"):
        # Can't check operation if not a frame or ir
        label = expected_op or f"one of {expected_op_any}"
        checks.append({
            "name": "ir_operation",
            "passed": False,
            "detail": f"expected operation={label} but response type={resp_type} (not frame or ir)",
        })

    # Check 3: data non-empty
    if resp_type == "frame" and expect.get("data_non_empty"):
        row_count = result_record["row_count"] or 0
        data_ok = row_count > 0
        checks.append({
            "name": "data_non_empty",
            "passed": data_ok,
            "detail": f"row_count={row_count}",
        })

    # Check 4: required fields
    if resp_type == "frame":
        frame = resp.get("frame", {})
        data = frame.get("data", [])
        first_row = data[0] if data else {}
        for field in expect.get("fields", []):
            field_ok = field in first_row
            checks.append({
                "name": f"field_present:{field}",
                "passed": field_ok,
                "detail": f"field '{field}' {'found' if field_ok else 'missing'} in first row",
            })

    # Check 5: value assertions
    if resp_type == "frame":
        frame = resp.get("frame", {})
        data = frame.get("data", [])
        first_row = data[0] if data else {}
        for vc in expect.get("value_checks", []):
            ok, detail = eval_value_check(vc, first_row)
            checks.append({
                "name": f"value_check:{vc['field']}{vc['op']}{vc['value']}",
                "passed": ok,
                "detail": detail,
            })

    # Check 6: IR field checks (validate fields in the raw IR, e.g. group_by)
    # Format: [{"field": "group_by", "op": "contains", "value": "service_name"}]
    if resp_type in ("frame", "ir"):
        raw_ir = result_record.get("raw_ir") or {}
        for ifc in expect.get("ir_field_checks", []):
            ir_field = ifc.get("field")
            op = ifc.get("op")
            expected_val = ifc.get("value")
            actual_val = raw_ir.get(ir_field)
            if op == "contains":
                if isinstance(actual_val, list):
                    ok = expected_val in actual_val
                    detail = f"ir.{ir_field} {op} '{expected_val}': actual={actual_val}"
                elif isinstance(actual_val, str):
                    ok = expected_val in actual_val
                    detail = f"ir.{ir_field} contains '{expected_val}': actual='{actual_val}'"
                else:
                    ok = False
                    detail = f"ir.{ir_field} is {type(actual_val).__name__}, expected list/str"
            elif op == "=":
                ok = actual_val == expected_val
                detail = f"ir.{ir_field}={expected_val}: actual={actual_val}"
            elif op == "not_empty":
                ok = bool(actual_val)
                detail = f"ir.{ir_field} not_empty: actual={actual_val}"
            else:
                ok = False
                detail = f"unknown ir_field_check op '{op}'"
            checks.append({
                "name": f"ir_field:{ir_field}_{op}_{expected_val}",
                "passed": ok,
                "detail": detail,
            })

        for fc in expect.get("ir_filter_checks", []):
            expected_field = fc.get("field")
            expected_op = fc.get("op", "=")
            expected_value = fc.get("value")
            filters = raw_ir.get("filters") or []
            if not isinstance(filters, list):
                ok = False
                detail = f"ir.filters is {type(filters).__name__}, expected list"
            else:
                ok = any(
                    isinstance(f, dict)
                    and f.get("field") == expected_field
                    and f.get("op") == expected_op
                    and f.get("value") == expected_value
                    for f in filters
                )
                detail = (
                    f"expected filter field={expected_field!r} op={expected_op!r} "
                    f"value={expected_value!r}: actual={filters}"
                )
            checks.append({
                "name": f"ir_filter:{expected_field}_{expected_op}_{expected_value}",
                "passed": ok,
                "detail": detail,
            })

    result_record["checks"] = checks
    all_passed = all(c["passed"] for c in checks)
    result_record["result"] = "pass" if all_passed else "fail"

    # --- Print per-case result ---
    status = printer.green("PASS") if all_passed else printer.red("FAIL")
    print(f"  {status}  {printer.bold(case_id)}")

    if not all_passed or verbose:
        for c in checks:
            mark = "  ✓" if c["passed"] else "  ✗"
            color = printer.green if c["passed"] else printer.red
            print(f"       {color(mark)} {c['name']}: {c['detail']}")

    if verbose and resp_type == "frame":
        frame = resp.get("frame", {})
        ir = frame.get("nlq_ir", {})
        sql = frame.get("source_sql", "")
        print(f"       {printer.cyan('IR:')} {json.dumps(ir)}")
        print(f"       {printer.cyan('SQL:')} {sql[:200]}{'…' if len(sql) > 200 else ''}")

    if verbose and resp_type == "ir":
        ir = resp.get("ir", {})
        print(f"       {printer.cyan('IR:')} {json.dumps(ir)}")

    if resp_type == "invalid_response" and not all_passed:
        reason = resp.get("reason", "")
        raw = resp.get("raw_llm_response", "")
        print(f"       {printer.yellow('reason:')} {reason}")
        print(f"       {printer.yellow('raw_llm:')} {raw[:120]}{'…' if len(raw) > 120 else ''}")

    return result_record


# ── Main ──────────────────────────────────────────────────────────────────────


def main() -> int:
    args = parse_args()
    printer = Printer(color=not args.no_color)

    # Load cases
    if not os.path.exists(args.cases):
        print(f"ERROR: cases file not found: {args.cases}", file=sys.stderr)
        return 1

    with open(args.cases) as f:
        cases: list[dict[str, Any]] = json.load(f)

    # Apply filter
    if args.filter:
        pat = args.filter.lower()
        cases = [
            c for c in cases
            if pat in c["id"].lower()
            or pat in c.get("description", "").lower()
            or any(pat in t.lower() for t in c.get("tags", []))
        ]
        if not cases:
            print(f"WARNING: no cases matched filter '{args.filter}'", file=sys.stderr)
            return 0

    # Verify API reachability (non-NLQ endpoint)
    print(printer.bold(f"\nNLQ Eval — {args.url}"))
    print(f"Cases: {args.cases} ({len(cases)} selected)")
    print(f"Tenant: {args.tenant_id}")
    print()

    test_url = args.url.rstrip("/") + "/v1/environments"
    try:
        req = urllib.request.Request(
            test_url,
            headers={"X-Tenant-ID": args.tenant_id},
        )
        with urllib.request.urlopen(req, timeout=10):
            pass
        print(printer.green("✓") + " API reachable\n")
    except Exception as exc:
        print(printer.yellow(f"⚠ API reachability check failed ({exc})"))
        print("  Continuing — NLQ endpoint may still be accessible.\n")

    # Run cases
    results: list[dict[str, Any]] = []
    passed = failed = errored = 0

    for case in cases:
        rec = evaluate_case(case, args.url, args.tenant_id, args.verbose, printer)
        results.append(rec)
        if rec["result"] == "pass":
            passed += 1
        elif rec["result"] == "fail":
            failed += 1
        else:
            errored += 1

    # Summary
    total = passed + failed + errored
    print()
    print(printer.bold("── Summary ──────────────────────────────"))
    print(f"  {printer.green(str(passed))} passed   {printer.red(str(failed))} failed   {printer.yellow(str(errored))} errored   ({total} total)")

    if failed or errored:
        print()
        print(printer.bold("── Failed cases ─────────────────────────"))
        for r in results:
            if r["result"] in ("fail", "error"):
                failing_checks = [c for c in r.get("checks", []) if not c["passed"]]
                details = "; ".join(c["name"] + ": " + c["detail"] for c in failing_checks[:3])
                if r["error"]:
                    details = r["error"]
                print(f"  {printer.red('✗')} {r['id']}: {details}")

        print()
        print("To diagnose failures, ask Copilot:")
        print(printer.cyan(f"  cat {args.output} | python3 -m json.tool | grep -A30 '\"result\": \"fail\"'"))

    # Write JSON report
    report = {
        "run_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "url": args.url,
        "tenant_id": args.tenant_id,
        "filter": args.filter,
        "summary": {
            "passed": passed,
            "failed": failed,
            "errored": errored,
            "total": total,
        },
        "cases": results,
    }
    os.makedirs(os.path.dirname(args.output), exist_ok=True)
    with open(args.output, "w") as f:
        json.dump(report, f, indent=2)
    print(f"\nReport written to: {args.output}")

    return 0 if (failed == 0 and errored == 0) else 1


if __name__ == "__main__":
    sys.exit(main())
