# Distribution

This directory holds the Observable distribution artifacts that pin a tested,
compatible set of component versions. It is the monorepo-local precursor to the
`observable-distribution` component described in
[docs/component-decomposition.md](../docs/component-decomposition.md) and
[ADR-035](../spec/adr/ADR-035-component-independence.md); it moves to its own
repository once the components build and release independently.

## `manifest.yaml`

The distribution manifest pins one version per target component plus the
product version. `observable-distribution` consumes it to install a pinned
component set rather than tracking a single source commit.

It is the authoritative source of per-component versions:

- `productVersion` must equal the root `VERSION`.
- Each component version must equal its own package version: the Cargo
  `package.version` of its service crate, or `apps/frontend/package.json` for
  `web`. `contracts` has no single owning package yet and is manifest-only.
- Component versions may diverge from `productVersion`.

To release a component independently, bump its manifest entry and its package
version in the same pull request. Editing only one of them fails validation.

```bash
bash scripts/check-distribution-manifest.sh   # also run by scripts/verify-version.sh
bash scripts/component-version.sh query       # prints one component's version
```

## Per-component images

Each deployable component has its own container image, built from the
`component-runtime` target of the root [Dockerfile](../Dockerfile) (the frontend
from `apps/frontend/Dockerfile`). The component-builder stage builds only the
requested Cargo package, so a change to one component does not rebuild another
component's binary.

```bash
# All components
bash scripts/build-component-images.sh

# A subset, with a registry prefix and tag
REGISTRY=ghcr.io/ktjn TAG=0.1.0 bash scripts/build-component-images.sh query alerting

# Tag each image with its own manifest component version
REGISTRY=ghcr.io/ktjn TAG=manifest bash scripts/build-component-images.sh
```

Every image carries the `org.opencontainers.image.version` label set to its
manifest component version, whatever `TAG` is. `TAG` defaults to `local`.

Component name → image → Cargo package:

| Component | Image | Source |
| --- | --- | --- |
| `auth` | `observable-auth` | `services/auth-service` |
| `control` | `observable-control` | `services/admin-service` |
| `ingest` | `observable-ingest` | `services/ingest-gateway` |
| `process` | `observable-process` | `services/stream-processor` |
| `store-clickhouse` | `observable-store-clickhouse` | `services/storage-writer` |
| `query` | `observable-query` | `services/query-api` |
| `alerting` | `observable-alerting` | `services/alert-evaluator` |
| `web` | `observable-web` | `apps/frontend` |

`docker-compose.yml` runs each Rust service from its own
`observable-<component>:local` image. The smoke, perf, and soak runners use the
`verification-tools` Dockerfile target (`observable-verification:local`).

CI builds only the components a change affects. Run
`python3 scripts/ci_changed_components.py BASE HEAD` to see the set for a diff.

The shared `observable-services` image remains for the release workflow, Helm,
and kind until Helm moves to per-component image/version pins.
