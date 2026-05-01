#!/usr/bin/env python3
"""
NLQ multi-model eval — run the NLQ test suite against multiple LLM models in sequence.

For each test case, every model is tried before moving to the next case so that
failures surface immediately alongside the other models' results for the same query.

Usage:
  python3 scripts/nlq-multi-model.py [options]

Options:
  --url URL           Base API URL                       (default: http://localhost:8080)
  --cases FILE        Path to test cases JSON            (default: tests/nlq/cases.json)
  --output-dir DIR    Directory for per-model reports    (default: tests/nlq/results)
  --tenant-id UUID    X-Tenant-ID header                 (default: dev tenant)
  --models M1,M2,...  Comma-separated list of models     (default: phi3:latest,phi3.5:latest,llama3.1:8b)
  --filter PATTERN    Only run cases matching PATTERN
  -v, --verbose       Print IR and SQL for every case
  --no-color          Disable ANSI colors

Prerequisites:
  The Observable API must be reachable at --url with a configured Ollama base URL.
"""

from __future__ import annotations

import argparse
import datetime
import importlib.util
import json
import os
import signal
import sys
import urllib.error
import urllib.request
from typing import Any

# ── Bootstrap: import helpers from nlq-eval.py ──────────────────────────────

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

_eval_spec = importlib.util.spec_from_file_location(
    "nlq_eval", os.path.join(REPO_ROOT, "scripts", "nlq-eval.py")
)
_eval_mod = importlib.util.module_from_spec(_eval_spec)  # type: ignore[arg-type]
_eval_spec.loader.exec_module(_eval_mod)  # type: ignore[union-attr]

call_nlq: Any = _eval_mod.call_nlq
eval_value_check: Any = _eval_mod.eval_value_check
Printer: Any = _eval_mod.Printer
DEFAULT_CASES: str = _eval_mod.DEFAULT_CASES
DEFAULT_TENANT_ID: str = _eval_mod.DEFAULT_TENANT_ID

# ── Constants ────────────────────────────────────────────────────────────────

DEFAULT_URL = "http://localhost:8080"
DEFAULT_MODELS = ["phi3:latest", "phi3.5:latest", "llama3.1:8b"]
DEFAULT_OUTPUT_DIR = os.path.join(REPO_ROOT, "tests", "nlq", "results")

# ANSI (mirrored from nlq-eval for the summary table)
GREEN = "\033[32m"
RED = "\033[31m"
YELLOW = "\033[33m"
CYAN = "\033[36m"
BOLD = "\033[1m"
RESET = "\033[0m"

# ── CLI ──────────────────────────────────────────────────────────────────────


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(
        description="NLQ multi-model eval — run NLQ test cases against multiple models."
    )
    p.add_argument("--url", default=DEFAULT_URL, help="Base API URL")
    p.add_argument("--cases", default=DEFAULT_CASES, help="Path to cases.json")
    p.add_argument("--output-dir", default=DEFAULT_OUTPUT_DIR, help="Directory for per-model reports")
    p.add_argument("--tenant-id", default=DEFAULT_TENANT_ID, help="X-Tenant-ID header")
    p.add_argument(
        "--models",
        default=",".join(DEFAULT_MODELS),
        help="Comma-separated list of model names (default: phi3:latest,phi3.5:latest,llama3.1:8b)",
    )
    p.add_argument("--filter", default=None, metavar="PATTERN", help="Only run matching cases")
    p.add_argument("-v", "--verbose", action="store_true", help="Print IR and SQL per case/model")
    p.add_argument("--no-color", action="store_true", help="Disable ANSI colors")
    return p.parse_args()


# ── Config API helpers ────────────────────────────────────────────────────────


def get_current_config(base_url: str, tenant_id: str) -> dict[str, Any]:
    """GET /v1/config — returns the current LLM configuration."""
    url = base_url.rstrip("/") + "/v1/config"
    req = urllib.request.Request(url, headers={"X-Tenant-ID": tenant_id})
    with urllib.request.urlopen(req, timeout=10) as resp:
        return json.loads(resp.read())


