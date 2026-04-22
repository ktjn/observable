import React from 'react';

interface Props {
  definition: Record<string, unknown>;
}

function formatSource(val: unknown): string {
  if (!val || typeof val !== 'object') return String(val ?? '');
  const v = val as Record<string, string>;
  if (v.command !== undefined) return `$(${v.command})`;
  if (v.env !== undefined) return `\${${v.env}}`;
  if (v.literal !== undefined) return `"${v.literal}"`;
  if (v.field !== undefined) return `← ${v.field}`;
  return JSON.stringify(val);
}

export function PreviewPanel({ definition }: Props) {
  const mapping = definition.mapping as Record<string, unknown> | undefined;

  const resourceAttrs = (mapping?.resource_attributes ?? {}) as Record<string, unknown>;
  const logAttrs = (mapping?.log_attributes ?? {}) as Record<string, unknown>;
  const body = mapping?.body;
  const severityText = mapping?.severity_text;
  const timeField = mapping?.time_field as Record<string, string> | undefined;

  const preview = {
    resource_attributes: Object.fromEntries(
      Object.entries(resourceAttrs).map(([k, v]) => [k, formatSource(v)])
    ),
    body: body ? formatSource(body) : '← message (default)',
    severity_text: severityText ? formatSource(severityText) : null,
    timestamp: timeField ? `← ${timeField.field} (${timeField.format})` : 'observed_time (default)',
    log_attributes: Object.fromEntries(
      Object.entries(logAttrs).map(([k, v]) => [k, formatSource(v)])
    ),
  };

  return (
    <div style={{ marginTop: 24, borderTop: '1px solid #ddd', paddingTop: 16 }}>
      <h3 style={{ fontSize: 14, marginBottom: 8 }}>Live OTLP LogRecord preview</h3>
      <pre style={{ background: '#e8f4e8', padding: 16, overflow: 'auto', fontSize: 12, borderRadius: 4 }}>
        {JSON.stringify(preview, null, 2)}
      </pre>
    </div>
  );
}
