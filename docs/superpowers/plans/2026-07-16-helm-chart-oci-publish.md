# Publish Helm Chart to GHCR OCI Registry Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Publish the already-packaged Observable Helm chart to `ghcr.io/ktjn/charts/observable` as an OCI artifact during the tag-triggered release workflow, so it is installable via `helm install ... oci://...` instead of only downloadable as a GitHub Actions build artifact.

**Architecture:** Add two steps to the existing `build` job in `.github/workflows/release-artifacts.yml`, immediately after the current "Package Helm chart and checksums" step: a `helm registry login` against `ghcr.io` reusing the job's existing `GITHUB_TOKEN`, then a `helm push` of the already-produced `dist/observable-{version}.tgz` to `oci://ghcr.io/ktjn/charts`. No new workflow file, trigger, or secret.

**Tech Stack:** GitHub Actions, Helm 4.2.3 (`azure/setup-helm`, already pinned in this workflow), GHCR OCI registry.

## Global Constraints

- Same trigger as today: `push: tags: v*` — no new trigger.
- Reuse the existing `GITHUB_TOKEN` / `packages: write` permission already granted in this workflow — no new secret.
- Do not modify the tag/version verification steps, the image build steps, or the existing chart packaging step that already produce `dist/observable-{version}.tgz`.
- Chart ref is derived entirely from `charts/observable/Chart.yaml`'s `name`/`version` fields (currently `observable` / must match `VERSION` per `scripts/verify-version.sh`) — do not hardcode a version string in the new steps.
- `ROADMAP.md`'s "Publish the Helm chart" checkbox stays unchecked in this PR (per the design doc, matching the two prior Milestone 7 items) — add a note instead, do not tick it.
- This is a docs+CI-workflow-only change; no application code, chart contents, or chart version bump.

---

### Task 1: Add Helm OCI publish steps, docs, and roadmap note

**Files:**
- Modify: `.github/workflows/release-artifacts.yml` (insert two new steps after the "Package Helm chart and checksums" step, before "Upload tag-bound release artifacts")
- Modify: `VERSIONING.md` (extend "Tag-bound artifact builds" section)
- Modify: `ROADMAP.md` (add a note under the Milestone 7 "Publish the Helm chart" line)

**Interfaces:**
- Consumes: the `dist/observable-${{ steps.version.outputs.version }}.tgz` file already produced by the existing "Package Helm chart and checksums" step in this same job (uses the job's existing `steps.version.outputs.version` output — no new output needed).
- Produces: `ghcr.io/ktjn/charts/observable:{VERSION}` OCI artifact (nothing downstream in this repo consumes it programmatically; it's an external distribution target).

- [ ] **Step 1: Read the current workflow file to get exact surrounding context**

Run: `sed -n '80,115p' .github/workflows/release-artifacts.yml`

Expected: shows the `azure/setup-helm` step, the "Package Helm chart and checksums" step (ending with the `sha256sum * > SHA256SUMS` block), and the "Upload tag-bound release artifacts" step that follows it. Confirm the exact text of the step name `- name: Package Helm chart and checksums` and the step name `- name: Upload tag-bound release artifacts` — the next step inserts between these two, matched on this exact text.

- [ ] **Step 2: Insert the two new steps into the workflow**

Insert immediately after the closing of the "Package Helm chart and checksums" step's `run:` block (right before the `- name: Upload tag-bound release artifacts` step):

```yaml
      - name: Log in to GHCR for Helm
        shell: bash
        run: |
          set -euo pipefail
          echo "${{ secrets.GITHUB_TOKEN }}" | helm registry login ghcr.io \
            --username "${{ github.actor }}" \
            --password-stdin

      - name: Publish Helm chart to GHCR
        shell: bash
        run: |
          set -euo pipefail
          version="${{ steps.version.outputs.version }}"
          helm push "dist/observable-${version}.tgz" oci://ghcr.io/ktjn/charts
```

- [ ] **Step 3: Validate the workflow YAML still parses**

Run: `python -c "import yaml; yaml.safe_load(open('.github/workflows/release-artifacts.yml')); print('valid yaml')"`

Expected: `valid yaml`

- [ ] **Step 4: Confirm the new steps sit between the right neighbors**

Run: `grep -n "name:" .github/workflows/release-artifacts.yml`

Expected: the ordered step-name list shows `Package Helm chart and checksums`, then `Log in to GHCR for Helm`, then `Publish Helm chart to GHCR`, then `Upload tag-bound release artifacts`, in that order, with no other step names disturbed from the current file.

- [ ] **Step 5: Update VERSIONING.md**

In the "Tag-bound artifact builds" section, after the paragraph ending `...linux/amd64 and linux/arm64. No mutable latest tag is published. SBOM generation, provenance attestations, and signing remain separate release steps so their policy can be reviewed independently.`, add:

```markdown

The packaged Helm chart is published to:

```text
oci://ghcr.io/ktjn/charts/observable
```

tagged `{VERSION}` to match the chart's `version` in `charts/observable/Chart.yaml`. Install
directly from the registry with:

```text
helm install observable oci://ghcr.io/ktjn/charts/observable --version {VERSION}
```
```

- [ ] **Step 6: Update ROADMAP.md**

Find the Milestone 7 line:

```markdown
- [ ] Publish the Helm chart.
```

Replace with:

```markdown
- [ ] Publish the Helm chart. (workflow implemented; mark complete after the first release
      tag verifies the chart can be pulled and installed from `oci://ghcr.io/ktjn/charts/observable`)
```

- [ ] **Step 7: Verify no unrelated lines changed**

Run: `git diff --stat`

Expected: exactly three files changed — `.github/workflows/release-artifacts.yml`, `VERSIONING.md`, `ROADMAP.md`.

- [ ] **Step 8: Commit**

```bash
git add .github/workflows/release-artifacts.yml VERSIONING.md ROADMAP.md
git commit -m "ci(release): publish Helm chart to GHCR as an OCI artifact"
```

---

## Manual verification (cannot be exercised by PR CI)

The tag-only `Release Artifacts` workflow cannot run end-to-end from a pull request. Note in the
PR description (matching the pattern already used for the two prior Milestone 7 PRs): full
verification requires pushing the next real `v{VERSION}` release tag and confirming
`helm pull oci://ghcr.io/ktjn/charts/observable --version {VERSION}` succeeds afterward. Do not
push a release tag as part of this task — that is a separate, explicitly user-authorized action.
