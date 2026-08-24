import type { CreateChannelRequest, NotificationChannelItem } from "../api/notifications";
type V = string | number | null;
interface O { sql: string; bind?: Record<string, V>; returnValue?: "resultRows"; rowMode?: "object"; }
interface DB { exec(sql: string): unknown; exec(o: O): unknown; }
const TENANT = "00000000-0000-0000-0000-000000000001";
const schema = "CREATE TABLE IF NOT EXISTS notification_channels (tenant_id TEXT NOT NULL, channel_id TEXT PRIMARY KEY, channel_json TEXT NOT NULL);";
const rows = <T,>(db: DB, o: O): T[] => (db.exec({ ...o, returnValue: "resultRows", rowMode: "object" }) as T[]) ?? [];
const newId = () => `playground-channel-${globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(36).slice(2)}`}`;
const map = (r: Record<string, V>) => JSON.parse(String(r.channel_json)) as NotificationChannelItem;
export class SqliteNotificationChannelRepository {
  private constructor(private readonly db: DB) {}
  static async open(): Promise<SqliteNotificationChannelRepository> { const { default: init } = await import("@sqlite.org/sqlite-wasm"); const sqlite3 = await init(); const r = new SqliteNotificationChannelRepository(new sqlite3.oo1.DB(":memory:", "c") as unknown as DB); r.db.exec(schema); r.seed(); return r; }
  list(tenantId: string): NotificationChannelItem[] { return rows<Record<string, V>>(this.db, { sql: "SELECT channel_json FROM notification_channels WHERE tenant_id = $tenant_id ORDER BY channel_id", bind: { $tenant_id: tenantId } }).map(map); }
  create(tenantId: string, req: CreateChannelRequest): NotificationChannelItem { const channel = { channel_id: newId(), name: req.name, channel_type: req.channel_type, config: req.config } satisfies NotificationChannelItem; this.insert(tenantId, channel); return channel; }
  delete(tenantId: string, channelId: string): void { this.db.exec({ sql: "DELETE FROM notification_channels WHERE tenant_id = $tenant_id AND channel_id = $channel_id", bind: { $tenant_id: tenantId, $channel_id: channelId } }); }
  private insert(tenantId: string, channel: NotificationChannelItem): void { this.db.exec({ sql: "INSERT INTO notification_channels (tenant_id, channel_id, channel_json) VALUES ($tenant_id, $channel_id, $channel_json)", bind: { $tenant_id: tenantId, $channel_id: channel.channel_id, $channel_json: JSON.stringify(channel) } }); }
  private seed(): void { this.insert(TENANT, { channel_id: "playground-channel-1", name: "demo webhook", channel_type: "webhook", config: { url: "https://example.invalid/hook" } }); }
}
