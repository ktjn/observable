import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { useRef, useState } from "react";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Panel } from "../components/ui/panel";
import { createToken, deleteToken, listTokens, renewToken, restoreToken, revokeToken, type TokenRecord, type CreateTokenRequest } from "../api/tokens";
import { useTenantContext } from "../hooks/useTenantContext";

export default function SetupTokensPage() {
  const qc = useQueryClient();
  const { tenantId } = useTenantContext();
  const { data, isLoading } = useQuery({
    queryKey: ["tokens", tenantId],
    queryFn: () => listTokens(tenantId),
  });

  const [showForm, setShowForm] = useState(false);
  const [name, setName] = useState("");
  const [environment, setEnvironment] = useState("");
  const [newPlaintext, setNewPlaintext] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const nameRef = useRef<HTMLInputElement>(null);

  const knownEnvs = Array.from(
    new Set((data?.tokens ?? []).filter((t) => !t.revoked).map((t) => t.environment).filter(Boolean)),
  ).sort();

  const createMutation = useMutation({
    mutationFn: (body: CreateTokenRequest) => createToken(tenantId, body),
    onSuccess: (res) => {
      void qc.invalidateQueries({ queryKey: ["tokens", tenantId] });
      setNewPlaintext(res.plaintext);
      setShowForm(false);
      setName("");
      setEnvironment("");
      setFormError(null);
    },
    onError: () => setFormError("Failed to create token. Please try again."),
  });

  const revokeMutation = useMutation({
    mutationFn: (id: string) => revokeToken(tenantId, id),
    onSuccess: () => void qc.invalidateQueries({ queryKey: ["tokens", tenantId] }),
  });

  const renewMutation = useMutation({
    mutationFn: (id: string) => renewToken(tenantId, id),
    onSuccess: (res) => {
      void qc.invalidateQueries({ queryKey: ["tokens", tenantId] });
      setNewPlaintext(res.plaintext);
    },
  });

  const restoreMutation = useMutation({
    mutationFn: (id: string) => restoreToken(tenantId, id),
    onSuccess: () => void qc.invalidateQueries({ queryKey: ["tokens", tenantId] }),
  });

  const deleteMutation = useMutation({
    mutationFn: (id: string) => deleteToken(tenantId, id),
    onSuccess: () => void qc.invalidateQueries({ queryKey: ["tokens", tenantId] }),
  });

  function handleCreate(e: React.FormEvent) {
    e.preventDefault();
    if (!name.trim() || !environment.trim()) {
      setFormError("Name and environment are required.");
      return;
    }
    createMutation.mutate({ name: name.trim(), environment: environment.trim() });
  }

  function handleShowForm() {
    setShowForm(true);
    setFormError(null);
    setTimeout(() => nameRef.current?.focus(), 50);
  }

  async function copyPlaintext() {
    if (!newPlaintext) return;
    try {
      await navigator.clipboard.writeText(newPlaintext);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      /* ignore */
    }
  }

  return (
    <section className="page-stack" aria-labelledby="setup-tokens-heading">
      <div className="page-header">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Setup</div>
          <h1 id="setup-tokens-heading">Ingestion tokens</h1>
        </div>
      </div>

      <Panel eyebrow="Credentials" title="Ingestion tokens">
        <p className="mb-4 text-sm text-[var(--text-muted)]">
          Each token binds a client to a tenant and environment. The ingest gateway resolves the
          environment from the token — clients need no additional configuration.
        </p>

        {newPlaintext && (
          <div
            className="mb-4 rounded border border-[var(--border)] bg-[var(--surface-raised)] p-3"
            role="alert"
          >
            <p className="mb-1 text-xs font-semibold text-[var(--text-muted)]">
              Token value — copy it now. It will not be shown again.
            </p>
            <div className="flex items-center gap-2">
              <code className="flex-1 break-all text-xs">{newPlaintext}</code>
              <Button variant="secondary" onClick={() => void copyPlaintext()}>
                {copied ? "Copied!" : "Copy"}
              </Button>
              <Button variant="ghost" onClick={() => setNewPlaintext(null)}>
                Dismiss
              </Button>
            </div>
          </div>
        )}

        {showForm && (
          <form onSubmit={handleCreate} className="mb-4 flex flex-wrap items-end gap-2">
            <datalist id="env-list">
              {knownEnvs.map((e) => (
                <option key={e} value={e} />
              ))}
            </datalist>
            <div className="flex flex-col gap-1">
              <label className="text-xs font-medium" htmlFor="token-name">
                Name
              </label>
              <input
                id="token-name"
                ref={nameRef}
                className="select-input"
                placeholder="e.g. shop-api staging"
                value={name}
                onChange={(e) => setName(e.target.value)}
                autoComplete="off"
              />
            </div>
            <div className="flex flex-col gap-1">
              <label className="text-xs font-medium" htmlFor="token-env">
                Environment
              </label>
              <input
                id="token-env"
                className="select-input"
                list="env-list"
                placeholder="e.g. production"
                value={environment}
                onChange={(e) => setEnvironment(e.target.value)}
                autoComplete="off"
              />
            </div>
            <Button type="submit" disabled={createMutation.isPending}>
              {createMutation.isPending ? "Creating…" : "Create token"}
            </Button>
            <Button
              type="button"
              variant="ghost"
              onClick={() => {
                setShowForm(false);
                setFormError(null);
              }}
            >
              Cancel
            </Button>
            {formError && <p className="w-full text-xs text-[var(--error)]">{formError}</p>}
          </form>
        )}

        {!showForm && (
          <Button variant="secondary" className="mb-4" onClick={handleShowForm}>
            + New token
          </Button>
        )}

        {isLoading ? (
          <p className="text-sm text-[var(--text-muted)]">Loading tokens…</p>
        ) : (
          <TokenTable
            tokens={data?.tokens ?? []}
            onRevoke={(id) => revokeMutation.mutate(id)}
            revoking={revokeMutation.isPending ? revokeMutation.variables : undefined}
            onRenew={(id) => renewMutation.mutate(id)}
            renewing={renewMutation.isPending ? renewMutation.variables : undefined}
            onRestore={(id) => restoreMutation.mutate(id)}
            restoring={restoreMutation.isPending ? restoreMutation.variables : undefined}
            onDelete={(id) => {
              if (confirm("Permanently delete this token? This cannot be undone.")) {
                deleteMutation.mutate(id);
              }
            }}
            deleting={deleteMutation.isPending ? deleteMutation.variables : undefined}
          />
        )}
      </Panel>
    </section>
  );
}

