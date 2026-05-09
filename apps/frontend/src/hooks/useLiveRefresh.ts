export const LIVE_VIEW_REFRESH_INTERVAL_MS = 5_000;

export const liveViewQueryOptions = {
  refetchInterval: LIVE_VIEW_REFRESH_INTERVAL_MS,
  refetchIntervalInBackground: false,
} as const;
