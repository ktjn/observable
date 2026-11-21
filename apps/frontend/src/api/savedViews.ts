import { dispatch } from "../runtime/requestTransport";

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

export async function fetchSavedViews<T = AnyViewConfig>(_tenantId: string, signalKind: SignalKind): Promise<SavedViewListResponse<T>> {
  return dispatch<SavedViewListResponse<T>>({
    method: "GET",
    path: "/v1/saved-views",
    query: { signal_kind: signalKind },
  });
}

export async function createSavedView<T = AnyViewConfig>(_tenantId: string, req: CreateSavedViewRequest<T>): Promise<SavedView<T>> {
  return dispatch<SavedView<T>>({ method: "POST", path: "/v1/saved-views", body: req });
}

export async function updateSavedView<T = AnyViewConfig>(
  _tenantId: string,
  savedViewId: string,
  req: UpdateSavedViewRequest<T>,
): Promise<SavedView<T>> {
  return dispatch<SavedView<T>>({ method: "PUT", path: `/v1/saved-views/${savedViewId}`, body: req });
}

export async function deleteSavedView(_tenantId: string, savedViewId: string): Promise<void> {
  await dispatch({ method: "DELETE", path: `/v1/saved-views/${savedViewId}` });
}

export async function fetchSavedViewGrants(_tenantId: string, savedViewId: string): Promise<GrantListResponse> {
  return dispatch<GrantListResponse>({ method: "GET", path: `/v1/saved-views/${savedViewId}/grants` });
}

export async function addSavedViewGrant(
  _tenantId: string,
  savedViewId: string,
  userId: string,
  relation: "owner" | "editor" | "viewer",
): Promise<void> {
  await dispatch({
    method: "POST",
    path: `/v1/saved-views/${savedViewId}/grants`,
    body: { user_id: userId, relation },
  });
}

export async function revokeSavedViewGrant(_tenantId: string, savedViewId: string, userId: string): Promise<void> {
  await dispatch({ method: "DELETE", path: `/v1/saved-views/${savedViewId}/grants/${userId}` });
}
