import React, { useState } from 'react';

const TARGETS = [
  { value: 'x86_64-unknown-linux-musl', label: 'Linux x86-64 (static musl) — recommended for containers' },
  { value: 'aarch64-unknown-linux-musl', label: 'Linux ARM64 (static musl) — Graviton, Apple Silicon containers' },
  { value: 'x86_64-unknown-linux-gnu', label: 'Linux x86-64 (glibc)' },
  { value: 'aarch64-unknown-linux-gnu', label: 'Linux ARM64 (glibc)' },
  { value: 'x86_64-pc-windows-gnu', label: 'Windows x86-64' },
  { value: 'x86_64-apple-darwin', label: 'macOS Intel' },
  { value: 'aarch64-apple-darwin', label: 'macOS Apple Silicon' },
];

interface Props {
  definition: Record<string, unknown>;
  onChange: (d: Record<string, unknown>) => void;
}

function buildPayload(definition: Record<string, unknown>, target: string): string {
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  const { parsed_fields: _pf, _sample, _includeRaw, ...cleanDef } = definition;
  return JSON.stringify({ definition: cleanDef, target });
}

function makeCurlCommand(definition: Record<string, unknown>, target: string): string {
  const host = `${window.location.hostname}:8091`;
  const url = `http://${host}/build`;
  const payload = buildPayload(definition, target);
  const name = (definition.name as string) ?? 'mediator';
  return `curl -X POST '${url}' \\\n  -H 'Content-Type: application/json' \\\n  -d '${payload.replace(/'/g, "'\\''")}' \\\n  --output ${name}-${target}.zip`;
}

export function DownloadPanel({ definition, onChange }: Props) {
  const [target, setTarget] = useState('x86_64-unknown-linux-musl');
  const [building, setBuilding] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const curlCmd = makeCurlCommand(definition, target);
  const canBuild = !!(definition.name as string)?.trim();

  const copyToClipboard = () => {
    navigator.clipboard.writeText(curlCmd).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  };

  const build = async () => {
    setBuilding(true);
    setError(null);
    try {
      const body = buildPayload(definition, target);
      const res = await fetch('/build', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body,
      });
      if (!res.ok) {
        const text = await res.text();
        throw new Error(text);
      }
      const blob = await res.blob();
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `${definition.name ?? 'mediator'}-${target}.zip`;
      a.click();
      URL.revokeObjectURL(url);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBuilding(false);
    }
  };

  return (
    <div>
      <h2>Step 4 — Build &amp; Download</h2>

      <label style={{ display: 'block', marginBottom: 16 }}>Pipeline name
        <span style={{ color: '#c00', marginLeft: 2 }}>*</span><br />
        <input type="text" style={{ width: '100%' }}
          placeholder="e.g. journalctl-to-otlp"
          value={(definition.name as string) ?? ''}
          onChange={e => onChange({ ...definition, name: e.target.value.replace(/\s+/g, '-').toLowerCase() })} />
        <small style={{ color: '#888' }}>Used as the binary filename, Rust crate name, systemd unit, and Dockerfile. Only lowercase letters, digits, and hyphens.</small>
      </label>

      <label>Target ABI<br />
        <select value={target} onChange={e => setTarget(e.target.value)}>
          {TARGETS.map(t => <option key={t.value} value={t.value}>{t.label}</option>)}
        </select>
      </label>

      <p style={{ fontSize: 13, color: '#555', marginTop: 8 }}>
        The download package contains: compiled binary, generated Rust source,
        systemd unit file, init.d script, Dockerfile, and docker-compose.yml snippet.
      </p>

      {/* Curl command */}
      <div style={{ marginTop: 16 }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 4 }}>
          <span style={{ fontSize: 13, fontWeight: 600 }}>curl command</span>
          <button
            onClick={copyToClipboard}
            style={{ fontSize: 12, padding: '2px 10px', cursor: 'pointer' }}
          >
            {copied ? '✓ Copied' : 'Copy'}
          </button>
        </div>
        <pre style={{
          background: '#1e1e1e',
          color: '#d4d4d4',
          padding: 12,
          borderRadius: 4,
          fontSize: 12,
          overflowX: 'auto',
          whiteSpace: 'pre-wrap',
          wordBreak: 'break-all',
          userSelect: 'all',
          cursor: 'text',
        }}>
          {curlCmd}
        </pre>
      </div>

      <details style={{ marginTop: 12 }}>
        <summary style={{ fontSize: 13, cursor: 'pointer' }}>Environment variables for test / production</summary>
        <pre style={{ background: '#f4f4f4', padding: 12, fontSize: 12, marginTop: 8 }}>{`# Required
OTLP_ENDPOINT=https://ingest.example.com:4317
OTLP_TOKEN=your-token-here

# Optional overrides
OTLP_PROTOCOL=grpc          # grpc (default) or http
OTLP_INSECURE=false         # true for local dev (disables TLS)
TRANSPORT_PORT=5140         # transport listen port
COLLECTABLE_LOG_LEVEL=info  # trace / debug / info / warn / error
COLLECTABLE_LOG_FORMAT=json # json or text`}
        </pre>
      </details>

        <button style={{ marginTop: 16 }} onClick={build} disabled={building || !canBuild}>
        {building ? 'Building…' : 'Build & Download'}
      </button>

      {error && (
        <pre style={{ color: 'red', marginTop: 12, whiteSpace: 'pre-wrap' }}>{error}</pre>
      )}
    </div>
  );
}
