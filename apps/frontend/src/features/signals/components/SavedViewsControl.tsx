import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Button } from "../../../components/ui/button";
import {
  addSavedViewGrant,
  createSavedView,
  deleteSavedView,
  fetchSavedViewGrants,
  fetchSavedViews,
  revokeSavedViewGrant,
  updateSavedView,
  type LogViewConfig,
  type SavedView,
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
  const [managingViewId, setManagingViewId] = useState<string | null>(null);
  const [newGrantUserId, setNewGrantUserId] = useState("");
  const [newGrantRelation, setNewGrantRelation] = useState<"owner" | "editor" | "viewer">("viewer");
  const queryClient = useQueryClient();

  const { data } = useQuery({
    queryKey: ["saved-views", tenantId, "logs"],
    queryFn: () => fetchSavedViews(tenantId, "logs"),
    enabled: isOpen,
  });
  const views = data?.items ?? [];
  const managingView = views.find((v) => v.saved_view_id === managingViewId) ?? null;

  const { data: grantsData } = useQuery({
    queryKey: ["saved-view-grants", tenantId, managingViewId],
    queryFn: () => fetchSavedViewGrants(tenantId, managingViewId as string),
    enabled: managingViewId !== null,
  });
  const grants = grantsData?.grants ?? [];

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

  const visibilityMutation = useMutation({
    mutationFn: (view: SavedView) =>
      updateSavedView(tenantId, view.saved_view_id, {
        name: view.name,
        config: view.config,
        visibility: view.visibility === "private" ? "public" : "private",
      }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["saved-views", tenantId, "logs"] });
    },
  });

  const addGrantMutation = useMutation({
    mutationFn: () =>
      addSavedViewGrant(tenantId, managingViewId as string, newGrantUserId.trim(), newGrantRelation),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["saved-view-grants", tenantId, managingViewId] });
      setNewGrantUserId("");
    },
  });

  const revokeGrantMutation = useMutation({
    mutationFn: (userId: string) => revokeSavedViewGrant(tenantId, managingViewId as string, userId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["saved-view-grants", tenantId, managingViewId] });
    },
  });

  return (
    <div className="relative">
      <Button variant="secondary" onClick={() => setIsOpen((v) => !v)}>
        Saved Views
      </Button>
      {isOpen && (
        <div className="absolute z-10 mt-1 w-80 border border-[var(--border)] bg-[var(--surface)] p-2 shadow-lg">
          <ul className="max-h-72 overflow-y-auto">
            {views.map((view) => (
              <li key={view.saved_view_id} className="flex flex-col gap-1 border-b border-[var(--border)] py-1 last:border-0">
                <div className="flex items-center justify-between gap-2">
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
                  <span className="text-[10px] uppercase text-[var(--muted)]">{view.visibility}</span>
                  <button
                    type="button"
                    aria-label={`Manage ${view.name}`}
                    className="text-xs text-[var(--muted)] hover:text-[var(--brand)]"
                    onClick={() =>
                      setManagingViewId(managingViewId === view.saved_view_id ? null : view.saved_view_id)
                    }
                  >
                    Manage
                  </button>
                  <button
                    type="button"
                    aria-label={`Delete ${view.name}`}
                    className="text-xs text-[var(--muted)] hover:text-[var(--bad)]"
                    onClick={() => deleteMutation.mutate(view.saved_view_id)}
                  >
                    Delete
                  </button>
                </div>
                {managingViewId === view.saved_view_id && managingView && (
                  <div className="ml-2 flex flex-col gap-2 border-l border-[var(--border)] py-1 pl-2">
                    <Button
                      variant="secondary"
                      disabled={visibilityMutation.isPending}
                      onClick={() => visibilityMutation.mutate(managingView)}
                    >
                      Make {managingView.visibility === "private" ? "Public" : "Private"}
                    </Button>
                    <ul className="flex flex-col gap-1">
                      {grants.map((grant) => (
                        <li key={grant.user_id} className="flex items-center justify-between gap-2 text-xs">
                          <span>
                            {grant.user_id} ({grant.relation})
                          </span>
                          <button
                            type="button"
                            aria-label={`Remove ${grant.user_id}`}
                            className="text-[var(--muted)] hover:text-[var(--bad)]"
                            onClick={() => revokeGrantMutation.mutate(grant.user_id)}
                          >
                            Remove
                          </button>
                        </li>
                      ))}
                    </ul>
                    <div className="flex items-center gap-1">
                      <input
                        type="text"
                        placeholder="User ID"
                        value={newGrantUserId}
                        onChange={(e) => setNewGrantUserId(e.target.value)}
                        className="min-w-0 flex-1 border border-[var(--border)] bg-[var(--surface)] px-2 py-1 text-xs"
                      />
                      <select
                        aria-label="Relation"
                        value={newGrantRelation}
                        onChange={(e) =>
                          setNewGrantRelation(e.target.value as "owner" | "editor" | "viewer")
                        }
                        className="border border-[var(--border)] bg-[var(--surface)] px-1 py-1 text-xs"
                      >
                        <option value="viewer">Viewer</option>
                        <option value="editor">Editor</option>
                        <option value="owner">Owner</option>
                      </select>
                      <Button
                        variant="primary"
                        disabled={!newGrantUserId.trim() || addGrantMutation.isPending}
                        onClick={() => addGrantMutation.mutate()}
                      >
                        Add
                      </Button>
                    </div>
                  </div>
                )}
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
