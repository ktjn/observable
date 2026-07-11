# Admin UI Cleanup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Collapse the duplicated admin/setup navigation into a single coherent Administration section, remove or simplify dead-end stub pages, and rename the Live button in log search.

**Architecture:** Nav tree restructure only (no URL changes); duplicate page deleted and its unique panels folded into BillingReportPage; Fleet page stripped to a single coming-soon notice; Identity info inlined into Overview; Live button renamed to Tail with NLQ caveat.

**Tech Stack:** React + TanStack Router, Vitest + Testing Library.

## Global Constraints

- Never change `/setup`, `/setup/llm`, `/setup/tokens` URLs — onboarding flow depends on them.
- Keep `/admin/identity` route alive (just remove from nav) — direct links must still resolve.
- No new npm packages.
- Run `npm test` in `apps/frontend` after every task.

---

### Task 1: Merge Setup nav group into Administration

Removes the top-level "Setup" nav group from the sidebar. Ingest, LLM, and API Tokens become children of "Administration". URLs stay the same.

**Files:**
- Modify: `apps/frontend/src/components/AppShell.tsx:29-75`
- Modify: `apps/frontend/src/components/AppShell.test.tsx`

**Interfaces:**
- Consumes: `buildNavTree()` local function that returns `NavTreeItem[]`
- Produces: same `buildNavTree()` — callers unchanged

- [ ] **Step 1: Write the failing test**

Open `apps/frontend/src/components/AppShell.test.tsx` and add to the `describe("AppShell navigation")` block:

```typescript
test("shows Ingest, LLM and Tokens under Administration, not under a separate Setup group", () => {
  render(<AppShell />);

  expect(screen.getByRole("link", { name: "Ingest" })).toBeInTheDocument();
  expect(screen.getByRole("link", { name: "LLM" })).toBeInTheDocument();
  expect(screen.getByRole("link", { name: "Tokens" })).toBeInTheDocument();
  // No top-level "Setup" parent link should appear
  expect(screen.queryByRole("link", { name: "Setup" })).not.toBeInTheDocument();
});
```

- [ ] **Step 2: Run test to verify it fails**

```
cd apps/frontend && npm test -- --run AppShell
```

Expected: FAIL — "Setup" link present, Ingest/LLM/Tokens still live under it.

- [ ] **Step 3: Update `buildNavTree` in AppShell.tsx**

Replace the `setup` object and expand the `admin` children block:

```typescript
// Before (lines ~33-41, ~60-69):
{
  id: "setup",
  label: "Setup",
  icon: Wrench,
  children: [
    { id: "setup-ingest", label: "Ingest", to: "/setup" },
    { id: "setup-llm", label: "LLM", to: "/setup/llm" },
    { id: "setup-tokens", label: "Tokens", to: "/setup/tokens" },
  ],
},
// ...
{
  id: "admin",
  label: "Administration",
  icon: Settings,
  children: [
    { id: "admin-overview", label: "Overview", to: "/admin" },
    { id: "admin-config", label: "Tenant configuration", to: "/admin/config" },
    { id: "admin-fleet", label: "Fleet management", to: "/admin/fleet" },
    { id: "admin-identity", label: "Identity", to: "/admin/identity" },
  ],
},
```

Replace both blocks with:

```typescript
{
  id: "admin",
  label: "Administration",
  icon: Settings,
  children: [
    { id: "admin-overview", label: "Overview", to: "/admin" },
    { id: "admin-members", label: "Members", to: "/admin/members" },
    { id: "admin-fleet", label: "Fleet management", to: "/admin/fleet" },
    { id: "setup-ingest", label: "Ingest", to: "/setup" },
    { id: "setup-llm", label: "LLM", to: "/setup/llm" },
    { id: "setup-tokens", label: "Tokens", to: "/setup/tokens" },
  ],
},
```

Also remove the `Wrench` import from lucide-react since it's no longer used (check: if `Wrench` only appeared in the setup group, remove it; if still referenced elsewhere, leave it).

- [ ] **Step 4: Run test to verify it passes**

```
cd apps/frontend && npm test -- --run AppShell
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add apps/frontend/src/components/AppShell.tsx apps/frontend/src/components/AppShell.test.tsx
git commit -m "feat(admin-nav): fold Setup items into Administration nav group"
```

---

### Task 2: Delete /admin/config — merge its unique panels into Overview

