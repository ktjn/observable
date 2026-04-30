# UI Uplift — Bloomberg Dense + Phosphor Green

**Date:** 2026-04-30
**Status:** Approved

## Context

The current UI uses a soft, rounded aesthetic (8px border-radius on panels, 6px on controls, Inter typeface, pastel status colors). As the product matures and carries more information per screen, the visual style has started to feel at odds with the density the data demands. The goal is to shift the entire UI toward a utilitarian, information-dense look: sharp edges, ruled lines, compressed type, and a monochrome-green dark mode reminiscent of phosphor CRT terminals.

## Design Principles

1. **Zero border-radius** — All panels, inputs, buttons, badges, and controls use `border-radius: 0`. No exceptions.
2. **Ruled lines over shadows** — Borders replace drop-shadows as the primary separator. Shadows are removed entirely.
3. **Type hierarchy through weight and case, not size variation** — Body text at 11px, secondary/labels at 9–10px. Uppercase + letter-spacing for section labels.
4. **Information density first** — Tighter padding (2–4px cells in tables, 3–5px in controls). Row height shrinks to content.
5. **Monochrome palettes** — Light mode: blacks, grays, whites. Dark mode: greens only (with red/amber preserved for error/warn signal only).

## Typography

| Role | Font | Size | Weight |
|------|------|------|--------|
| Body / UI | IBM Plex Sans Condensed | 11px | 400 |
| Labels / nav | IBM Plex Sans Condensed | 10–11px | 600 |
| Section headers | IBM Plex Sans Condensed | 9px uppercase + letter-spacing | 700 |
| Timestamps / values / code | IBM Plex Mono | 9–11px | 400 |
| Metric values | IBM Plex Mono | 18px | 700 |

Load via Google Fonts: `IBM+Plex+Sans+Condensed:wght@400;500;600;700` + `IBM+Plex+Mono:wght@400;600`.

## Color Tokens

### Light Mode (Bloomberg Dense)

| Token | Value | Usage |
|-------|-------|-------|
| `--bg` | `#f2f2f2` | Page background |
| `--surface` | `#e8e8e8` | Sidebar, topbar |
| `--surface-subtle` | `#e4e4e4` | Sidebar background |
| `--surface-raised` | `#ffffff` | Input backgrounds, panel bodies |
| `--surface-inset` | `#ebebeb` | Zebra-stripe even rows |
| `--border` | `#cccccc` | Thin dividers |
| `--border-strong` | `#999999` | Panel/sidebar borders |
| `--text` | `#111111` | Primary text |
| `--text-strong` | `#000000` | Headers, active nav |
| `--muted` | `#666666` | Timestamps, labels, secondary text |
| `--brand` | `#111111` | Active state, logo bg, table header bg |
| `--brand-bg` | `#d8d8d8` | Active nav background |
| `--good` | `#007700` | Success text |
| `--good-bg` | `#f0f0f0` | (unused in dense mode — use border badge) |
| `--warn` | `#a05000` | Warning text |
| `--bad` | `#bb0000` | Error text |
| `--shadow-panel` | `none` | Removed |
| `--shadow-control` | `none` | Removed |

### Dark Mode (Phosphor Green)

| Token | Value | Usage |
|-------|-------|-------|
| `--bg` | `#050f05` | Page background |
| `--surface` | `#070f07` | Sidebar, topbar |
| `--surface-subtle` | `#0a1f0a` | Active nav bg, table header bg |
| `--surface-raised` | `#020a02` | Input bg, zebra even rows |
| `--surface-inset` | `#030a03` | Deepest inset surfaces |
| `--border` | `#0d2a0d` | Thin row dividers |
| `--border-strong` | `#1a5a1a` | Panel/sidebar borders, input borders |
| `--text` | `#33ff33` | Primary text, active items |
| `--text-strong` | `#33ff33` | Same — green is the only "strong" |
| `--muted` | `#1a9a1a` | Secondary text, labels, inactive nav |
| `--brand` | `#33ff33` | Active nav indicator, logo border |
| `--brand-bg` | `#0a1f0a` | Active nav background |
| `--good` | `#1a9a1a` | Success (dim green — not louder than primary) |
| `--warn` | `#ffaa00` | Warning — amber preserved for signal |
| `--bad` | `#ff5555` | Error — red preserved for signal |
| `--shadow-panel` | `none` | Removed |
| `--shadow-control` | `none` | Removed |

