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
}

export function DownloadPanel({ definition }: Props) {
  const [target, setTarget] = useState('x86_64-unknown-linux-musl');
  const [building, setBuilding] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const build = async () => {
    setBuilding(true);
    setError(null);
    try {
      const res = await fetch('/build', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ definition, target }),
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
      <h2>Step 4 — Download</h2>
      <label>Target ABI<br />
        <select value={target} onChange={e => setTarget(e.target.value)}>
          {TARGETS.map(t => <option key={t.value} value={t.value}>{t.label}</option>)}
        </select>
      </label>

      <p style={{ fontSize: 13, color: '#555', marginTop: 8 }}>
        The download package contains: compiled binary, generated Rust source,
        Cargo.toml with pinned dependencies, systemd unit file, init.d script,
        Dockerfile, and docker-compose.yml snippet.
      </p>

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

      <button style={{ marginTop: 16 }} onClick={build} disabled={building}>
        {building ? 'Building…' : 'Build & Download'}
      </button>

      {error && (
        <pre style={{ color: 'red', marginTop: 12, whiteSpace: 'pre-wrap' }}>{error}</pre>
      )}
    </div>
  );
}