`TenantConfigurationPage` duplicates `BillingReportPage` with two unique panels: **Quota posture** and **Environment scope**. We fold those into `BillingReportPage`, delete the config page, remove its route and nav entry, and drop the now-redundant `Identity settings` header/action links (replaced in Task 4).

**Files:**
- Modify: `apps/frontend/src/features/admin/BillingReportPage.tsx`
- Modify: `apps/frontend/src/features/admin/AdminSurfaceNav.tsx`
- Delete: `apps/frontend/src/features/admin/TenantConfigurationPage.tsx`
- Delete: `apps/frontend/src/pages/AdminConfigPage.tsx`
- Modify: `apps/frontend/src/router.ts` — remove `adminConfigRoute` import and route
- Modify: `apps/frontend/src/pages/AdminPage.test.tsx` — remove config page test

**Interfaces:**
- Consumes: `getTenantUsageReport`, `listEnvironments`, `countTone`, `Badge` — all already imported in BillingReportPage
- Produces: none (deletion task)

- [ ] **Step 1: Write the failing test**

In `apps/frontend/src/pages/AdminPage.test.tsx`, locate the test `"renders the tenant usage report for the admin overview"` and add two assertions:

```typescript
// Quota posture panel
const quotaPosture = screen.getByRole("heading", { name: "Quota posture" });
expect(quotaPosture).toBeInTheDocument();

// Environment scope panel
const envScope = screen.getByRole("heading", { name: "Environment scope" });
expect(envScope).toBeInTheDocument();
```

Also remove the test block `"renders the tenant configuration page at /admin/config"` entirely (lines 188–199 in the current file).

- [ ] **Step 2: Run test to verify the new assertions fail**

```
cd apps/frontend && npm test -- --run AdminPage
```

Expected: FAIL — "Quota posture" and "Environment scope" headings not found in Overview.

- [ ] **Step 3: Merge panels into BillingReportPage**

In `apps/frontend/src/features/admin/BillingReportPage.tsx`:

**a) Add `getTenantUsageReport` to the existing query** — it's already there (`data`). The environments query is also already present (`environmentsData`). Nothing new to import.

**b) Replace** the placeholder "Identity and access" panel body with the Quota posture panel. Change:

```tsx
<Panel title="Identity and access" eyebrow="Settings">
  <p className="max-w-3xl text-sm text-[var(--muted)]">
    Identity provider settings live in the dedicated identity page. This console only
    summarizes the current tenant and role context.
  </p>
</Panel>
```

With the Quota posture panel from TenantConfigurationPage:

```tsx
<Panel title="Quota posture" eyebrow="Usage">
  <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
    <MetricCard label="Cost index" value={data.estimated_cost_index} tone={countTone(data.estimated_cost_index)} />
    <MetricCard label="Query reads" value={control.query_reads} tone={countTone(control.query_reads)} />
    <MetricCard label="Telemetry spans" value={telemetry.spans} tone={countTone(telemetry.spans)} />
    <MetricCard label="Metric series" value={telemetry.metric_series_created} tone={countTone(telemetry.metric_series_created)} />
    <MetricCard label="Credential denials" value={control.credential_denies} tone={control.credential_denies > 0 ? "warn" : "good"} />
  </div>
</Panel>
```

**c) Add** the Environment scope panel after the Quota posture panel. Add to imports: `environments` is already computed from `environmentsData` in the component. Add the panel:

```tsx
<Panel title="Environment scope" eyebrow="Discovery">
  <div className="flex flex-wrap gap-2">
    {environments.map((entry) => (
      <Badge key={entry.environment} tone={entry.environment === environment ? "good" : "neutral"}>
        {entry.environment}
      </Badge>
    ))}
    {environments.length === 0 && (
      <span className="text-sm text-[var(--muted)]">No environments returned for the selected tenant.</span>
    )}
  </div>
</Panel>
```

**d) Remove** the "Identity settings" `<Link>` from the page header (`to="/admin/identity"`) and from the Tenant access panel `actions` prop. These are dead links once the nav entry is removed. Both will be re-added as real content in Task 4.

- [ ] **Step 4: Remove "Tenant configuration" from AdminSurfaceNav**

In `apps/frontend/src/features/admin/AdminSurfaceNav.tsx`, remove the `{ to: "/admin/config", label: "Tenant configuration" }` entry from the `sections` array. Also remove `{ to: "/admin/identity", label: "Identity" }` — the nav tab for Identity goes away now (content will be in Overview after Task 4). Final sections:

