function tenantHeaders(tenantId: string): HeadersInit {
  return { "X-Tenant-ID": tenantId };
}

export type SignalKind = "logs" | "traces" | "metrics";

export interface LogViewConfig {
  query: string | null;
  severity_filter: string;
  time_range: { mode: "preset"; preset: string } | { mode: "absolute"; from_ms: number; to_ms: number };
  visible_columns: string[];
}

export interface TraceViewConfig {
  query: string | null;
  status_filter: "all" | "ok" | "error";
  time_range: { mode: "preset"; preset: string } | { mode: "absolute"; from_ms: number; to_ms: number };
  visible_columns: string[];
}

export interface MetricViewConfig {
  service: string;
  selected_metric: string | null;
  time_range: { mode: "preset"; preset: string } | { mode: "absolute"; from_ms: number; to_ms: number };
}

export type AnyViewConfig = LogViewConfig | TraceViewConfig | MetricViewConfig | Record<string, unknown>;

export interface SavedView<T = AnyViewConfig> {
  saved_view_id: string;
  name: string;
  signal_kind: SignalKind;
  visibility: "private" | "public";
  config: T;
  created_at: string;
  updated_at: string;
}

export interface SavedViewListResponse<T = AnyViewConfig> {
  items: SavedView<T>[];
}

export interface CreateSavedViewRequest<T = AnyViewConfig> {
  name: string;
  signal_kind: SignalKind;
  config: T;
}

export interface UpdateSavedViewRequest<T = AnyViewConfig> {
  name: string;
  config: T;
  visibility?: "private" | "public";
}

export interface GrantItem {
  user_id: string;
  relation: "owner" | "editor" | "viewer";
  granted_at: string;
}

export interface GrantListResponse {
  grants: GrantItem[];
}

export async function fetchSavedViews<T = AnyViewConfig>(tenantId: string, signalKind: SignalKind): Promise<SavedViewListResponse<T>> {
  const res = await fetch(`/v1/saved-views?signal_kind=${signalKind}`, {
    credentials: "include",
    headers: tenantHeaders(tenantId),
  });
  if (!res.ok) throw new Error(`Saved views list failed: ${res.status}`);
  return res.json();
}

export async function createSavedView<T = AnyViewConfig>(tenantId: string, req: CreateSavedViewRequest<T>): Promise<SavedView<T>> {
  const res = await fetch("/v1/saved-views", {
    credentials: "include",
    method: "POST",
    headers: { ...tenantHeaders(tenantId), "Content-Type": "application/json" },
    body: JSON.stringify(req),
  });
  if (!res.ok) throw new Error(`Saved view create failed: ${res.status}`);
  return res.json();
}

export async function updateSavedView<T = AnyViewConfig>(
  tenantId: string,
  savedViewId: string,
  req: UpdateSavedViewRequest<T>,
): Promise<SavedView<T>> {
  const res = await fetch(`/v1/saved-views/${savedViewId}`, {
    credentials: "include",
    method: "PUT",
    headers: { ...tenantHeaders(tenantId), "Content-Type": "application/json" },
    body: JSON.stringify(req),
  });
  if (!res.ok) throw new Error(`Saved view update failed: ${res.status}`);
  return res.json();
}

export async function deleteSavedView(tenantId: string, savedViewId: string): Promise<void> {
  const res = await fetch(`/v1/saved-views/${savedViewId}`, {
    credentials: "include",
    method: "DELETE",
    headers: tenantHeaders(tenantId),
  });
  if (!res.ok) throw new Error(`Saved view delete failed: ${res.status}`);
}

export async function fetchSavedViewGrants(tenantId: string, savedViewId: string): Promise<GrantListResponse> {
  const res = await fetch(`/v1/saved-views/${savedViewId}/grants`, {
    credentials: "include",
    headers: tenantHeaders(tenantId),
  });
  if (!res.ok) throw new Error(`Saved view grants list failed: ${res.status}`);
  return res.json();
}

export async function addSavedViewGrant(
  tenantId: string,
  savedViewId: string,
  userId: string,
  relation: "owner" | "editor" | "viewer",
): Promise<void> {
  const res = await fetch(`/v1/saved-views/${savedViewId}/grants`, {
    credentials: "include",
    method: "POST",
    headers: { ...tenantHeaders(tenantId), "Content-Type": "application/json" },
    body: JSON.stringify({ user_id: userId, relation }),
  });
  if (!res.ok) throw new Error(`Saved view grant add failed: ${res.status}`);
}

export async function revokeSavedViewGrant(tenantId: string, savedViewId: string, userId: string): Promise<void> {
  const res = await fetch(`/v1/saved-views/${savedViewId}/grants/${userId}`, {
    credentials: "include",
    method: "DELETE",
    headers: tenantHeaders(tenantId),
  });
  if (!res.ok) throw new Error(`Saved view grant revoke failed: ${res.status}`);
}