## Component Changes

### Sidebar
- Width: 140px (down from 260px)
- Logo: black pill with white text (light) / outlined green box (dark)
- Nav items: no icon boxes, text-only with left-border active indicator (`border-left: 2px solid var(--brand)`)
- Nav font: 11px, active = 600 weight
- Footer: monospace, user/env context

### Topbar
- Height: reduced from 72px min to ~32px
- Border: `2px solid var(--text)` (light) / `1px solid var(--border-strong)` (dark)
- Title: 11px uppercase + letter-spacing
- Controls: flat bordered pills, no background tint

### Panels (`.modern-panel`)
- `border-radius: 0`
- `box-shadow: none`
- Panel header: `background: var(--surface)`, `border-bottom: 1px solid var(--border-strong)`
- Header text: 9px uppercase, letter-spacing 1px

### Tables
- Header row: `background: var(--brand)` + inverted text (light) / `background: var(--surface-subtle)` (dark)
- Cell padding: `2px 5px`
- Row divider: `1px solid var(--border)`
- Zebra: even rows get `background: var(--surface-inset)`
- `th` font-size: 9px, font-weight: 400, letter-spacing: 0.5px

### Buttons
- `border-radius: 0`
- `box-shadow: none`
- Primary: `background: var(--text)`, `color: var(--bg)` (inverted)
- Secondary: `border: 1px solid var(--border-strong)`, `background: var(--surface)`
- Font: IBM Plex Sans Condensed, 11px

### Inputs / Selects
- `border-radius: 0`
- `border: 1px solid var(--border-strong)`
- `background: var(--surface-raised)`
- Font: IBM Plex Mono, 11px (query/filter inputs) or IBM Plex Sans Condensed (form inputs)

### Badges / Status chips
- `border-radius: 0`
- Style: `border: 1px solid <color>`, `color: <color>`, no background fill
- ERR: `--bad` color; WRN: `--warn`; INF/OK: `--muted`
- Font: 8–9px, uppercase

### Metric Cards
- Value: IBM Plex Mono, 18px, bold
- Label: 9px uppercase, `--muted` color
- Dividers between metrics: `border-right: 1px solid var(--border)`
- No card background — inline in panel body

### Segmented controls / Tabs
- `border-radius: 0`
- Active segment: inverted (black bg / white text in light; green text + bg tint in dark)

## Scope

This uplift touches:
- `apps/frontend/src/styles.css` — CSS variables, global element styles, layout classes
- `apps/frontend/src/components/AppShell.tsx` — sidebar width, nav markup (remove icon boxes)
- `apps/frontend/src/components/ui/button.tsx`
- `apps/frontend/src/components/ui/badge.tsx`
- `apps/frontend/src/components/ui/panel.tsx`
- `apps/frontend/src/components/ui/input.tsx`
- `apps/frontend/src/components/ui/select.tsx`
- `apps/frontend/src/components/ui/tabs.tsx`
- `apps/frontend/index.html` — Google Fonts `<link>` tags

All page-level components inherit the changes via CSS variables and shared component classes. No page-level changes should be required unless a page hardcodes a border-radius or color.

## What Stays Unchanged

- Error/warn/success **functional colors** (red/amber/green) are preserved in both modes for signal clarity
- Dark mode toggle remains (light/dark/system)
- Layout structure (sidebar + topbar + content shell)
- TanStack Router, React Query, all logic
- Responsive breakpoints

## Verification

1. Start dev server: `cd apps/frontend && npm run dev`
2. Visually check every page in light and dark mode:
   - Sidebar nav, active states, logo
   - Topbar height and controls
   - Log search page (table, search input, badges)
   - Service detail page (panels, metric cards, tabs)
   - Trace search / trace detail
   - Alerts, dashboards, setup pages
3. Confirm zero rounded corners anywhere in the UI
4. Confirm IBM Plex loads (check Network tab — two font requests)
5. Run `npm run typecheck && npm run lint` — no new errors
6. Run `npm test` — existing component tests pass