```typescript
const sections: AdminSection[] = [
  { to: "/admin", label: "Overview" },
  { to: "/admin/members", label: "Members" },
  { to: "/admin/fleet", label: "Fleet management" },
];
```

- [ ] **Step 5: Remove the config route from router.ts**

In `apps/frontend/src/router.ts`:
- Remove `import AdminConfigPage from "./pages/AdminConfigPage";` (line 5)
- Remove the `adminConfigRoute` const (lines 186–190)
- Remove `adminConfigRoute` from the `routeTree` array (line ~292)

- [ ] **Step 6: Delete the now-unused files**

```bash
rm apps/frontend/src/features/admin/TenantConfigurationPage.tsx
rm apps/frontend/src/pages/AdminConfigPage.tsx
```

- [ ] **Step 7: Run test to verify it passes**

```
cd apps/frontend && npm test -- --run AdminPage
```

Expected: PASS — "Quota posture" and "Environment scope" headings found in Overview; config page test gone.

- [ ] **Step 8: Commit**

```bash
git add apps/frontend/src/features/admin/BillingReportPage.tsx \
        apps/frontend/src/features/admin/AdminSurfaceNav.tsx \
        apps/frontend/src/router.ts \
        apps/frontend/src/pages/AdminPage.test.tsx
git rm apps/frontend/src/features/admin/TenantConfigurationPage.tsx \
       apps/frontend/src/pages/AdminConfigPage.tsx
git commit -m "feat(admin-nav): merge tenant config page into overview; remove /admin/config route"
```

---

### Task 3: Simplify Fleet Management — replace contract dump with coming-soon notice

The current `FleetManagementPage` has 200+ lines of internal contract documentation (field tables, heartbeat schema, etc.). This belongs in docs, not the product UI. Replace it with a single panel that describes what fleet management will do.

**Files:**
- Modify: `apps/frontend/src/features/admin/FleetManagementPage.tsx`
- Modify: `apps/frontend/src/pages/AdminPage.test.tsx`

**Interfaces:**
- Consumes: `Panel`, `EmptyState`, `AdminSurfaceNav` — same as today
- Produces: same component export name `FleetManagementPage`

- [ ] **Step 1: Update the fleet management test**

In `apps/frontend/src/pages/AdminPage.test.tsx`, replace the test `"renders the fleet management contract page at /admin/fleet"`:

```typescript
test("renders the fleet management page at /admin/fleet", async () => {
  window.history.pushState({}, "", "/admin/fleet");

  render(<App />);

  await screen.findByRole("heading", { name: "Fleet management" });
  expect(screen.getByText("Fleet management is not yet available")).toBeInTheDocument();
  // contract tables should be gone
  expect(screen.queryByText("agent.up", { selector: "td" })).not.toBeInTheDocument();
});
```

- [ ] **Step 2: Run test to verify it fails**

```
cd apps/frontend && npm test -- --run AdminPage
```

Expected: FAIL — "Fleet management is not yet available" not found; "agent.up" still present.

- [ ] **Step 3: Rewrite FleetManagementPage**

Replace the entire content of `apps/frontend/src/features/admin/FleetManagementPage.tsx`:

```tsx
import { AdminSurfaceNav } from "./AdminSurfaceNav";
import { Panel } from "../../components/ui/panel";

export function FleetManagementPage() {
  return (
    <section className="page-stack">
      <div className="page-header">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Administration</div>
          <h1>Fleet management</h1>
          <p className="mt-2 max-w-3xl text-sm text-[var(--muted)]">
            View and manage all Observable agents deployed across your infrastructure.
          </p>
        </div>
      </div>

      <AdminSurfaceNav />

      <Panel title="Fleet management is not yet available" eyebrow="Coming soon">
        <p className="max-w-2xl text-sm text-[var(--muted)]">
          When available, this page will show a live inventory of all agents reporting to your
          tenant — host identity, agent type and version, health status, and applied remote
          configuration version. Agent remote configuration and upgrades will be managed here.
        </p>
      </Panel>
    </section>
  );
}
```

- [ ] **Step 4: Run test to verify it passes**

```
cd apps/frontend && npm test -- --run AdminPage
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add apps/frontend/src/features/admin/FleetManagementPage.tsx \
        apps/frontend/src/pages/AdminPage.test.tsx
git commit -m "feat(admin-nav): replace fleet management contract dump with coming-soon panel"
```

