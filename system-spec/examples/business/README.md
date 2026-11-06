# Business-Level Specification Example

Applies the metamodel to organizational and process obligations, not software ([`MODEL-SCOPE.md`](../../MODEL-SCOPE.md)). One business area is one component; what holds an obligation is a capability of that component.

- **Area as component**: `incident-response` provides `incident-response.platform`, `.process` and `.organization`, one capability per holder of an obligation (tooling, procedure, people).
- **Requirements**: `statement` holds the obligation; `criteria` hold acceptance and negative checks. `priority` (`P0`, `P1`) is independent of `mandatory`; `owner` names the accountable role.
- **Supersession**: `ir-legacy-alerting` is `deprecated` and names `ir-detect` in `superseded_by` (`REQ-09`).
- **Phasing**: `ir-roster` and `ir-metrics` are P1 and optional. They are not made sub-requirements with `refines`, because `refines` means co-obligation (`REQ-08`); `ir-metrics` uses `depends_on`.
- **Categories**: `system.verification_categories` limits `REQ-01` to the four categories that fit a business spec. The validator lists the others as not required.
- **Risk**: `risk-missed-incident` is mitigated by two requirements.

Validate: `metamodel.py examples/business/spec.yaml --stage verification` (`VALID` plus a note listing the omitted categories). No evidence is supplied, so `--stage release` fails by design; evidence for business obligations is typically `inspection` or `analysis` records.