interface TokenTableProps {
  tokens: TokenRecord[];
  onRevoke: (id: string) => void;
  revoking?: string;
  onRenew: (id: string) => void;
  renewing?: string;
  onRestore: (id: string) => void;
  restoring?: string;
  onDelete: (id: string) => void;
  deleting?: string;
}

function TokenTable({ tokens, onRevoke, revoking, onRenew, renewing, onRestore, restoring, onDelete, deleting }: TokenTableProps) {
  if (tokens.length === 0) {
    return <p className="text-sm text-[var(--text-muted)]">No tokens registered.</p>;
  }

  return (
    <div className="overflow-x-auto">
      <table className="w-full text-sm">
        <thead>
          <tr className="border-b border-[var(--border)] text-left text-xs font-medium text-[var(--text-muted)]">
            <th className="py-2 pr-4">Name</th>
            <th className="py-2 pr-4">Tenant</th>
            <th className="py-2 pr-4">Environment</th>
            <th className="py-2 pr-4">Created</th>
            <th className="py-2 pr-4">Status</th>
            <th className="py-2" />
          </tr>
        </thead>
        <tbody>
          {tokens.map((t) => (
            <tr key={t.id} className="border-b border-[var(--border-subtle)]">
              <td className="py-2 pr-4 font-medium">{t.name}</td>
              <td className="py-2 pr-4 text-[var(--text-muted)]">{t.tenant_name}</td>
              <td className="py-2 pr-4">
                <code className="text-xs">{t.environment || <em className="text-[var(--text-muted)]">—</em>}</code>
              </td>
              <td className="py-2 pr-4 text-xs text-[var(--text-muted)]">
                {new Date(t.created_at).toLocaleDateString()}
              </td>
              <td className="py-2 pr-4">
                <Badge tone={t.revoked ? "neutral" : "good"}>
                  {t.revoked ? "Revoked" : "Active"}
                </Badge>
              </td>
              <td className="py-2">
                {!t.revoked ? (
                  <div className="flex gap-1">
                    <Button
                      variant="ghost"
                      disabled={renewing === t.id}
                      onClick={() => onRenew(t.id)}
                    >
                      {renewing === t.id ? "Renewing…" : "Renew"}
                    </Button>
                    <Button
                      variant="ghost"
                      disabled={revoking === t.id}
                      onClick={() => {
                        if (confirm(`Revoke token "${t.name}"? This cannot be undone.`)) {
                          onRevoke(t.id);
                        }
                      }}
                    >
                      {revoking === t.id ? "Revoking…" : "Revoke"}
                    </Button>
                  </div>
                ) : (
                  <div className="flex gap-1">
                    <Button
                      variant="ghost"
                      disabled={restoring === t.id}
                      onClick={() => onRestore(t.id)}
                    >
                      {restoring === t.id ? "Restoring…" : "Restore"}
                    </Button>
                    <Button
                      variant="ghost"
                      disabled={deleting === t.id}
                      onClick={() => onDelete(t.id)}
                    >
                      {deleting === t.id ? "Deleting…" : "Delete"}
                    </Button>
                  </div>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