---

### Task 4: Fold Identity info into Overview; remove Identity nav entry

The `/admin/identity` page shows 5 read-only Zitadel fields (provider, issuer, OIDC discovery link, redirect URI, SCIM). This is reference info that belongs alongside the RBAC summary, not on a separate page. Inline it into BillingReportPage as a panel; remove the nav tab.

The `/admin/identity` route and `IdentitySettingsPage.tsx` file are kept intact for direct links.

**Files:**
- Modify: `apps/frontend/src/features/admin/BillingReportPage.tsx`
- Modify: `apps/frontend/src/pages/AdminPage.test.tsx`

**Interfaces:**
- Consumes: `window.__OBSERVABLE_ZITADEL_ISSUER__` — same pattern as `IdentitySettingsPage`
- Produces: identity info visible in Overview

- [ ] **Step 1: Write the failing test**

In `apps/frontend/src/pages/AdminPage.test.tsx`, add to the `"renders the tenant usage report for the admin overview"` test:

```typescript
// Identity panel inlined in Overview
expect(screen.getByRole("heading", { name: "Identity provider" })).toBeInTheDocument();
expect(screen.getByText("Zitadel 2.71.x")).toBeInTheDocument();
```

- [ ] **Step 2: Run test to verify it fails**

```
cd apps/frontend && npm test -- --run AdminPage
```

Expected: FAIL — "Identity provider" heading not found in Overview.

- [ ] **Step 3: Add Identity panel to BillingReportPage**

In `apps/frontend/src/features/admin/BillingReportPage.tsx`, add a `Panel` after the Environment scope panel (added in Task 2). Read the issuer from `window.__OBSERVABLE_ZITADEL_ISSUER__` the same way `IdentitySettingsPage` does:

```tsx
const issuer =
  typeof window !== "undefined"
    ? (window as Window & { __OBSERVABLE_ZITADEL_ISSUER__?: string }).__OBSERVABLE_ZITADEL_ISSUER__ ?? "http://localhost:8082"
    : "http://localhost:8082";
```

Add this to the component body (above the return statement, after the existing variable declarations), then append the panel at the end of `<section className="page-stack">`:

```tsx
<Panel title="Identity provider" eyebrow="Auth">
  <div className="overflow-x-auto">
    <table className="min-w-full border-collapse text-left text-sm">
      <tbody>
        <tr>
          <td className="py-1.5 pr-6 font-semibold text-[var(--text-strong)] whitespace-nowrap">Provider</td>
          <td className="py-1.5 text-[var(--text)]">Zitadel 2.71.x</td>
        </tr>
        <tr>
          <td className="py-1.5 pr-6 font-semibold text-[var(--text-strong)] whitespace-nowrap">Issuer URL</td>
          <td className="py-1.5">
            <code className="font-mono text-xs text-[var(--text)]">{issuer}</code>
          </td>
        </tr>
        <tr>
          <td className="py-1.5 pr-6 font-semibold text-[var(--text-strong)] whitespace-nowrap">OIDC Discovery</td>
          <td className="py-1.5">
            <a
              href={`${issuer}/.well-known/openid-configuration`}
              target="_blank"
              rel="noreferrer"
              className="font-mono text-xs text-[var(--brand)] hover:text-[var(--text)]"
            >
              {issuer}/.well-known/openid-configuration
            </a>
          </td>
        </tr>
        <tr>
          <td className="py-1.5 pr-6 font-semibold text-[var(--text-strong)] whitespace-nowrap">Redirect URI</td>
          <td className="py-1.5">
            <code className="font-mono text-xs text-[var(--text)]">
              {typeof window !== "undefined" ? window.location.origin : ""}/auth/callback
            </code>
          </td>
        </tr>
        <tr>
          <td className="py-1.5 pr-6 font-semibold text-[var(--text-strong)] whitespace-nowrap">SCIM 2.0 (planned)</td>
          <td className="py-1.5">
            <code className="font-mono text-xs text-[var(--text)]">{issuer}/scim/v2/&lt;org-id&gt;/</code>
            <span className="ml-2 text-xs text-[var(--muted)]">— enable per-org in Zitadel Admin Console</span>
          </td>
        </tr>
      </tbody>
    </table>
  </div>
</Panel>
```

- [ ] **Step 4: Run test to verify it passes**

```
cd apps/frontend && npm test -- --run AdminPage
```

