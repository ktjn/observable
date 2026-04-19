import React, { useState } from 'react';

interface Props {
  definition: Record<string, unknown>;
  onChange: (d: Record<string, unknown>) => void;
  onNext: () => void;
}

const ALL_TARGETS = ['time_field', 'body', 'severity_text', 'trace_id', 'span_id', 'resource_attributes', 'log_attributes', 'drop'];

/** Suggest a sensible default OTLP target for a field name. */
function defaultTarget(field: string): string {
  if (field === '$raw') return 'body';
  const f = field.toLowerCase();
  if (/^(timestamp|time|date|datetime|@timestamp)$/.test(f)) return 'time_field';
  if (/^(level|severity|log_?level|loglevel|priority)$/.test(f)) return 'severity_text';
  if (/^(message|msg|body|text|log)$/.test(f)) return 'body';
  if (/^trace_?id$/.test(f)) return 'trace_id';
  if (/^span_?id$/.test(f)) return 'span_id';
  return 'log_attributes';
}

function applyMapping(
  mapping: Record<string, unknown>,
  field: string,
  target: string,
): Record<string, unknown> {
  const m = { ...mapping };

  // Clear previous location for this field
  (['resource_attributes', 'log_attributes'] as const).forEach(section => {
    const s = m[section] as Record<string, unknown> | undefined;
    if (s && field in s) {
      const updated = { ...s };
      delete updated[field];
      m[section] = updated;
    }
  });
  if ((m.body as Record<string,unknown>)?.field === field) delete m.body;
  if ((m.severity_text as Record<string,unknown>)?.field === field) delete m.severity_text;
  if ((m.time_field as Record<string,unknown>)?.field === field) delete m.time_field;
  if ((m.trace_id as Record<string,unknown>)?.field === field) delete m.trace_id;
  if ((m.span_id as Record<string,unknown>)?.field === field) delete m.span_id;

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
  // log_attributes (default)
  m.log_attributes = { ...((m.log_attributes as object) ?? {}), [field]: { field } };
  return m;
}

export function OtlpMapper({ definition, onChange, onNext }: Props) {
  const fields = (definition.parsed_fields as string[]) ?? ['$raw'];
  const [fieldTargets, setFieldTargets] = useState<Record<string, string>>(() =>
    Object.fromEntries(fields.map(f => [f, defaultTarget(f)])),
  );

  // Sync if fields change (user went back to Step 2)
  const currentFields = (definition.parsed_fields as string[]) ?? ['$raw'];
  const missingInState = currentFields.filter(f => !(f in fieldTargets));
  if (missingInState.length > 0) {
    setFieldTargets(prev => ({
      ...prev,
      ...Object.fromEntries(missingInState.map(f => [f, defaultTarget(f)])),
    }));
  }

  const handleChange = (field: string, target: string) => {
    const newTargets = { ...fieldTargets, [field]: target };
    setFieldTargets(newTargets);
    let mapping = (definition.mapping as Record<string, unknown>) ?? {};
    mapping = applyMapping(mapping, field, target);
    onChange({ ...definition, mapping });
  };

  // Initialise mapping from defaults on first render
  React.useEffect(() => {
    let mapping = (definition.mapping as Record<string, unknown>) ?? {};
    fields.forEach(f => { mapping = applyMapping(mapping, f, defaultTarget(f)); });
    const output = (definition.output as Record<string, unknown>) ?? {};
    onChange({
      ...definition,
      mapping,
      output: { endpoint: '${OTLP_ENDPOINT}', protocol: 'grpc', ...output },
    });
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <div>
      <h2>Step 3 — OTLP Mapping</h2>
      <p>Map each extracted field to its destination in the OTel LogRecord.</p>

      {currentFields.length === 1 && currentFields[0] === '$raw' && (
        <p style={{ color: '#c66', fontSize: 13 }}>
          ⚠ No fields extracted — go back to Step 2, paste sample data, and confirm the parser config.
        </p>
      )}

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

      <div style={{ marginTop: 20 }}>
        <label>OTLP endpoint<br />
          <input type="text" style={{ width: '100%' }}
            value={((definition.output as Record<string,unknown>)?.endpoint as string) ?? '${OTLP_ENDPOINT}'}
            onChange={e => onChange({
              ...definition,
              output: { ...((definition.output as object) ?? {}), endpoint: e.target.value },
            })} />
        </label>
        <label style={{ display: 'block', marginTop: 8 }}>Protocol<br />
          <select
            value={((definition.output as Record<string,unknown>)?.protocol as string) ?? 'grpc'}
            onChange={e => onChange({
              ...definition,
              output: { ...((definition.output as object) ?? {}), protocol: e.target.value },
            })}>
            <option value="grpc">gRPC</option>
            <option value="http">HTTP</option>
          </select>
        </label>
      </div>

      <button style={{ marginTop: 24 }} onClick={onNext}>Next →</button>
    </div>
  );
}
