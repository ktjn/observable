import type { NotificationChannel as NotificationChannelItem } from "./generated/notifications/notifications.NotificationChannel.v1";
import { dispatch } from "../runtime/requestTransport";

export type { NotificationChannelItem };

export type NotificationChannelType = "webhook";

export interface NotificationChannelConfig {
  url: string;
  [key: string]: unknown;
}

export interface CreateChannelRequest {
  name: string;
  channel_type: NotificationChannelType;
  config: NotificationChannelConfig;
}

export async function listNotificationChannels(_tenantId: string): Promise<NotificationChannelItem[]> {
  return dispatch<NotificationChannelItem[]>({ method: "GET", path: "/v1/notifications/channels" });
}

export async function createNotificationChannel(
  _tenantId: string,
  req: CreateChannelRequest,
): Promise<NotificationChannelItem> {
  return dispatch<NotificationChannelItem>({
    method: "POST",
    path: "/v1/notifications/channels",
    body: req,
  });
}

export async function deleteNotificationChannel(
  _tenantId: string,
  channelId: string,
): Promise<void> {
  await dispatch({ method: "DELETE", path: `/v1/notifications/channels/${channelId}` });
}
