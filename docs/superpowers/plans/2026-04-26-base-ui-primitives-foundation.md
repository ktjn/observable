# Base UI Primitives Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the first shared primitive layer for the frontend by wiring Tailwind CSS v4 and Base UI into `apps/frontend`, preserving the existing theme contract, and shipping owned `Button`, `Input`, `Select`, and `Tabs` primitives with focused tests.

**Architecture:** Keep the current app shell, routes, and page-level styling intact while introducing a parallel primitive foundation under `src/components/ui`. Tailwind v4 will power only the new primitives in this slice, and all theme-aware styling will continue to derive from the existing CSS variable contract driven by `ThemeProvider`.

**Tech Stack:** React 19, TypeScript, Vite 8, Vitest, Tailwind CSS v4 with `@tailwindcss/vite`, Base UI React component package with subpath imports such as `@base-ui/react/tabs`.

---

### Task 1: Add frontend dependency and build foundation

**Files:**
- Modify: `apps/frontend/package.json`
- Modify: `apps/frontend/vite.config.ts`
- Modify: `apps/frontend/src/styles.css`
- Test: `apps/frontend/package.json` scripts via `npm run typecheck`, `npm run lint`, `npm run test`, `npm run build`

- [ ] **Step 1: Write the failing dependency/setup expectation**

Document the exact missing pieces before changing code:

```text
Current gaps:
- package.json does not include tailwindcss
- package.json does not include @tailwindcss/vite
- package.json does not include the Base UI package
- vite.config.ts does not register the Tailwind plugin
- styles.css does not import Tailwind
```

- [ ] **Step 2: Verify the current frontend dependency graph lacks the required packages**

Run:

```bash
npm ls --workspace=apps/frontend tailwindcss @tailwindcss/vite @base-ui/react
```

Expected:

```text
missing dependencies / non-zero exit because none of the packages are installed yet
```

- [ ] **Step 3: Update `apps/frontend/package.json` with the new dependencies**

Apply this shape:

```json
{
  "dependencies": {
    "@base-ui/react": "^1.3.0",
    "@tanstack/react-query": "^5.0.0",
    "@tanstack/react-router": "^1.0.0",
    "react": "^19.0.0",
    "react-dom": "^19.0.0"
  },
  "devDependencies": {
    "@eslint/js": "^10.0.1",
    "@tailwindcss/vite": "^4.1.0",
    "@testing-library/jest-dom": "^6.0.0",
    "@testing-library/react": "^16.0.0",
    "@types/react": "^19.0.0",
    "@types/react-dom": "^19.0.0",
    "@typescript-eslint/eslint-plugin": "^8.0.0",
    "@typescript-eslint/parser": "^8.0.0",
    "@vitejs/plugin-react": "^6.0.1",
    "@vitest/ui": "^4.1.4",
    "eslint": "^10.2.1",
    "jsdom": "^29.0.2",
    "tailwindcss": "^4.1.0",
    "typescript": "^6.0.3",
    "vite": "^8.0.8",
    "vitest": "^4.1.4"
  }
}
```

Notes:

- keep the existing scripts unchanged in this slice
- do not add PostCSS config; Tailwind v4 uses the Vite plugin directly

- [ ] **Step 4: Install dependencies**

Run:

```bash
npm install --workspace=apps/frontend
```

Expected:

```text
lockfile updated and install succeeds with tailwindcss, @tailwindcss/vite, and @base-ui/react present
```

- [ ] **Step 5: Register the Tailwind Vite plugin**

Update `apps/frontend/vite.config.ts` to this structure:

```ts
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    port: 5173,
    proxy: {
      "/v1": { target: "http://localhost:8090", changeOrigin: true },
    },
  },
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/test-setup.ts"],
  },
});
```

- [ ] **Step 6: Convert `src/styles.css` into the Tailwind v4 entry while preserving theme tokens**

At the top of `apps/frontend/src/styles.css`, add:

