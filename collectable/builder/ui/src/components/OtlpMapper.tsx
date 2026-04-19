import React from 'react';

interface Props {
  definition: Record<string, unknown>;
  onChange: (d: Record<string, unknown>) => void;
  onNext: () => void;
}

const TARGETS = [
  'resource_attributes',
  'log_attributes',
  'body',
  'severity_text',
  'trace_id',
  'span_id',
  'drop',
];

export function OtlpMapper({ definition, onChange, onNext }: Props) {
  const mapping = (definition.mapping as Record<string, unknown>) ?? {};

  const setOutput = (field: string, target: string) => {
    const updated = { ...mapping };
    if (target === 'drop') {
      delete (updated as Record<string, unknown>)[field];
    } else if (target === 'body') {
      updated.body = { field };
    } else if (target === 'severity_text') {
      updated.severity_text = { field };
    } else if (target === 'resource_attributes') {
      const ra = (updated.resource_attributes as Record<string, unknown>) ?? {};
      updated.resource_attributes = { ...ra, [field]: { field } };
    } else if (target === 'log_attributes') {
      const la = (updated.log_attributes as Record<string, unknown>) ?? {};
      updated.log_attributes = { ...la, [field]: { field } };
    }
    onChange({ ...definition, mapping: updated });
  };

  return (
    <div>
      <h2>Step 3 — OTLP Mapping</h2>
      <p>For each extracted field, choose where it maps in the OTel LogRecord.</p>

      <table style={{ width: '100%', borderCollapse: 'collapse' }}>
        <thead>
          <tr>
            <th style={{ textAlign: 'left', padding: '4px 8px' }}>Field</th>
            <th style={{ textAlign: 'left', padding: '4px 8px' }}>OTLP target</th>
          </tr>
        </thead>
        <tbody>
          <tr>
            <td style={{ padding: '4px 8px', fontFamily: 'monospace' }}>$raw</td>
            <td style={{ padding: '4px 8px' }}>
              <select defaultValue="body" onChange={e => setOutput('$raw', e.target.value)}>
                {TARGETS.map(t => <option key={t} value={t}>{t}</option>)}
              </select>
            </td>
          </tr>
          <tr>
            <td style={{ padding: '4px 8px', fontFamily: 'monospace' }}>timestamp</td>
            <td style={{ padding: '4px 8px' }}>
              <select defaultValue="drop" onChange={e => setOutput('timestamp', e.target.value)}>
                {['time_field', ...TARGETS].map(t => <option key={t} value={t}>{t}</option>)}
              </select>
            </td>
          </tr>
        </tbody>
      </table>

      <p style={{ color: '#888', fontSize: 12, marginTop: 8 }}>
        Additional fields will be populated from parsed output after Step 2 is complete.
      </p>

      <div style={{ marginTop: 16 }}>
        <label>OTLP endpoint<br />
          <input type="text" style={{ width: '100%' }}
            placeholder="${OTLP_ENDPOINT}"
            onChange={e => onChange({
              ...definition,
              output: { ...((definition.output as object) ?? {}), endpoint: e.target.value }
            })} />
        </label>
        <label style={{ display: 'block', marginTop: 8 }}>Protocol<br />
          <select onChange={e => onChange({
            ...definition,
            output: { ...((definition.output as object) ?? {}), protocol: e.target.value }
          })}>
            <option value="grpc">gRPC</option>
            <option value="http">HTTP</option>
          </select>
        </label>
      </div>

      <button style={{ marginTop: 24 }} onClick={onNext}>
        Next →
      </button>
    </div>
  );
}
