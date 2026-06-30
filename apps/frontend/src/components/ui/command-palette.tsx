import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useRouter } from "@tanstack/react-router";

interface CommandPaletteProps {
  open: boolean;
  onClose: () => void;
}

interface NavResult {
  type: "nav";
  label: string;
  path: string;
}

interface TraceResult {
  type: "trace";
  traceId: string;
}

type Result = NavResult | TraceResult;

const PAGE_ENTRIES: NavResult[] = [
  { type: "nav", label: "Home", path: "/" },
  { type: "nav", label: "Services", path: "/services" },
  { type: "nav", label: "Traces", path: "/traces" },
  { type: "nav", label: "Logs", path: "/logs" },
  { type: "nav", label: "Metrics", path: "/metrics" },
  { type: "nav", label: "Infrastructure", path: "/infrastructure" },
  { type: "nav", label: "Alerts & SLOs", path: "/alerts" },
  { type: "nav", label: "Incidents", path: "/incidents" },
  { type: "nav", label: "Dashboards", path: "/dashboards" },
  { type: "nav", label: "Change Events", path: "/change-events" },
  { type: "nav", label: "Query Workbench", path: "/workbench" },
  { type: "nav", label: "Admin", path: "/admin" },
];

const TRACE_ID_RE = /^[0-9a-f]{16,32}$/i;

function getResults(query: string): Result[] {
  const q = query.trim();

  if (q === "") {
    return PAGE_ENTRIES;
  }

  const results: Result[] = PAGE_ENTRIES.filter((e) =>
    e.label.toLowerCase().includes(q.toLowerCase()),
  );

  if (TRACE_ID_RE.test(q)) {
    results.unshift({ type: "trace", traceId: q });
  }

  return results;
}

export function CommandPalette({ open, onClose }: CommandPaletteProps) {
  const router = useRouter();
  const [query, setQuery] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  // Reset state when opening
  useEffect(() => {
    if (open) {
      setQuery("");
      setActiveIndex(0);
      // Focus the input after the portal renders
      requestAnimationFrame(() => {
        inputRef.current?.focus();
      });
    }
  }, [open]);

  // Escape key listener on document
  useEffect(() => {
    if (!open) return;

    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") {
        onClose();
      }
    }

    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [open, onClose]);

  const results = getResults(query);

  function activate(result: Result) {
    if (result.type === "nav") {
      router.navigate({ to: result.path });
    } else {
      router.navigate({ to: `/traces/${result.traceId}` });
    }
    onClose();
  }

  function handleDialogKeyDown(e: React.KeyboardEvent) {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActiveIndex((i) => Math.min(i + 1, results.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActiveIndex((i) => Math.max(i - 1, 0));
    } else if (e.key === "Enter") {
      if (results[activeIndex]) {
        activate(results[activeIndex]);
      }
    } else if (e.key === "Tab") {
      // Trap focus within dialog — there's only the input + list, keep focus on input
      e.preventDefault();
    }
  }

  if (!open) return null;

  return createPortal(
    <>
      {/* Backdrop */}
      <div
        className="fixed inset-0 bg-black/40 z-50"
        onClick={onClose}
        aria-hidden="true"
      />

      {/* Dialog */}
      <div
        role="dialog"
        aria-label="Command palette"
        aria-modal="true"
        className="fixed top-[20%] left-1/2 -translate-x-1/2 w-full max-w-lg bg-[var(--surface)] border border-[var(--border)] rounded shadow-xl z-50"
        onKeyDown={handleDialogKeyDown}
      >
        <input
          ref={inputRef}
          role="combobox"
          aria-autocomplete="list"
          aria-expanded={results.length > 0}
          aria-controls="command-palette-listbox"
          className="w-full p-3 text-sm bg-transparent border-b border-[var(--border)] outline-none"
          placeholder="Search pages or paste a trace ID…"
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            setActiveIndex(0);
          }}
        />

        <ul
          id="command-palette-listbox"
          role="listbox"
          className="max-h-64 overflow-y-auto"
        >
          {results.length === 0 ? (
            <li className="px-4 py-3 text-sm text-[var(--muted)]">No results</li>
          ) : (
            results.map((result, i) => {
              const label =
                result.type === "nav"
                  ? result.label
                  : `Go to trace: ${result.traceId}`;
              const isActive = i === activeIndex;

              return (
                <li
                  key={label}
                  role="option"
                  aria-selected={isActive}
                  className={
                    "px-4 py-2 text-sm cursor-pointer" +
                    (isActive ? " bg-[var(--surface-subtle)]" : "")
                  }
                  onClick={() => activate(result)}
                  onMouseEnter={() => setActiveIndex(i)}
                >
                  {label}
                </li>
              );
            })
          )}
        </ul>
      </div>
    </>,
    document.body,
  );
}