```css
@import "tailwindcss";

:root {
  color-scheme: light;
  --bg: #f6f7f9;
  --surface: #ffffff;
  --surface-subtle: #eef1f5;
  --border: #d8dee7;
  --text: #17202a;
  --muted: #5f6b7a;
  --brand: #2563eb;
  --brand-strong: #1d4ed8;
  --good: #15803d;
  --good-bg: #e8f7ee;
  --warn: #b45309;
  --warn-bg: #fff4df;
  --bad: #b91c1c;
  --bad-bg: #fdeaea;
  --info-bg: #e8f1ff;
  --focus-ring: color-mix(in srgb, var(--brand) 70%, white);
  font-family:
    Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
}

:root[data-theme="dark"] {
  color-scheme: dark;
  --bg: #111318;
  --surface: #191d24;
  --surface-subtle: #232935;
  --border: #333b49;
  --text: #edf1f7;
  --muted: #a7b0bd;
  --brand: #60a5fa;
  --brand-strong: #93c5fd;
  --good: #4ade80;
  --good-bg: #123420;
  --warn: #fbbf24;
  --warn-bg: #3a2a0a;
  --bad: #f87171;
  --bad-bg: #3a1414;
  --info-bg: #12233f;
  --focus-ring: color-mix(in srgb, var(--brand) 70%, black);
}
```

Keep the existing legacy layout and page classes below those token definitions. Do not attempt a broad rewrite of existing selectors in this task.

- [ ] **Step 7: Run the frontend build once to catch configuration-level failures**

Run:

```bash
npm run build --workspace=apps/frontend
```

Expected:

```text
TypeScript and Vite build complete successfully with Tailwind plugin enabled
```

- [ ] **Step 8: Commit the foundation setup**

Run:

```bash
git add apps/frontend/package.json apps/frontend/package-lock.json apps/frontend/vite.config.ts apps/frontend/src/styles.css
git commit -m "feat(frontend): add base ui foundation"
```

### Task 2: Add shared UI utilities and owned primitive files

**Files:**
- Create: `apps/frontend/src/components/ui/button.tsx`
- Create: `apps/frontend/src/components/ui/input.tsx`
- Create: `apps/frontend/src/components/ui/select.tsx`
- Create: `apps/frontend/src/components/ui/tabs.tsx`
- Create: `apps/frontend/src/components/ui/cn.ts`
- Test: primitive unit/component tests added in Task 3

- [ ] **Step 1: Write the failing primitive-module expectation**

Run:

```bash
rg --files apps/frontend/src/components/ui
```

Expected:

```text
no files found because the ui directory does not exist yet
```

- [ ] **Step 2: Create the shared class utility**

Add `apps/frontend/src/components/ui/cn.ts`:

```ts
export function cn(...values: Array<string | false | null | undefined>): string {
  return values.filter(Boolean).join(" ");
}
```

- [ ] **Step 3: Create the shared Button primitive**

Add `apps/frontend/src/components/ui/button.tsx`:

```tsx
import type { ButtonHTMLAttributes } from "react";
import { cn } from "./cn";

type ButtonVariant = "primary" | "secondary" | "destructive";

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
}

const variantClasses: Record<ButtonVariant, string> = {
  primary:
    "bg-[var(--brand)] text-white hover:bg-[var(--brand-strong)] disabled:bg-[var(--surface-subtle)] disabled:text-[var(--muted)]",
  secondary:
    "bg-[var(--surface)] text-[var(--text)] border border-[var(--border)] hover:bg-[var(--surface-subtle)] disabled:text-[var(--muted)]",
  destructive:
    "bg-[var(--bad)] text-white hover:opacity-90 disabled:bg-[var(--surface-subtle)] disabled:text-[var(--muted)]",
};

export function Button({
  className,
  type = "button",
  variant = "primary",
  ...props
}: ButtonProps) {
  return (
    <button
      type={type}
      className={cn(
        "inline-flex min-h-9 items-center justify-center rounded-md px-3 text-sm font-semibold transition-colors outline-none",
        "focus-visible:ring-2 focus-visible:ring-[var(--focus-ring)] focus-visible:ring-offset-2 focus-visible:ring-offset-[var(--bg)]",
        "disabled:cursor-not-allowed",
        variantClasses[variant],
        className
      )}
      {...props}
    />
  );
}
```

