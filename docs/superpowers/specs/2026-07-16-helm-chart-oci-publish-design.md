# Publish Helm Chart to GHCR OCI Registry — Design

**Date:** 2026-07-16
**Roadmap item:** `ROADMAP.md` Milestone 7 — "Publish the Helm chart."

## Problem

The `Release Artifacts` workflow (`.github/workflows/release-artifacts.yml`) already packages the
Observable Helm chart into `dist/observable-{VERSION}.tgz` and uploads it as a 30-day GitHub
Actions build artifact. That is a build output, not a publish target: there is no `helm repo add`
or `helm pull oci://...` path a user can install from. This closes that gap.

## Approach

Publish the already-packaged chart to an OCI registry on GHCR, alongside the existing
`ghcr.io/ktjn/observable-services` container image, using the same registry and the same
`GITHUB_TOKEN` credential already used for the image push.

Rejected alternative: a classic `index.yaml`-based chart repository hosted on a `gh-pages`
branch. This is the older Helm distribution pattern and requires maintaining a second publish
target and an index-merge step across releases. OCI push is the Helm-recommended direction and
avoids a second hosting mechanism entirely, since GHCR already hosts the container image.

## Design

- **Location in workflow:** a new step in the existing `build` job in
  `.github/workflows/release-artifacts.yml`, immediately after "Package Helm chart and checksums"
  and before "Upload tag-bound release artifacts." No new workflow file, no new trigger — same
  `push: tags: v*` trigger and the same tag/version verification gate already at the top of the
  job.
- **Auth:** `helm registry login ghcr.io -u ${{ github.actor }} --password-stdin` fed
  `${{ secrets.GITHUB_TOKEN }}`. Reuses the token already granted `packages: write` for the Docker
  image login; no new secret.
- **Publish command:** `helm push dist/observable-${version}.tgz oci://ghcr.io/ktjn/charts`. Helm's
  OCI push derives the final reference (`ghcr.io/ktjn/charts/observable:{version}`) from the
  chart's own `name`/`version` fields in `charts/observable/Chart.yaml` — no separate ref
  bookkeeping needed in the workflow.
- **Result:** `ghcr.io/ktjn/charts/observable:{VERSION}`, installable via:
  ```
  helm install observable oci://ghcr.io/ktjn/charts/observable --version {VERSION}
  ```
- **Docs:** `VERSIONING.md`'s "Tag-bound artifact builds" section gets a short addition
  documenting the published chart location and the install command above.
- **Roadmap bookkeeping:** Following the pattern already used for the two prior Milestone 7 items
  (build-from-tags, multi-arch image publish), the ROADMAP.md checkbox for "Publish the Helm
  chart" stays unchecked in this PR. A note is added mirroring the existing PR language: mark
  complete only after the first real release tag verifies the chart can be pulled and installed.

## Out of scope

- Chart signing or provenance attestations (separate roadmap items later in Milestone 7).
- A classic `index.yaml`/gh-pages chart repository (rejected above).
- Any change to chart contents, values, or chart version bump policy.

## Testing / verification

This is a CI-workflow-only change; there is no application code path to unit test. Verification
for this PR is:

- The workflow YAML parses and the job's existing steps (version verification, image build,
  chart packaging) are unaffected — confirmed by not modifying any step before the insertion
  point.
- `helm push` syntax and OCI ref derivation matches Helm's documented OCI support (chart
  name/version from `Chart.yaml`, registry path is the parent of that name).

The push itself can only be exercised end-to-end by a real `v{VERSION}` tag (same caveat already
recorded for the two prior Milestone 7 PRs) — pull-request CI cannot trigger the tag-only
workflow.
