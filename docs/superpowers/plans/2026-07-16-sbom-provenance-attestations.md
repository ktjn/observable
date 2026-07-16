# SBOM and Provenance Attestations for the Release Image Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Attach an SPDX SBOM and SLSA build-provenance attestation to the published multi-arch container image (`ghcr.io/${GITHUB_REPOSITORY_OWNER,,}/observable-services:v{VERSION}`) during the tag-triggered release workflow, using GitHub's native attestation actions.

**Architecture:** Add an `id: publish` to the existing "Publish multi-architecture image" step in `.github/workflows/release-artifacts.yml` (so its `digest` output can be referenced), then insert three new steps immediately after it: generate an SPDX SBOM with `anchore/sbom-action`, attest it with `actions/attest-sbom`, and attest build provenance with `actions/attest-build-provenance`. No new workflow file or trigger.

**Tech Stack:** GitHub Actions, `anchore/sbom-action` (syft), `actions/attest-sbom`, `actions/attest-build-provenance` (GitHub-native Sigstore-backed attestations).

## Global Constraints

- Same trigger as today: `push: tags: v*` — no new trigger.
- Add exactly two new permissions to the workflow: `id-token: write` and `attestations: write` — required by GitHub's attestation actions. Do not add any other new permission.
- Scope is the published container image only — do not attest the Helm chart or the downloadable `dist/` bundle in this task (explicitly deferred per the design).
- Do not modify the tag/version verification step, the image build steps' `provenance: false`/`sbom: false` flags, the chart packaging step, or the Helm OCI publish steps added in the prior PR.
- `ROADMAP.md`'s "Generate SBOMs and provenance attestations" checkbox stays unchecked in this PR — add a note instead, matching the convention already used for the three prior Milestone 7 items.
- This is a docs+CI-workflow-only change; no application code, no chart contents, no version bump.
- Pin every new `uses:` action to an exact commit SHA with a trailing `# vX.Y.Z` comment, matching every existing `uses:` line in this file. Use exactly these pins (looked up from each action's GitHub releases):
  - `anchore/sbom-action@e22c389904149dbc22b58101806040fa8d37a610 # v0.24.0`
  - `actions/attest-sbom@c604332985a26aa8cf1bdc465b92731239ec6b9e # v4.1.0`
  - `actions/attest-build-provenance@0f67c3f4856b2e3261c31976d6725780e5e4c373 # v4.1.1`

---

### Task 1: Add SBOM generation and attestation steps, permissions, and docs

**Files:**
- Modify: `.github/workflows/release-artifacts.yml` (add two permissions; add `id: publish` to the existing "Publish multi-architecture image" step; insert three new steps after it, before the `azure/setup-helm` step)
- Modify: `VERSIONING.md` (extend "Tag-bound artifact builds" section)
- Modify: `ROADMAP.md` (add a note under the Milestone 7 "Generate SBOMs and provenance attestations" line)

**Interfaces:**
- Consumes: `steps.version.outputs.image` and `steps.version.outputs.tag` (already produced by the existing "Verify release tag and source commit" step in this same job).
- Produces: `steps.publish.outputs.digest` (the manifest-list digest of the pushed multi-arch image, from the newly-added `id: publish` on the existing push step) — consumed by the two new attest steps later in this same task. Nothing outside this task depends on it.

- [ ] **Step 1: Read the current workflow file to get exact surrounding context**

Run: `sed -n '1,15p;68,85p' .github/workflows/release-artifacts.yml`

Expected: shows the top-level `permissions:` block (`contents: read`, `packages: write`) and the "Publish multi-architecture image" step through the following `azure/setup-helm` step. Confirm the exact text of the step name `- name: Publish multi-architecture image` and the step `- uses: azure/setup-helm@9bc31f4ebc9c6b171d7bfbaa5d006ae7abdb4310 # v5` — the new steps insert between these two, matched on this exact text.

- [ ] **Step 2: Add the two new permissions**

Change:

```yaml
permissions:
  contents: read
  packages: write
```

to:

```yaml
permissions:
  contents: read
  packages: write
  id-token: write
  attestations: write
```

- [ ] **Step 3: Add `id: publish` to the existing push step**

Change:

```yaml
      - name: Publish multi-architecture image
        uses: docker/build-push-action@53b7df96c91f9c12dcc8a07bcb9ccacbed38856a # v7
```

to:

```yaml
      - name: Publish multi-architecture image
        id: publish
        uses: docker/build-push-action@53b7df96c91f9c12dcc8a07bcb9ccacbed38856a # v7
```

Do not change anything else in this step (its `with:` block, `provenance: false`, `sbom: false` stay exactly as they are).

- [ ] **Step 4: Insert the three new steps after the push step**

Insert immediately after the "Publish multi-architecture image" step's closing (right before the `- uses: azure/setup-helm@...` step):

```yaml
      - name: Generate SBOM
        uses: anchore/sbom-action@e22c389904149dbc22b58101806040fa8d37a610 # v0.24.0
        with:
          image: ${{ steps.version.outputs.image }}:${{ steps.version.outputs.tag }}
          format: spdx-json
          output-file: sbom.spdx.json
          upload-artifact: false

      - name: Attest SBOM
        uses: actions/attest-sbom@c604332985a26aa8cf1bdc465b92731239ec6b9e # v4.1.0
        with:
          subject-name: ${{ steps.version.outputs.image }}
          subject-digest: ${{ steps.publish.outputs.digest }}
          sbom-path: sbom.spdx.json

      - name: Attest build provenance
        uses: actions/attest-build-provenance@0f67c3f4856b2e3261c31976d6725780e5e4c373 # v4.1.1
        with:
          subject-name: ${{ steps.version.outputs.image }}
          subject-digest: ${{ steps.publish.outputs.digest }}
```

- [ ] **Step 5: Validate the workflow YAML still parses**

Run: `python -c "import yaml; yaml.safe_load(open('.github/workflows/release-artifacts.yml')); print('valid yaml')"`

Expected: `valid yaml`

- [ ] **Step 6: Confirm the new steps and permissions sit in the right place**

Run: `grep -n "name:\|permissions:\|id-token\|attestations:\|id: publish" .github/workflows/release-artifacts.yml`

Expected: the `permissions:` block shows all four keys (`contents`, `packages`, `id-token`, `attestations`); the ordered step-name list shows `Publish multi-architecture image` (now with `id: publish` visible above its `uses:` line), then `Generate SBOM`, then `Attest SBOM`, then `Attest build provenance`, then `Package Helm chart and checksums` continuing on from the existing `azure/setup-helm` step, with no other step names disturbed from the current file.

- [ ] **Step 7: Update VERSIONING.md**

In the "Tag-bound artifact builds" section, after the paragraph documenting the Helm chart OCI publish (ending `...--version {VERSION}`), add:

```markdown

The published container image carries an SPDX SBOM and SLSA build-provenance attestation,
generated and attached during this workflow via GitHub's native attestation actions. Verify
either with:

```text
gh attestation verify oci://ghcr.io/{owner}/observable-services:v{VERSION} --owner {owner}
```
```

- [ ] **Step 8: Update ROADMAP.md**

Find the Milestone 7 line:

```markdown
- [ ] Generate SBOMs and provenance attestations.
```

Replace with:

```markdown
- [ ] Generate SBOMs and provenance attestations. (workflow implemented for the container image;
      mark complete after the first release tag verifies both attestations are present and
      `gh attestation verify` succeeds)
```

- [ ] **Step 9: Verify no unrelated lines changed**

Run: `git diff --stat`

Expected: exactly three files changed — `.github/workflows/release-artifacts.yml`, `VERSIONING.md`, `ROADMAP.md`.

- [ ] **Step 10: Commit**

```bash
git add .github/workflows/release-artifacts.yml VERSIONING.md ROADMAP.md
git commit -m "ci(release): attest SBOM and build provenance for release image"
```

---

## Manual verification (cannot be exercised by PR CI)

The tag-only `Release Artifacts` workflow cannot run end-to-end from a pull request. Note in the
PR description (matching the pattern already used for the three prior Milestone 7 PRs): full
verification requires pushing the next real `v{VERSION}` release tag and confirming
`gh attestation verify oci://ghcr.io/{owner}/observable-services:v{VERSION} --owner {owner}`
reports both a valid SBOM and a valid build-provenance attestation. Do not push a release tag as
part of this task — that is a separate, explicitly user-authorized action.