- [ ] **Step 4: Create the shared Input primitive**

Add `apps/frontend/src/components/ui/input.tsx`:

```tsx
import { forwardRef } from "react";
import type { InputHTMLAttributes } from "react";
import { cn } from "./cn";

export type InputProps = InputHTMLAttributes<HTMLInputElement>;

export const Input = forwardRef<HTMLInputElement, InputProps>(function Input(
  { className, type = "text", ...props },
  ref
) {
  return (
    <input
      ref={ref}
      type={type}
      className={cn(
        "flex min-h-9 w-full rounded-md border border-[var(--border)] bg-[var(--surface)] px-3 text-sm text-[var(--text)] outline-none transition-colors",
        "placeholder:text-[var(--muted)]",
        "focus-visible:ring-2 focus-visible:ring-[var(--focus-ring)] focus-visible:ring-offset-2 focus-visible:ring-offset-[var(--bg)]",
        "disabled:cursor-not-allowed disabled:opacity-60",
        className
      )}
      {...props}
    />
  );
});
```

- [ ] **Step 5: Create the owned Select primitive with a native backing element**

Add `apps/frontend/src/components/ui/select.tsx`:

```tsx
import { forwardRef } from "react";
import type { OptionHTMLAttributes, SelectHTMLAttributes } from "react";
import { cn } from "./cn";

export interface SelectProps extends SelectHTMLAttributes<HTMLSelectElement> {}

export interface SelectOptionProps extends OptionHTMLAttributes<HTMLOptionElement> {}

const triggerClasses =
  "flex min-h-9 w-full rounded-md border border-[var(--border)] bg-[var(--surface)] px-3 text-sm text-[var(--text)] outline-none transition-colors focus-visible:ring-2 focus-visible:ring-[var(--focus-ring)] focus-visible:ring-offset-2 focus-visible:ring-offset-[var(--bg)] disabled:cursor-not-allowed disabled:opacity-60";

export const Select = forwardRef<HTMLSelectElement, SelectProps>(function Select(
  { className, children, ...props },
  ref
) {
  return (
    <select ref={ref} className={cn(triggerClasses, className)} {...props}>
      {children}
    </select>
  );
});

export function SelectOption(props: SelectOptionProps) {
  return <option {...props} />;
}
```

Note:

- keep this deliberately simple in the first slice
- do not introduce popup positioning or custom listbox behavior here

- [ ] **Step 6: Create the owned Tabs primitive backed by Base UI**

Add `apps/frontend/src/components/ui/tabs.tsx`:

```tsx
import { Tabs as BaseTabs } from "@base-ui/react/tabs";
import type { ComponentPropsWithoutRef } from "react";
import { cn } from "./cn";

export function TabsRoot(props: BaseTabs.Root.Props) {
  return <BaseTabs.Root {...props} />;
}

export function TabsList({
  className,
  ...props
}: ComponentPropsWithoutRef<typeof BaseTabs.List>) {
  return (
    <BaseTabs.List
      className={cn(
        "relative inline-flex min-h-10 items-center gap-1 rounded-lg border border-[var(--border)] bg-[var(--surface-subtle)] p-1",
        className
      )}
      {...props}
    />
  );
}

export function TabsTab({
  className,
  ...props
}: ComponentPropsWithoutRef<typeof BaseTabs.Tab>) {
  return (
    <BaseTabs.Tab
      className={cn(
        "inline-flex min-h-8 items-center justify-center rounded-md px-3 text-sm font-medium text-[var(--muted)] outline-none transition-colors",
        "data-[selected]:bg-[var(--surface)] data-[selected]:text-[var(--text)]",
        "focus-visible:ring-2 focus-visible:ring-[var(--focus-ring)] focus-visible:ring-offset-2 focus-visible:ring-offset-[var(--bg)]",
        className
      )}
      {...props}
    />
  );
}

export function TabsPanel({
  className,
  ...props
}: ComponentPropsWithoutRef<typeof BaseTabs.Panel>) {
  return (
    <BaseTabs.Panel className={cn("outline-none", className)} {...props} />
  );
}

export const Tabs = {
  Root: TabsRoot,
  List: TabsList,
  Tab: TabsTab,
  Panel: TabsPanel,
};
```