def set_model(base_url: str, tenant_id: str, model: str) -> None:
    """PUT /v1/config/llm — switch to the given model name."""
    url = base_url.rstrip("/") + "/v1/config/llm"
    body = json.dumps({"model": model}).encode()
    req = urllib.request.Request(
        url,
        data=body,
        headers={
            "Content-Type": "application/json",
            "X-Tenant-ID": tenant_id,
        },
        method="PUT",
    )
    with urllib.request.urlopen(req, timeout=10) as resp:
        resp.read()


# ── Single-case evaluation (no printing) ─────────────────────────────────────


def run_case(
    case: dict[str, Any],
    base_url: str,
    tenant_id: str,
) -> dict[str, Any]:
    """Run one test case and return a structured result record (no side effects)."""
    case_id = case["id"]
    query = case["query"]
    service_name = case.get("service_name")
    expect = case["expect"]

    record: dict[str, Any] = {
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

    try:
        resp = call_nlq(base_url, tenant_id, query, service_name)
    except Exception as exc:
        record["error"] = str(exc)
        return record

    resp_type = resp.get("type")
    record["response_type"] = resp_type

    if resp_type == "frame":
        frame = resp.get("frame", {})
        ir = frame.get("nlq_ir", {})
        record["actual_operation"] = ir.get("operation")
        record["raw_ir"] = ir
        record["source_sql"] = frame.get("source_sql")
        record["row_count"] = len(frame.get("data", []))
    elif resp_type == "invalid_response":
        record["raw_llm_response"] = resp.get("raw_llm_response")
        record["error"] = resp.get("reason", "invalid_response")

    checks: list[dict[str, Any]] = []

    # Check 1: response type
    expected_type = expect["type"]
    type_ok = resp_type == expected_type
    checks.append({"name": "response_type", "passed": type_ok,
                   "detail": f"expected={expected_type} actual={resp_type}"})

    # Check 2: IR operation
    expected_op = expect.get("operation")
    expected_op_any = expect.get("operation_any_of")
    if resp_type == "frame" and (expected_op is not None or expected_op_any is not None):
        actual_op = record["actual_operation"]
        if expected_op_any is not None:
            op_ok = actual_op in expected_op_any
            detail = f"expected one of {expected_op_any} actual={actual_op}"
        else:
            op_ok = actual_op == expected_op
            detail = f"expected={expected_op} actual={actual_op}"
        checks.append({"name": "ir_operation", "passed": op_ok, "detail": detail})
    elif (expected_op is not None or expected_op_any is not None) and resp_type != "frame":
        label = expected_op or f"one of {expected_op_any}"
        checks.append({"name": "ir_operation", "passed": False,
                       "detail": f"expected operation={label} but response type={resp_type} (not frame)"})

    # Check 3: data non-empty
    if resp_type == "frame" and expect.get("data_non_empty"):
        row_count = record["row_count"] or 0
        checks.append({"name": "data_non_empty", "passed": row_count > 0,
                       "detail": f"row_count={row_count}"})

    # Check 4: required fields
    if resp_type == "frame":
        frame = resp.get("frame", {})
        data = frame.get("data", [])
        first_row = data[0] if data else {}
        for field in expect.get("fields", []):
            field_ok = field in first_row
            checks.append({"name": f"field_present:{field}", "passed": field_ok,
                           "detail": f"field '{field}' {'found' if field_ok else 'missing'} in first row"})

    # Check 5: value assertions
    if resp_type == "frame":
        frame = resp.get("frame", {})
        data = frame.get("data", [])
        first_row = data[0] if data else {}
        for vc in expect.get("value_checks", []):
            ok, detail = eval_value_check(vc, first_row)
            checks.append({"name": f"value_check:{vc['field']}{vc['op']}{vc['value']}",
                           "passed": ok, "detail": detail})

    # Check 6: IR field checks
    if resp_type == "frame":
        raw_ir = record.get("raw_ir") or {}
        for ifc in expect.get("ir_field_checks", []):
            ir_field = ifc.get("field")
            op = ifc.get("op")
            expected_val = ifc.get("value")
            actual_val = raw_ir.get(ir_field)
            if op == "contains":
                if isinstance(actual_val, list):
                    ok = expected_val in actual_val
                    detail = f"ir.{ir_field} contains '{expected_val}': actual={actual_val}"
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
            checks.append({"name": f"ir_field:{ir_field}_{op}_{expected_val}",
                           "passed": ok, "detail": detail})

    record["checks"] = checks
    record["result"] = "pass" if checks and all(c["passed"] for c in checks) else "fail"
    if not checks and record["result"] == "error":
        pass  # keep as error

    return record


# ── Output helpers ────────────────────────────────────────────────────────────


def model_slug(model: str) -> str:
    """Convert 'phi3.5:latest' → 'phi3.5_latest' for filesystem use."""
    return model.replace(":", "_").replace("/", "_")


def failing_detail(record: dict[str, Any]) -> str:
    if record.get("error") and not record.get("checks"):
        return record["error"][:80]
    bad = [c for c in record.get("checks", []) if not c["passed"]]
    if not bad:
        return ""
    return bad[0]["name"] + ": " + bad[0]["detail"]


# ── Main ──────────────────────────────────────────────────────────────────────


def main() -> int:
    args = parse_args()
    use_color = not args.no_color and sys.stdout.isatty()

    def c(code: str, text: str) -> str:
        return f"{code}{text}{RESET}" if use_color else text

    models = [m.strip() for m in args.models.split(",") if m.strip()]
    if not models:
        print("ERROR: --models list is empty", file=sys.stderr)
        return 1

    # Load cases
    if not os.path.exists(args.cases):
        print(f"ERROR: cases file not found: {args.cases}", file=sys.stderr)
        return 1
    with open(args.cases) as f:
        cases: list[dict[str, Any]] = json.load(f)

    if args.filter:
        pat = args.filter.lower()
        cases = [
            c_ for c_ in cases
            if pat in c_["id"].lower()
            or pat in c_.get("description", "").lower()
            or any(pat in t.lower() for t in c_.get("tags", []))
        ]
        if not cases:
            print(f"WARNING: no cases matched filter '{args.filter}'", file=sys.stderr)
            return 0

    # Read and save the original model so we can restore it after the run
    original_model: str | None = None
    try:
        cfg = get_current_config(args.url, args.tenant_id)
        original_model = cfg.get("llm_model")
    except Exception as exc:
        print(c(YELLOW, f"⚠ Could not read current config ({exc}); original model will not be restored."))

    def restore_model() -> None:
        if original_model:
            try:
                set_model(args.url, args.tenant_id, original_model)
            except Exception:
                pass  # best-effort

    def handle_signal(sig: int, _frame: Any) -> None:
        print(c(YELLOW, "\n⚠ Interrupted — restoring original model…"))
        restore_model()
        sys.exit(1)

    signal.signal(signal.SIGINT, handle_signal)
    signal.signal(signal.SIGTERM, handle_signal)

    # per_model_results[model] = list of result records (one per case)
    per_model_results: dict[str, list[dict[str, Any]]] = {m: [] for m in models}

    print(c(BOLD, f"\nNLQ Multi-Model Eval — {args.url}"))
    print(f"Cases: {len(cases)}   Models: {', '.join(models)}\n")

    # Main loop: run all cases for each model before moving to the next
    for model in models:
        print(c(BOLD, f"── {model} {'─' * max(0, 50 - len(model))}"))

        # Switch to this model
        try:
            set_model(args.url, args.tenant_id, model)
        except Exception as exc:
            print(c(RED, f"  ✗ Could not switch to {model}: {exc}"))
            for case in cases:
                per_model_results[model].append({
                    "id": case["id"], "query": case["query"], "result": "error",
                    "checks": [], "response_type": None, "actual_operation": None,
                    "row_count": None, "raw_ir": None, "source_sql": None,
                    "raw_llm_response": None,
                    "error": f"model switch failed: {exc}",
                })
            continue

        for case in cases:
            record = run_case(case, args.url, args.tenant_id)
            per_model_results[model].append(record)

            all_passed = record["result"] == "pass"
            status = c(GREEN, "PASS") if all_passed else (c(RED, "FAIL") if record["result"] == "fail" else c(YELLOW, "ERR "))
            print(f"  {status}  {c(BOLD, case['id'])}")

            if not all_passed:
                bad = [ch for ch in record.get("checks", []) if not ch["passed"]]
                for ch in bad:
                    print(f"       {c(RED, '✗')} {ch['name']}: {ch['detail']}")
                if record.get("error") and not record.get("checks"):
                    print(f"       {c(YELLOW, record['error'][:120])}")

            if args.verbose and record.get("raw_ir"):
                sql = record.get("source_sql", "")
                print(f"       {c(CYAN, 'IR:')} {json.dumps(record['raw_ir'])}")
                print(f"       {c(CYAN, 'SQL:')} {sql[:200]}{'…' if len(sql) > 200 else ''}")

        # Per-model mini-summary
        records = per_model_results[model]
        passed = sum(1 for r in records if r["result"] == "pass")
        failed = sum(1 for r in records if r["result"] == "fail")
        errored = sum(1 for r in records if r["result"] == "error")
        print(f"\n  {c(GREEN, str(passed))} passed   {c(RED, str(failed)) if failed else str(failed)} failed   {c(YELLOW, str(errored)) if errored else str(errored)} errored   ({len(records)} total)\n")

    # Restore original model
    print(c(YELLOW, f"Restoring model → {original_model or '(unknown)'}…"))
    restore_model()

    # ── Summary table ────────────────────────────────────────────────────────

    print(c(BOLD, "\n── Summary ──────────────────────────────"))
    summary_row = f"  {'Model':<30}  {'Pass':>5}  {'Fail':>5}  {'Err':>5}  {'Total':>6}"
    print(c(BOLD, summary_row))
    print("  " + "─" * 52)

    model_summaries: dict[str, dict[str, int]] = {}
    any_failure = False

    for model in models:
        records = per_model_results[model]
        passed = sum(1 for r in records if r["result"] == "pass")
        failed = sum(1 for r in records if r["result"] == "fail")
        errored = sum(1 for r in records if r["result"] == "error")
        total = len(records)
        model_summaries[model] = {"passed": passed, "failed": failed, "errored": errored, "total": total}
        if failed or errored:
            any_failure = True
        # Build aligned columns with correct visible widths (color applied after padding)
        p_str = f"{GREEN}{passed:>5}{RESET}" if use_color else f"{passed:>5}"
        f_str = (f"{RED}{failed:>5}{RESET}" if use_color else f"{failed:>5}") if failed else f"{failed:>5}"
        e_str = (f"{YELLOW}{errored:>5}{RESET}" if use_color else f"{errored:>5}") if errored else f"{errored:>5}"
        print(f"  {model:<30}  {p_str}  {f_str}  {e_str}  {total:>6}")

    # ── Write per-model JSON reports ─────────────────────────────────────────

    os.makedirs(args.output_dir, exist_ok=True)

    for model in models:
        records = per_model_results[model]
        slug = model_slug(model)
        model_dir = os.path.join(args.output_dir, slug)
        os.makedirs(model_dir, exist_ok=True)

        s = model_summaries[model]
        report = {
            "run_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
            "url": args.url,
            "model": model,
            "tenant_id": args.tenant_id,
            "filter": args.filter,
            "summary": s,
            "cases": records,
        }
        out_path = os.path.join(model_dir, "last-run.json")
        with open(out_path, "w") as f:
            json.dump(report, f, indent=2)

    # ── Write cross-model summary JSON ────────────────────────────────────────

    summary_report = {
        "run_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "url": args.url,
        "models": models,
        "cases_count": len(cases),
        "filter": args.filter,
        "results": {
            model: {
                "summary": model_summaries[model],
                "failed_cases": [
                    {"id": r["id"], "detail": failing_detail(r)}
                    for r in per_model_results[model]
                    if r["result"] != "pass"
                ],
            }
            for model in models
        },
    }
    summary_path = os.path.join(args.output_dir, "summary.json")
    with open(summary_path, "w") as f:
        json.dump(summary_report, f, indent=2)

    print(f"\nReports written to: {args.output_dir}/")
    print(f"  summary.json + {'/'.join(model_slug(m) for m in models)}/last-run.json")

    return 1 if any_failure else 0


if __name__ == "__main__":
    sys.exit(main())
