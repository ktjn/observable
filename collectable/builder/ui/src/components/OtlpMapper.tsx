import React, { useState, useEffect } from 'react';

interface Props {
  definition: Record<string, unknown>;
  onChange: (d: Record<string, unknown>) => void;
  onNext: () => void;
}

const ALL_TARGETS = ['time_field', 'body', 'severity_text', 'trace_id', 'span_id', 'resource_attributes', 'log_attributes', 'drop'];

function defaultTarget(field: string): string {
  if (field === '$raw') return 'body';
  const f = field.toLowerCase();
  if (/^(timestamp|time|date|datetime|@timestamp|timemillis)$/.test(f)) return 'time_field';
  if (/^(level|severity|log_?level|loglevel|priority)$/.test(f)) return 'severity_text';
  if (/^(message|msg|body|text|log)$/.test(f)) return 'body';
  if (/^(loggername|logger|class)$/.test(f)) return 'log_attributes';
  if (/^trace_?id$/.test(f)) return 'trace_id';
  if (/^span_?id$/.test(f)) return 'span_id';
  return 'log_attributes';
}

function applyMapping(mapping: Record<string, unknown>, field: string, target: string): Record<string, unknown> {
  const m = { ...mapping };
  // Clear previous location for this field
  (['resource_attributes', 'log_attributes'] as const).forEach(section => {
    const s = m[section] as Record<string, unknown> | undefined;
    if (s && field in s) { const u = { ...s }; delete u[field]; m[section] = u; }
  });
  if ((m.body as Record<string, unknown>)?.field === field) delete m.body;
  if ((m.severity_text as Record<string, unknown>)?.field === field) delete m.severity_text;
  if ((m.time_field as Record<string, unknown>)?.field === field) delete m.time_field;
  if ((m.trace_id as Record<string, unknown>)?.field === field) delete m.trace_id;
  if ((m.span_id as Record<string, unknown>)?.field === field) delete m.span_id;

  if (target === 'drop') return m;
  if (target === 'body') { m.body = { field }; return m; }
  if (target === 'severity_text') { m.severity_text = { field }; return m; }
  if (target === 'time_field') { m.time_field = { field, format: 'auto' }; return m; }
  if (target === 'trace_id') { m.trace_id = { field }; return m; }
  if (target === 'span_id') { m.span_id = { field }; return m; }
  if (target === 'resource_attributes') {
    m.resource_attributes = { ...((m.resource_attributes as object) ?? {}), [field]: { field } };
    return m;
  }
  m.log_attributes = { ...((m.log_attributes as object) ?? {}), [field]: { field } };
  return m;
}

export function OtlpMapper({ definition, onChange, onNext }: Props) {
  // No fallback to ['$raw'] — if fields aren't set yet the user needs to go back to Step 2
  const currentFields = (definition.parsed_fields as string[]) ?? [];

  const [fieldTargets, setFieldTargets] = useState<Record<string, string>>(() =>
    Object.fromEntries(currentFields.map(f => [f, defaultTarget(f)])),
  );

  // Sync field targets when fields change (user went back to Step 2 and updated parser)
  useEffect(() => {
    setFieldTargets(prev => {
      const next = { ...prev };
      let changed = false;
      currentFields.forEach(f => {
        if (!(f in next)) { next[f] = defaultTarget(f); changed = true; }
      });
      return changed ? next : prev;
    });
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [JSON.stringify(currentFields)]);

  // Initialise mapping and output defaults on first render
  useEffect(() => {
    let mapping = (definition.mapping as Record<string, unknown>) ?? {};
    currentFields.forEach(f => { mapping = applyMapping(mapping, f, defaultTarget(f)); });
    const output = (definition.output as Record<string, unknown>) ?? {};
    onChange({
      ...definition,
      mapping,
      output: { endpoint: '${OTLP_ENDPOINT}', protocol: 'grpc', ...output },
    });
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const handleChange = (field: string, target: string) => {
    setFieldTargets(prev => ({ ...prev, [field]: target }));
    let mapping = (definition.mapping as Record<string, unknown>) ?? {};
    mapping = applyMapping(mapping, field, target);
    onChange({ ...definition, mapping });
  };

  return (
    <div>
      <h2>Step 3 — OTLP Mapping</h2>
      <p>Map each extracted field to its destination in the OTel LogRecord.</p>

      {currentFields.length === 0 && (
        <p style={{ color: '#c66', fontSize: 13, background: '#fff0f0', padding: '8px 12px', borderRadius: 4 }}>
          ⚠ No fields available — go back to Step 2, paste sample data, and wait for the parse preview to appear.
        </p>
      )}

      {currentFields.length > 0 && (
        <table style={{ width: '100%', borderCollapse: 'collapse' }}>
          <thead>
            <tr style={{ background: '#f4f4f4' }}>
              <th style={{ textAlign: 'left', padding: '6px 8px' }}>Extracted field</th>
              <th style={{ textAlign: 'left', padding: '6px 8px' }}>OTLP target</th>
            </tr>
          </thead>
          <tbody>
            {currentFields.map(field => (
              <tr key={field} style={{ borderBottom: '1px solid #eee' }}>
                <td style={{ padding: '6px 8px', fontFamily: 'monospace', fontSize: 13 }}>{field}</td>
                <td style={{ padding: '6px 8px' }}>
                  <select
                    value={fieldTargets[field] ?? defaultTarget(field)}
                    onChange={e => handleChange(field, e.target.value)}
                  >
                    {ALL_TARGETS.map(t => <option key={t} value={t}>{t}</option>)}
                  </select>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      <div style={{ marginTop: 20 }}>
        <label>OTLP endpoint<br />
          <input type="text" style={{ width: '100%' }}
            value={((definition.output as Record<string, unknown>)?.endpoint as string) ?? '${OTLP_ENDPOINT}'}
            onChange={e => onChange({
              ...definition,
              output: { ...((definition.output as object) ?? {}), endpoint: e.target.value },
            })} />
        </label>
        <label style={{ display: 'block', marginTop: 8 }}>Protocol<br />
          <select
            value={((definition.output as Record<string, unknown>)?.protocol as string) ?? 'grpc'}
            onChange={e => onChange({
              ...definition,
              output: { ...((definition.output as object) ?? {}), protocol: e.target.value },
            })}>
            <option value="grpc">gRPC</option>
            <option value="http">HTTP</option>
          </select>
        </label>
      </div>

      <button style={{ marginTop: 24 }} onClick={onNext} disabled={currentFields.length === 0}>
        Next →
      </button>
    </div>
  );
}