- [ ] **Step 7: Verify the new modules type-check in isolation**

Run:

```bash
npm run typecheck --workspace=apps/frontend
```

Expected:

```text
TypeScript passes with the new ui primitives compiled successfully
```

- [ ] **Step 8: Commit the primitive files**

Run:

```bash
git add apps/frontend/src/components/ui
git commit -m "feat(frontend): add shared ui primitives"
```

### Task 3: Add focused primitive tests

**Files:**
- Create: `apps/frontend/src/components/ui/button.test.tsx`
- Create: `apps/frontend/src/components/ui/input.test.tsx`
- Create: `apps/frontend/src/components/ui/select.test.tsx`
- Create: `apps/frontend/src/components/ui/tabs.test.tsx`
- Modify: `apps/frontend/src/test-setup.ts` if keyboard or focus helpers are needed
- Test: `npm run test --workspace=apps/frontend`

- [ ] **Step 1: Write the failing button test**

Add `apps/frontend/src/components/ui/button.test.tsx`:

```tsx
import { render, screen } from "@testing-library/react";
import { Button } from "./button";

test("renders button content and default type", () => {
  render(<Button>Save</Button>);
  const button = screen.getByRole("button", { name: "Save" });
  expect(button).toHaveAttribute("type", "button");
});

test("applies disabled state", () => {
  render(<Button disabled>Delete</Button>);
  expect(screen.getByRole("button", { name: "Delete" })).toBeDisabled();
});
```

- [ ] **Step 2: Write the failing input test**

Add `apps/frontend/src/components/ui/input.test.tsx`:

```tsx
import { render, screen } from "@testing-library/react";
import { Input } from "./input";

test("renders a text input with placeholder", () => {
  render(<Input placeholder="Search services" />);
  expect(screen.getByPlaceholderText("Search services")).toHaveAttribute("type", "text");
});

test("respects disabled state", () => {
  render(<Input aria-label="Search" disabled />);
  expect(screen.getByRole("textbox", { name: "Search" })).toBeDisabled();
});
```

- [ ] **Step 3: Write the failing select test**

Add `apps/frontend/src/components/ui/select.test.tsx`:

```tsx
import { fireEvent, render, screen } from "@testing-library/react";
import { Select, SelectOption } from "./select";

test("renders options and updates value", () => {
  render(
    <Select aria-label="Environment" defaultValue="dev">
      <SelectOption value="dev">dev</SelectOption>
      <SelectOption value="prod">prod</SelectOption>
    </Select>
  );

  const select = screen.getByRole("combobox", { name: "Environment" });
  expect(select).toHaveValue("dev");

  fireEvent.change(select, { target: { value: "prod" } });
  expect(select).toHaveValue("prod");
});
```

- [ ] **Step 4: Write the failing tabs test**

Add `apps/frontend/src/components/ui/tabs.test.tsx`:

```tsx
import { fireEvent, render, screen } from "@testing-library/react";
import { Tabs } from "./tabs";

function TestTabs() {
  return (
    <Tabs.Root defaultValue="logs">
      <Tabs.List aria-label="Service signals">
        <Tabs.Tab value="logs">Logs</Tabs.Tab>
        <Tabs.Tab value="metrics">Metrics</Tabs.Tab>
      </Tabs.List>
      <Tabs.Panel value="logs">Logs panel</Tabs.Panel>
      <Tabs.Panel value="metrics">Metrics panel</Tabs.Panel>
    </Tabs.Root>
  );
}

test("renders the default tab panel", () => {
  render(<TestTabs />);
  expect(screen.getByText("Logs panel")).toBeVisible();
});

test("switches tabs by click", () => {
  render(<TestTabs />);
  fireEvent.click(screen.getByRole("tab", { name: "Metrics" }));
  expect(screen.getByText("Metrics panel")).toBeVisible();
});

test("supports keyboard navigation", () => {
  render(<TestTabs />);
  const logs = screen.getByRole("tab", { name: "Logs" });
  logs.focus();
  fireEvent.keyDown(logs, { key: "ArrowRight" });
  expect(screen.getByRole("tab", { name: "Metrics" })).toHaveFocus();
});
```

- [ ] **Step 5: Run the primitive test subset and make them pass**

Run:

```bash
npm run test --workspace=apps/frontend -- src/components/ui/button.test.tsx src/components/ui/input.test.tsx src/components/ui/select.test.tsx src/components/ui/tabs.test.tsx
```

Expected:

```text
all four primitive test files pass
```

- [ ] **Step 6: Add a theme compatibility test for the primitive layer**

Append this case to `apps/frontend/src/components/ui/button.test.tsx`:

```tsx
test("renders under the dark theme contract", () => {
  document.documentElement.dataset.theme = "dark";
  render(<Button>Dark action</Button>);
  expect(screen.getByRole("button", { name: "Dark action" })).toBeInTheDocument();
  delete document.documentElement.dataset.theme;
});
```

This test is intentionally small: it verifies compatibility with the current theme contract rather than pixel styling.

- [ ] **Step 7: Run the full frontend test suite**

Run:

```bash
npm run test --workspace=apps/frontend
```

Expected:

```text
existing frontend tests and new primitive tests all pass
```

- [ ] **Step 8: Commit the tests**

Run:

```bash
git add apps/frontend/src/components/ui/*.test.tsx
git commit -m "test(frontend): cover shared ui primitives"
```

### Task 4: Add one narrow live-app integration touchpoint if needed

**Files:**
- Modify: `apps/frontend/src/components/AppShell.tsx` only if needed
- Modify: one page component only if needed
- Test: existing affected frontend tests

- [x] **Step 1: Check whether a live-app integration touchpoint is necessary**

Use this rule:

```text
If the primitive tests already prove compilation, behavior, and theme compatibility, skip this task entirely.
Only take it if there is unresolved risk that the primitives are not consumable from the real app shell.
```

Status:
- skipped on `ui-base-ui-primitives-foundation`
- reason: the branch already contains the Tailwind/Base UI foundation plus focused primitive tests and theme-compatibility coverage, with no unresolved evidence that the primitives are not consumable from the existing app shell

- [x] **Step 2: If needed, migrate exactly one low-risk usage site**

Recommended target:

```text
replace one search input or one button in a low-risk surface, but do not migrate an entire page
```

Example shape for a search field migration:

```tsx
import { Input } from "../components/ui/input";

<Input
  className="max-w-sm"
  value={query}
  onChange={(event) => setQuery(event.target.value)}
  placeholder="Search services"
  aria-label="Search services"
/>
```

Status: not needed for this slice; no live-app usage site was migrated.

- [x] **Step 3: Run only the directly affected test file**

Run:

```bash
npm run test --workspace=apps/frontend -- src/App.test.tsx
```

Expected:

```text
the existing integration test still passes after the narrow primitive adoption
```

Status: skipped because no live-app integration touchpoint was taken.

- [x] **Step 4: Commit the optional integration touchpoint**

