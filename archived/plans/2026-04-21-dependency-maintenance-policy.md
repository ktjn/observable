# Dependency Maintenance Policy Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a formal dependency maintenance policy to `spec/10-process.md` and sync Docker image guidance into `CLAUDE.md`.

**Architecture:** Two targeted edits — a new `§16.10` section appended to the process spec, and a one-bullet addition to the CLAUDE.md dependency guidance block. No new files, no ADR needed; this is operational policy, not an architectural decision.

**Tech Stack:** Markdown, git.

---

### Task 1: Add §16.10 to `spec/10-process.md`

**Files:**
- Modify: `spec/10-process.md` — append new section after `§16.9`

- [x] **Step 1: Open the file and locate the insertion point**

The file ends at `§16.9 Documentation and Spec Review`. The new section goes immediately after the closing `---` of §16.9 (before `## 17. Project Plan`).

- [x] **Step 2: Insert the new section**

Add the following block between the end of §16.9 and `## 17. Project Plan`:

```markdown
### 16.10 Dependency Maintenance Policy

#### Pinning rules

| Ecosystem | Version specifier | Lockfile | Notes |
|---|---|---|---|
| Rust crates | `^major.minor` in `Cargo.toml` | `Cargo.lock` committed | Lockfile is the exact pin |
| npm packages | `^major` in `package.json` | `package-lock.json` committed | Lockfile is the exact pin |
| Docker Compose (local/dev) | `image:major.minor` minimum | n/a | |
| Production Dockerfiles / base images | `image:major.minor.patch` | SHA digest strongly preferred | |
| GitHub Actions | `action@vN` (latest major tag) | n/a | |

Lockfiles are always committed. Range specifiers without committed lockfiles are not permitted.

#### Update cadence

- **Routine:** monthly sweep — bump all dependencies to the latest stable version within the declared range, run `bash scripts/local-ci.sh`, open a dedicated PR.
- **Security (critical or high CVE):** 7-day SLA from public disclosure. Patch-only bumps bypass the monthly cycle.
- **Security (medium CVE):** 30-day SLA.
- **Breaking upgrades** (major version bumps, image EOL): treated as a feature slice — the PR must include a source spec reference, acceptance target, and rollback note. Do not bundle breaking upgrades with routine dependency updates.

#### Automation

Dependabot or Renovate is the preferred tool for surfacing routine update PRs. Configuration lives in `.github/dependabot.yml` or `renovate.json`. Automation is not required before Phase 2 but is the target state.

#### Ownership

- The PR author is responsible for verifying the update does not break `bash scripts/local-ci.sh` before pushing. No exceptions.
- Routine dependency PRs must state: what changed, whether local-ci passed, and whether any lockfile drift was introduced.

```

- [x] **Step 3: Verify the section renders correctly**

Read back the modified region of `spec/10-process.md` around the new section to confirm heading numbering, table alignment, and no broken Markdown.

- [x] **Step 4: Commit**

```bash
git add spec/10-process.md
git commit -m "spec(process): add §16.10 dependency maintenance policy"
```

---

### Task 2: Sync Docker image guidance into `CLAUDE.md`

**Files:**
- Modify: `CLAUDE.md` — add Docker image bullet to the "Before Starting Any Implementation Task" dependency block

- [x] **Step 1: Locate the insertion point**

In `CLAUDE.md`, find the block:

```markdown
2. **Use the latest stable versions** of all dependencies:
   - **Rust crates:** check [crates.io](https://crates.io) for the current stable version before adding or updating a dependency.
   - **npm packages:** check [npmjs.com](https://www.npmjs.com) for the current stable version before adding or updating a dependency.
   - **GitHub Actions:** use the latest release tag of every action (e.g. `actions/checkout@v4`); check the action's release page if uncertain.
   - Do not pin to an older version without a documented reason in the PR description.
```

- [x] **Step 2: Add the Docker image bullet**

Replace the block above with:

```markdown
2. **Use the latest stable versions** of all dependencies:
   - **Rust crates:** check [crates.io](https://crates.io) for the current stable version before adding or updating a dependency.
   - **npm packages:** check [npmjs.com](https://www.npmjs.com) for the current stable version before adding or updating a dependency.
   - **GitHub Actions:** use the latest release tag of every action (e.g. `actions/checkout@v4`); check the action's release page if uncertain.
   - **Docker images (Compose/local):** pin to `image:major.minor` at minimum. For production Dockerfiles and base images, use `image:major.minor.patch`; SHA digest is strongly preferred.
   - Do not pin to an older version without a documented reason in the PR description.
```

- [x] **Step 3: Verify the change**

Read back the modified block in `CLAUDE.md` to confirm the bullet is correctly indented and the surrounding text is intact.

- [x] **Step 4: Commit**

```bash
git add CLAUDE.md
git commit -m "docs(claude): add Docker image pinning guidance to dependency rules"
```

---

### Task 3: Open PR

- [x] **Step 1: Push the branch**

```bash
git push -u origin <branch-name>
```

- [x] **Step 2: Open PR**

Title: `spec(process): add dependency maintenance policy (§16.10)`

Body must include:
- Source spec: `docs/superpowers/specs/2026-04-21-dependency-maintenance-policy-design.md`
- Acceptance target: `§16.10` exists in `spec/10-process.md` with pinning rules, cadence, CVE SLAs, automation note, and ownership; CLAUDE.md has Docker image guidance
- ADR/spec sync: no ADR change required — this is operational policy, not an architectural decision
- Doc/spec review: run `doc-review` skill and confirm all four phases pass before opening PR
- Verification: `bash scripts/local-ci.sh` passes (or exempt — pure `.md` changes)
- New errors introduced: none
- Next smallest slice: configure Dependabot (`.github/dependabot.yml`) for Rust, npm, and GitHub Actions — Phase 2 target
