# Web arm64 snapshot verification

Recorded 2026-10-19 for the [Phase 7 artifact boundary](component-decomposition.md#remaining-02-work).
This is local execution evidence, not published-release acceptance.

## Build

The [snapshot builder](../distribution/README.md#committed-snapshot-builds) successfully ran:

```text
python scripts/build_component_snapshot.py web --platform linux/arm64 --ref 71da53c1 --output NEW_DIRECTORY
```

`NEW_DIRECTORY` was a fresh temporary directory. Docker Desktop reported an x86_64
Linux engine and BuildKit v0.33.1 with arm64 support. The build and runtime used
emulation; no native arm64 hardware was tested.

| Property | Observed value |
|---|---|
| Source revision | `71da53c17b7b57296678c1dbbec526516cb4b349` |
| Component/version | `web` / `0.1.0` |
| Platform | `linux/arm64` |
| Source algorithm | `component-source-v1` |
| Source digest | `sha256:93881353329cc65ef9a91a57f5560d8e521d03ba24c4b6baef3344923fb850c7` |
| Context digest | `sha256:232bb5007e1bf6a43c09f269844f919f6a1745dda57fbcdded821aea909cf5a4` |
| Local image root digest/ID | `sha256:7eb079f7e9d90ed138192947d0515fb0fdd6d9a3042fb75cffae7230640deeaf` |
| Image binding | `descriptor` |

The builder verified platform, revision/version/source/context labels, and equality
between BuildKit's image digest, the loaded image descriptor and image ID.
`identity_verified` stayed `false`; web `runtime_checks` stayed empty because the
following HTTP verification was performed separately from the builder.

## Static serving

An ephemeral container ran the verified image ID with `--platform linux/arm64`,
`--pull never`, `--rm`, and a random host port bound only to `127.0.0.1`.
The committed nginx configuration required DNS-only `--add-host` mappings for
`auth-service`, `admin-service`, `alert-evaluator` and `query-api`, each to
`127.0.0.1`. No backend services ran or received verification requests.

| Request | Status | Content type | Body check |
|---|---|---|---|
| `/` | 200 | `text/html` | HTML doctype and root element |
| `/assets/index-bVu1DBIr.js` | 200 | `application/javascript` | 823,622 bytes; not HTML |
| `/assets/index-D9XAsqdN.css` | 200 | `text/css` | 47,313 bytes; not HTML |

The asset paths were parsed from the served HTML, rather than guessed. A first
probe incorrectly included the external Google Fonts stylesheet in the local-asset
assertion; after checking the generated HTML, the corrected probe verified both
packaged references above. The external stylesheet was explicitly left unverified.
Both verification containers were stopped and confirmed absent after auto-removal.

## Limits and follow-up

- Browser execution, lazy-loaded assets/workers/WASM, external fonts, API routing,
  authentication, backend connectivity and cluster integration were not tested.
- Rust arm64 builds/linkage and native arm64 hardware remain open checks.
- The dependency-install output reported eight vulnerabilities (two low, five high,
  one critical). No audit remediation or dependency change was part of this check.
- Local digests and BuildKit metadata do not authenticate release identity or prove
  reproducibility. External-material identity, authenticated history/attestations,
  publication race protection and GHCR publishing remain open under the
  [immutable release policy](../VERSIONING.md#immutable-component-releases).

No architecture or roadmap scope changed, so no ADR/spec change is needed: this
records an executed check of the existing ADR-035 artifact boundary. No source,
Docker recipe, dependency, test or protected regression gate changed. Full local CI
was not run for this documentation-only update; it remains required before any
later code push.