Run:

```bash
git add apps/frontend/src/components/AppShell.tsx apps/frontend/src/pages
git commit -m "refactor(frontend): adopt shared ui primitive"
```

Status: no commit required; optional touchpoint was intentionally skipped.

### Task 5: Run required verification and update planning docs

**Files:**
- Modify: `docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md`
- Test: `bash scripts/local-ci.sh`

- [x] **Step 1: Update the active roadmap plan with the slice outcome**

Add a completed or in-progress note under the appropriate frontend migration area in `docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md`.

Use this content shape:

```md
- [x] **UI-Followup: Base UI primitive foundation**
  - Outcome: `apps/frontend` now includes Tailwind CSS v4 and an owned `src/components/ui` layer with shared `Button`, `Input`, `Select`, and `Tabs` primitives. Existing theme preference behavior remains unchanged because primitives read the current CSS variable contract.
  - Checkpoint: can future screen migrations adopt the new primitive layer without reopening dependency and token setup? Answer: yes. The Vite/Tailwind pipeline, theme tokens, and owned primitive wrappers are now in place.
```

Place it in the nearest relevant frontend sequencing section rather than inventing a new top-level phase.

- [x] **Step 2: Run the narrowest frontend checks first**

Run:

```bash
npm run typecheck --workspace=apps/frontend
npm run lint --workspace=apps/frontend
npm run test --workspace=apps/frontend
npm run build --workspace=apps/frontend
```

Expected:

```text
all four frontend checks pass
```

- Status: `npm run typecheck --workspace=apps/frontend`, `npm run lint --workspace=apps/frontend`, `npm run test --workspace=apps/frontend`, and `npm run build --workspace=apps/frontend` all passed on `ui-base-ui-primitives-foundation` after `081ce9be60b6e35dd3d47f2c8cfe95e559111cde` fixed the primitive lint rule issue.

- [ ] **Step 3: Run the mandatory repository gate before any push**

Run:

```bash
bash scripts/local-ci.sh
```

Expected:

```text
full local gate passes; no frontend, Docker, Rust, or smoke regressions remain
```

- Status: this step is intentionally left as a pre-push gate, not a final branch verdict. Earlier in-branch smoke-test failure notes were transient and are not the authoritative final status for `ui-base-ui-primitives-foundation`; use fresh verification at the branch tip before any push.

- [x] **Step 4: Review the final diff**

Run:

```bash
git status --short
git diff --stat HEAD~3..HEAD
```

Expected:

```text
only the intended frontend foundation files and plan update remain in the branch
```

- [x] **Step 5: Commit the roadmap-plan sync**

Run:

```bash
git add docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md
git commit -m "docs(plan): record base ui foundation slice"
```

## Self-Review

### Spec coverage

Covered requirements from `docs/superpowers/specs/2026-04-26-base-ui-primitives-foundation-design.md`:

- Base UI + Tailwind v4 installation: Task 1
- preserve current theme contract: Task 1 and Task 3
- owned `components/ui` layer: Task 2
- primitive set limited to `Button`, `Input`, `Select`, `Tabs`: Task 2
- focused testing only: Task 3
- no broad screen migration: enforced in Task 4 and Task 5

No uncovered design requirement remains.

### Placeholder scan

The plan intentionally avoids placeholders for package names, files, commands, and primitive code shape. The only conditional branch is Task 4, which is explicitly optional and bounded to one low-risk usage site.

### Type consistency

The primitive names are consistent across all tasks:

- `Button`
- `Input`
- `Select`
- `SelectOption`
- `Tabs.Root`
- `Tabs.List`
- `Tabs.Tab`
- `Tabs.Panel`

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-04-26-base-ui-primitives-foundation.md`. Two execution options:

**1. Subagent-Driven (recommended)** - I dispatch a fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** - Execute tasks in this session using executing-plans, batch execution with checkpoints

Which approach?
