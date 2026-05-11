function tenantHeaders(tenantId: string): HeadersInit {
  return { "X-Tenant-ID": tenantId };
}

export type NotificationChannelType = "webhook";

export interface NotificationChannelConfig {
  url: string;
  [key: string]: unknown;
}

export interface NotificationChannelItem {
  channel_id: string;
  name: string;
  channel_type: NotificationChannelType;
  config: NotificationChannelConfig;
}

export interface CreateChannelRequest {
  name: string;
  channel_type: NotificationChannelType;
  config: NotificationChannelConfig;
}

export async function listNotificationChannels(tenantId: string): Promise<NotificationChannelItem[]> {
  const res = await fetch("/v1/notifications/channels", { headers: tenantHeaders(tenantId) });
  if (!res.ok) throw new Error(`Failed to list notification channels: ${res.status}`);
  return res.json();
}

export async function createNotificationChannel(
  tenantId: string,
  req: CreateChannelRequest,
): Promise<NotificationChannelItem> {
  const res = await fetch("/v1/notifications/channels", {
    method: "POST",
    headers: { ...tenantHeaders(tenantId), "Content-Type": "application/json" },
    body: JSON.stringify(req),
  });
  if (!res.ok) throw new Error(`Failed to create notification channel: ${res.status}`);
  return res.json();
}

export async function deleteNotificationChannel(
  tenantId: string,
  channelId: string,
): Promise<void> {
  const res = await fetch(`/v1/notifications/channels/${channelId}`, {
    method: "DELETE",
    headers: tenantHeaders(tenantId),
  });
  if (!res.ok) throw new Error(`Failed to delete notification channel: ${res.status}`);
}