Expected: PASS — "Identity provider" heading and "Zitadel 2.71.x" found in Overview.

- [ ] **Step 5: Commit**

```bash
git add apps/frontend/src/features/admin/BillingReportPage.tsx \
        apps/frontend/src/pages/AdminPage.test.tsx
git commit -m "feat(admin-nav): inline identity provider info into admin overview"
```

---

### Task 5: Rename Live → Tail in LogSearch; add NLQ caveat

The "Live" button in log search switches to a streaming tail mode that does not apply the active NLQ query filter. Renaming it to "Tail" (industry-standard terminology) clarifies it's a different log-access mode, not just a faster refresh. A small inline notice when both live tail and an NLQ query are active prevents silent filter loss.

**Files:**
- Modify: `apps/frontend/src/pages/LogSearch.tsx`

**Interfaces:**
- Consumes: `isLive: boolean`, `userQuery: string` — both already state in the component
- Produces: renamed button; caveat `<p>` visible only when `isLive && userQuery.trim()`

- [ ] **Step 1: Find the current button (lines ~313–332)**

The button currently reads:

```tsx
<button
  type="button"
  onClick={() => setIsLive((v) => !v)}
  className={[
    "flex items-center gap-1.5 px-2.5 py-1 text-xs font-bold border transition-colors",
    isLive
      ? "border-[var(--bad)] text-[var(--bad)]"
      : "border-[var(--border)] text-[var(--muted)] hover:border-[var(--brand)] hover:text-[var(--text)]",
  ].join(" ")}
  aria-pressed={isLive}
  aria-label={isLive ? "Stop live tail" : "Start live tail"}
>
  {isLive && (
    <span
      className="inline-block w-1.5 h-1.5 rounded-full bg-[var(--bad)] animate-pulse"
      aria-hidden="true"
    />
  )}
  {isLive ? "Stop" : "Live"}
</button>
```

- [ ] **Step 2: Update the button label and add the NLQ caveat**

Replace with:

```tsx
<button
  type="button"
  onClick={() => setIsLive((v) => !v)}
  className={[
    "flex items-center gap-1.5 px-2.5 py-1 text-xs font-bold border transition-colors",
    isLive
      ? "border-[var(--bad)] text-[var(--bad)]"
      : "border-[var(--border)] text-[var(--muted)] hover:border-[var(--brand)] hover:text-[var(--text)]",
  ].join(" ")}
  aria-pressed={isLive}
  aria-label={isLive ? "Stop tail mode" : "Start tail mode"}
>
  {isLive && (
    <span
      className="inline-block w-1.5 h-1.5 rounded-full bg-[var(--bad)] animate-pulse"
      aria-hidden="true"
    />
  )}
  {isLive ? "Stop" : "Tail"}
</button>
```

Then, immediately after the closing `</div>` of the filter row (after the button), add:

```tsx
{isLive && userQuery.trim() && (
  <p className="text-[10px] text-[var(--warn)] px-1">
    NLQ query not applied in tail mode — service and severity filters are active.
  </p>
)}
```

(Check the exact variable name for the NLQ input in this file: it is `userQuery` based on the `useLiveTail` options. If the variable is named differently, use whatever name the file uses for the NLQ/search query string.)

- [ ] **Step 3: Verify the build passes**

```
cd apps/frontend && npm run build 2>&1 | tail -20
```

Expected: no TypeScript errors.

- [ ] **Step 4: Commit**

```bash
git add apps/frontend/src/pages/LogSearch.tsx
git commit -m "feat(logs): rename Live button to Tail; add NLQ-inactive notice in tail mode"
```

---

## Self-Review

**Spec coverage:**
- ✅ Setup/admin split fixed (Task 1)
- ✅ /admin/config deleted, content merged (Task 2)
- ✅ Fleet stub replaced with honest coming-soon (Task 3)
- ✅ Identity folded into Overview, nav tab removed (Task 4)
- ✅ Live button renamed + NLQ caveat (Task 5)

**Type consistency:**
- `BillingReportPage` already imports `Badge`, `MetricCard`, `Panel`, `countTone` — Tasks 2–4 add no new imports
- `issuer` variable declared once in the component body — no conflict with other locals

**Placeholder check:** None found.

**Deferred (out of scope):**
- AdminSurfaceNav Members tab still present — Members page is real, keep it.
- `/admin/identity` route still alive — direct links continue to work.
- NLQ-in-tail-mode backend support — left for a future fetch-backed feature.
