import React from 'react';

interface Props {
  definition: Record<string, unknown>;
}

export function PreviewPanel({ definition }: Props) {
  if (!definition.mapping) return null;
  return (
    <div style={{ marginTop: 32 }}>
      <h3>Live OTLP LogRecord preview</h3>
      <pre style={{ background: '#e8f4e8', padding: 16, overflow: 'auto', fontSize: 12 }}>
        {JSON.stringify({
          resource_attributes: (definition.mapping as Record<string, unknown>).resource_attributes ?? {},
          log_attributes: (definition.mapping as Record<string, unknown>).log_attributes ?? {},
          body: (definition.mapping as Record<string, unknown>).body ?? '<raw>',
          severity_text: (definition.mapping as Record<string, unknown>).severity_text ?? null,
        }, null, 2)}
      </pre>
    </div>
  );
}
