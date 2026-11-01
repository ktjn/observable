# Components Example: Standalone Components with Their Own Tests

[`spec.yaml`](spec.yaml) is the commerce example with physical bindings, split into fragments ([`spec/checkout.yaml`](spec/checkout.yaml), [`spec/inventory.yaml`](spec/inventory.yaml), [`spec/system.yaml`](spec/system.yaml)) merged by `include`, so each component's agent edits only its own file, so each component can be built and verified from its own slice:

- `source_paths` / `test_paths` per component (`src/<id>`, `tests/<id>`); `schema_path` on `stock-item`.
- Test evidence names a file under the owning component's `test_paths` (`tests/checkout/test_checkout.py::test_place_order`), so `COMP-08` holds.
- `checkout` imports `inventory` only because it declares `requires: [reserve-inventory@^1.0]`; its tests use `tests/checkout/fakes/reserve_inventory.py` (declared in `fakes`, `COMP-09`), never inventory code.

```bash
# from reference-implementation/
uv run python metamodel.py ../examples/components/spec.yaml                                         # metadata + COMP-08
uv run python metamodel.py ../examples/components/spec.yaml --check-artifacts --root ../examples/components
uv run python metamodel.py ../examples/components/spec.yaml --check-drift --root ../examples/components
uv run python metamodel.py ../examples/components/spec.yaml --context checkout                      # the slice an agent gets
```

Source and tests live in this directory so `--root` points here. Illustrative evidence values (dates, verifiers) were not produced by running a pipeline.

## Break it on purpose

- Add `import src.checkout.app` to `src/inventory/api.py` → `--check-drift` reports `inventory` imports `checkout` with no declared requirement.
- Change `tests/checkout/test_checkout.py::test_place_order` evidence provenance to `tests/inventory/x.py::t` → `COMP-08`.
- Add a `discount` property to `schemas/stock-item.json` → `--check-drift` reports a property undeclared in the model.

- Import `src.inventory.api` in `tests/checkout/test_checkout.py` → `--check-drift` rejects tests importing another component.
- Remove the `fakes` entry → `COMP-09`.
- Remove `req-reserve-contract` (or its evidence) → `COMP-10`: the fake models behavior nobody tests at the provider.

Limits: Python imports and JSON Schema property names only; fake fidelity to the contract is not verified.
