# SBOM and Provenance Attestations for the Release Image — Design

**Date:** 2026-07-16
**Roadmap item:** `ROADMAP.md` Milestone 7 — "Generate SBOMs and provenance attestations."

## Problem

The tag-triggered `Release Artifacts` workflow (`.github/workflows/release-artifacts.yml`) builds
and publishes a multi-architecture container image to GHCR, but both `docker/build-push-action`
steps explicitly set `provenance: false` and `sbom: false` — this was a deliberate deferral
(documented in `VERSIONING.md`: "SBOM generation, provenance attestations, and signing remain
separate release steps so their policy can be reviewed independently"). This closes the SBOM and
provenance half of that deferral; signing is a separate, later roadmap item.

## Approach

Use GitHub's native artifact attestation actions (`actions/attest-sbom`,
`actions/attest-build-provenance`) rather than BuildKit's built-in `provenance`/`sbom` flags on
`docker/build-push-action`. GitHub's attestations are independently verifiable by any consumer via
`gh attestation verify` or the GitHub UI, tied to the specific image digest and this workflow run
as the recorded builder — which is more discoverable to downstream consumers than BuildKit's
embedded OCI referrers, and shares the same Sigstore/OIDC identity that the next roadmap item
(artifact signing) will also use.

Rejected alternative: flipping `provenance: true, sbom: true` on the existing
`docker/build-push-action` steps. This requires zero new actions, but the resulting attestations
are BuildKit-embedded OCI referrers, less visible to consumers than GitHub's attestation API/UI,
and this path does not naturally extend to file-based artifacts (Helm chart, downloadable `dist/`
bundle) the way GitHub's attestation actions do, should those be added later.

## Scope

This pass covers **the published multi-arch container image only**
(`ghcr.io/${GITHUB_REPOSITORY_OWNER,,}/observable-services:v{VERSION}`) — the artifact most
consumers actually pull and run. The Helm chart (`oci://ghcr.io/.../charts/observable`) and the
downloadable `dist/` bundle (OCI archive tar, chart `.tgz`, `SHA256SUMS`) are explicitly deferred
to a follow-up once this pattern is proven on the first real release tag. Signing container images
and artifacts (cosign) is a separate, later Milestone 7 item — out of scope here.

## Design

- **Location in workflow:** the existing `build` job in `.github/workflows/release-artifacts.yml`.
  The "Publish multi-architecture image" step gets a new `id: publish` so its `digest` output
  (the manifest-list digest of the pushed multi-arch image) can be referenced by later steps. Three
  new steps are inserted immediately after it and before the `azure/setup-helm` step:
  1. **Generate SBOM** — `anchore/sbom-action@v0` scans the pushed image reference
     (`${{ steps.version.outputs.image }}:${{ steps.version.outputs.tag }}`) directly from GHCR
     (already authenticated via the existing `docker/login-action` step earlier in the job) and
     writes an SPDX-JSON SBOM to a local file (`sbom.spdx.json`). `upload-artifact: false`, since
     this pass does not add a separate downloadable SBOM artifact.
  2. **Attest SBOM** — `actions/attest-sbom@v2` with `subject-name: ${{ steps.version.outputs.image }}`,
     `subject-digest: ${{ steps.publish.outputs.digest }}`, `sbom-path: sbom.spdx.json`.
  3. **Attest build provenance** — `actions/attest-build-provenance@v2` with the same
     `subject-name`/`subject-digest`, recording this workflow run as the builder.
- **Permissions:** add `id-token: write` and `attestations: write` to the workflow's existing
  `permissions:` block (alongside `contents: read`, `packages: write`) — both are required by
  GitHub's attestation actions for OIDC-based Sigstore signing of the attestation.
- **Docs:** `VERSIONING.md`'s "Tag-bound artifact builds" section gets a short addition documenting
  that the published image carries SBOM and provenance attestations, and the verification command:
  ```
  gh attestation verify oci://ghcr.io/{owner}/observable-services:v{VERSION} --owner {owner}
  ```
- **Roadmap bookkeeping:** following the pattern used for the three prior Milestone 7 items, the
  `ROADMAP.md` checkbox for "Generate SBOMs and provenance attestations" stays unchecked in this
  PR, with a note that it should be marked complete after the first real release tag confirms both
  attestations are present and verify successfully.

## Out of scope

- Helm chart and downloadable `dist/` bundle attestations (deferred follow-up, noted above).
- Signing container images or release artifacts with cosign (separate, later roadmap item).
- Any change to the image build steps' `provenance`/`sbom` flags on `docker/build-push-action`
  (left as `false`; attestation is handled by the new dedicated steps instead).

## Testing / verification

This is a CI-workflow-and-docs-only change; there is no application code path to unit test.
Verification for this PR:

- The workflow YAML parses.
- The new steps reference only outputs that already exist (`steps.version.outputs.image`,
  `steps.version.outputs.tag`) or that this change adds (`steps.publish.outputs.digest`).

Whether the SBOM scan and both attestations actually succeed against a real pushed image can only
be exercised by a real `v{VERSION}` tag — the same caveat already recorded for the three prior
Milestone 7 PRs. Pull-request CI cannot trigger this tag-only workflow.
