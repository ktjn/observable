/**
 * @modelable domain: notifications
 * @modelable name: NotificationChannel
 * @modelable owner: platform-team
 * @modelable kind: entity
 * @modelable version: 1
 * @modelable changeKind: additive
 */
export interface NotificationsNotificationChannelV1 {
  channel_id: string;
  name: string;
  channel_type: 'webhook';
  config: unknown;
}
export type NotificationChannel = NotificationsNotificationChannelV1;
