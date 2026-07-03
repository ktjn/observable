import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Button } from "../../../components/ui/button";
import {
  createSavedView,
  deleteSavedView,
  fetchSavedViews,
  type LogViewConfig,
} from "../../../api/savedViews";

export interface SavedViewsControlProps {
  tenantId: string;
  currentConfig: LogViewConfig;
  onLoad: (config: LogViewConfig) => void;
}

export function SavedViewsControl({ tenantId, currentConfig, onLoad }: SavedViewsControlProps) {
  const [isOpen, setIsOpen] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [newViewName, setNewViewName] = useState("");
  const queryClient = useQueryClient();

  const { data } = useQuery({
    queryKey: ["saved-views", tenantId, "logs"],
    queryFn: () => fetchSavedViews(tenantId, "logs"),
    enabled: isOpen,
  });
  const views = data?.items ?? [];

  const createMutation = useMutation({
    mutationFn: (name: string) =>
      createSavedView(tenantId, { name, signal_kind: "logs", config: currentConfig }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["saved-views", tenantId, "logs"] });
      setIsSaving(false);
      setNewViewName("");
    },
  });

  const deleteMutation = useMutation({
    mutationFn: (savedViewId: string) => deleteSavedView(tenantId, savedViewId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["saved-views", tenantId, "logs"] });
    },
  });

  return (
    <div className="relative">
      <Button variant="secondary" onClick={() => setIsOpen((v) => !v)}>
        Saved Views
      </Button>
      {isOpen && (
        <div className="absolute z-10 mt-1 w-72 border border-[var(--border)] bg-[var(--surface)] p-2 shadow-lg">
          <ul className="max-h-60 overflow-y-auto">
            {views.map((view) => (
              <li key={view.saved_view_id} className="flex items-center justify-between gap-2 py-1">
                <button
                  type="button"
                  className="flex-1 text-left text-sm hover:text-[var(--brand)]"
                  onClick={() => {
                    onLoad(view.config);
                    setIsOpen(false);
                  }}
                >
                  {view.name}
                </button>
                <button
                  type="button"
                  aria-label={`Delete ${view.name}`}
                  className="text-xs text-[var(--muted)] hover:text-[var(--bad)]"
                  onClick={() => deleteMutation.mutate(view.saved_view_id)}
                >
                  Delete
                </button>
              </li>
            ))}
            {views.length === 0 && (
              <li className="py-1 text-xs text-[var(--muted)]">No saved views yet.</li>
            )}
          </ul>
          <div className="mt-2 border-t border-[var(--border)] pt-2">
            {isSaving ? (
              <div className="flex flex-col gap-2">
                <label className="text-xs text-[var(--muted)]" htmlFor="saved-view-name">
                  View name
                </label>
                <input
                  id="saved-view-name"
                  type="text"
                  value={newViewName}
                  onChange={(e) => setNewViewName(e.target.value)}
                  className="border border-[var(--border)] bg-[var(--surface)] px-2 py-1 text-xs"
                />
                <Button
                  variant="primary"
                  disabled={!newViewName.trim() || createMutation.isPending}
                  onClick={() => createMutation.mutate(newViewName.trim())}
                >
                  Save
                </Button>
              </div>
            ) : (
              <Button variant="secondary" onClick={() => setIsSaving(true)}>
                Save current view
              </Button>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
