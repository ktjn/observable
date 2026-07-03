import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  listNotificationChannels,
  createNotificationChannel,
  deleteNotificationChannel,
  type CreateChannelRequest,
  type NotificationChannelConfig,
} from "../../api/notifications";
import { Button } from "../../components/ui/button";
import { CopyableText } from "../../components/ui/copy-button";
import { Input } from "../../components/ui/input";
import { Panel } from "../../components/ui/panel";
import { Toolbar } from "../../components/ui/toolbar";
import { EmptyState } from "../../components/ui/empty-state";
import { useTenantContext } from "../../hooks/useTenantContext";

export function NotificationChannelsList() {
  const queryClient = useQueryClient();
  const { tenantId } = useTenantContext();
  const [isCreating, setIsCreating] = useState(false);
  const [name, setName] = useState("");
  const [url, setUrl] = useState("");
  const [error, setError] = useState<string | null>(null);

  const { data, isLoading } = useQuery({
    queryKey: ["notification-channels", tenantId],
    queryFn: () => listNotificationChannels(tenantId),
  });

  const createMutation = useMutation({
    mutationFn: (req: CreateChannelRequest) => createNotificationChannel(tenantId, req),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["notification-channels", tenantId] });
      setIsCreating(false);
      setName("");
      setUrl("");
      setError(null);
    },
    onError: (e: Error) => setError(e.message),
  });

  const deleteMutation = useMutation({
    mutationFn: (channelId: string) => deleteNotificationChannel(tenantId, channelId),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["notification-channels", tenantId] }),
  });

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!name || !url) {
      setError("Name and URL are required");
      return;
    }
    createMutation.mutate({
      name,
      channel_type: "webhook",
      config: { url },
    });
  };

  const channels = data ?? [];

  return (
    <div className="space-y-4">
      <Toolbar aria-label="Notification actions" className="justify-end">
        <Button onClick={() => setIsCreating((v) => !v)}>
          {isCreating ? "Cancel" : "New Channel"}
        </Button>
      </Toolbar>

      {isCreating && (
        <Panel title="Add Webhook Channel" eyebrow="Outgoing notification">
          <form onSubmit={handleSubmit} className="flex flex-col gap-3">
            <div className="grid gap-3 sm:grid-cols-2">
              <div className="space-y-1">
                <label className="text-xs font-bold uppercase text-[var(--muted)]" htmlFor="chan-name">Channel name</label>
                <Input
                  id="chan-name"
                  placeholder="e.g. Production Webhook"
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  required
                />
              </div>
              <div className="space-y-1">
                <label className="text-xs font-bold uppercase text-[var(--muted)]" htmlFor="chan-url">Webhook URL</label>
                <Input
                  id="chan-url"
                  placeholder="https://..."
                  value={url}
                  onChange={(e) => setUrl(e.target.value)}
                  required
                />
              </div>
            </div>

            {error && (
              <div role="alert" className="text-sm font-bold text-[var(--bad)]">
                {error}
              </div>
            )}

            <div className="flex gap-2 pt-2">
              <Button type="submit" disabled={createMutation.isPending}>
                {createMutation.isPending ? "Adding…" : "Add Channel"}
              </Button>
              <Button type="button" variant="ghost" onClick={() => setIsCreating(false)}>
                Cancel
              </Button>
            </div>
          </form>
        </Panel>
      )}

      {isLoading ? (
        <div className="py-8 text-center text-[var(--muted)]">Loading channels…</div>
      ) : channels.length === 0 ? (
        <EmptyState
          title="No notification channels"
          description="Add a webhook to receive alerts in your external tools."
        />
      ) : (
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {channels.map((channel) => (
            <Panel key={channel.channel_id} title={channel.name} eyebrow={channel.channel_type}>
              <div className="flex flex-col gap-2">
                <CopyableText
                  value={(channel.config as NotificationChannelConfig).url}
                  label="Copy webhook URL"
                  className="text-xs text-[var(--muted)]"
                />
                <div className="flex justify-end pt-2">
                  <Button
                    variant="ghost"
                    className="text-[var(--bad)]"
                    onClick={() => {
                      if (confirm("Delete this channel?")) {
                        deleteMutation.mutate(channel.channel_id);
                      }
                    }}
                  >
                    Delete
                  </Button>
                </div>
              </div>
            </Panel>
          ))}
        </div>
      )}
    </div>
  );
}
