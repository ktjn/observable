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

During the `0.2` decomposition the deployable components are still built from
this monorepo and share the root `VERSION`, so every component currently tracks
`0.1.0`. Independent per-component versions and image tags are introduced as
each component gains its own image and release lifecycle.

Validate it with:

```bash
bash scripts/check-distribution-manifest.sh
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
```

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

The shared `observable-services` image and the current Compose/Helm wiring remain
in place during the migration; Compose and Helm are switched to per-component
images and version pins in a later Phase 7 slice.
