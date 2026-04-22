import React, { useState, useEffect } from 'react';

interface Props {
  definition: Record<string, unknown>;
  onChange: (d: Record<string, unknown>) => void;
  onNext: () => void;
}

// Well-known OTel resource semantic convention keys for datalist suggestions.
const OTEL_RESOURCE_KEYS = [
  'service.name', 'service.version', 'service.namespace', 'service.instance.id',
  'host.name', 'host.id', 'host.type', 'host.arch',
  'os.type', 'os.description', 'os.version',
  'process.pid', 'process.executable.name', 'process.command_line', 'process.owner',
  'container.id', 'container.name', 'container.image.name', 'container.image.tag',
  'k8s.cluster.name', 'k8s.namespace.name', 'k8s.pod.name', 'k8s.node.name',
  'k8s.deployment.name',
  'cloud.provider', 'cloud.region', 'cloud.availability_zone', 'cloud.account.id',
  'deployment.environment',
];

const STANDARD_TARGETS = ['body', 'severity_text', 'time_field', 'trace_id', 'span_id', 'drop'];

type ResourceSourceType = 'env' | 'command' | 'literal';

interface ResourceAttrRow {
  id: string;
  otlpKey: string;
  sourceType: ResourceSourceType;
  value: string;
}

interface LogAttrRow {
  id: string;
  otlpKey: string;
  field: string;
}

function defaultTarget(field: string): string {
  if (field === '$raw') return 'body';
  const f = field.toLowerCase();
  if (/^(timestamp|time|date|datetime|@timestamp|timemillis)$/.test(f)) return 'time_field';
  if (/^(level|severity|log_?level|loglevel|priority)$/.test(f)) return 'severity_text';
  if (/^(message|msg|body|text|log)$/.test(f)) return 'body';
  if (/^trace_?id$/.test(f)) return 'trace_id';
  if (/^span_?id$/.test(f)) return 'span_id';
  return 'log_attributes';
}

function uid() {
  return Math.random().toString(36).slice(2);
}

function rowsToResourceAttrs(rows: ResourceAttrRow[]): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const row of rows) {
    if (!row.otlpKey.trim()) continue;
    if (row.sourceType === 'env') out[row.otlpKey] = { env: row.value };
    else if (row.sourceType === 'command') out[row.otlpKey] = { command: row.value };
    else out[row.otlpKey] = { literal: row.value };
  }
  return out;
}

function rowsToLogAttrs(rows: LogAttrRow[]): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const row of rows) {
    if (!row.otlpKey.trim() || !row.field) continue;
    out[row.otlpKey] = { field: row.field };
  }
  return out;
}

function attrsToResourceRows(attrs: Record<string, unknown> | undefined): ResourceAttrRow[] {
  if (!attrs) return [];
  return Object.entries(attrs).map(([key, val]) => {
    const v = val as Record<string, string>;
    if (v.env !== undefined) return { id: uid(), otlpKey: key, sourceType: 'env' as const, value: v.env };
    if (v.command !== undefined) return { id: uid(), otlpKey: key, sourceType: 'command' as const, value: v.command };
    return { id: uid(), otlpKey: key, sourceType: 'literal' as const, value: v.literal ?? '' };
  });
}

function attrsToLogRows(attrs: Record<string, unknown> | undefined): LogAttrRow[] {
  if (!attrs) return [];
  return Object.entries(attrs).map(([key, val]) => ({
    id: uid(),
    otlpKey: key,
    field: (val as Record<string, string>).field ?? '',
  }));
}

export function OtlpMapper({ definition, onChange, onNext }: Props) {
  const currentFields = (definition.parsed_fields as string[]) ?? [];
  const mapping = (definition.mapping as Record<string, unknown>) ?? {};

  const [fieldTargets, setFieldTargets] = useState<Record<string, string>>(() =>
    Object.fromEntries(currentFields.map(f => [f, defaultTarget(f)])),
  );

  const [resourceRows, setResourceRows] = useState<ResourceAttrRow[]>(() =>
    attrsToResourceRows(mapping.resource_attributes as Record<string, unknown> | undefined),
  );

  const [logRows, setLogRows] = useState<LogAttrRow[]>(() => {
    const existing = attrsToLogRows(mapping.log_attributes as Record<string, unknown> | undefined);
    // Pre-populate from parser fields that default to log_attributes and aren't already mapped
    const existingFields = new Set(existing.map(r => r.field));
    const pre = currentFields
      .filter(f => defaultTarget(f) === 'log_attributes' && !existingFields.has(f))
      .map(f => ({ id: uid(), otlpKey: f, field: f }));
    return [...existing, ...pre];
  });

  // Sync new parser fields into log rows when user goes back and changes parser
  useEffect(() => {
    setFieldTargets(prev => {
      const next = { ...prev };
      let changed = false;
      currentFields.forEach(f => {
        if (!(f in next)) { next[f] = defaultTarget(f); changed = true; }
      });
      return changed ? next : prev;
    });

    setLogRows(prev => {
      const existingFields = new Set(prev.map(r => r.field));
      const toAdd = currentFields
        .filter(f => defaultTarget(f) === 'log_attributes' && !existingFields.has(f))
        .map(f => ({ id: uid(), otlpKey: f, field: f }));
      return toAdd.length > 0 ? [...prev, ...toAdd] : prev;
    });
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [JSON.stringify(currentFields)]);

  // Initialise output defaults on first render
  useEffect(() => {
    const output = (definition.output as Record<string, unknown>) ?? {};
    onChange({
      ...definition,
      output: { endpoint: '${OTLP_ENDPOINT}', protocol: 'grpc', ...output },
    });
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Recompute and propagate mapping whenever any section changes
  const propagate = (
    ft: Record<string, string>,
    rr: ResourceAttrRow[],
    lr: LogAttrRow[],
  ) => {
    let m: Record<string, unknown> = {};

    // Standard field mappings
    for (const [field, target] of Object.entries(ft)) {
      if (target === 'drop') continue;
      if (target === 'body') { m.body = { field }; continue; }
      if (target === 'severity_text') { m.severity_text = { field }; continue; }
      if (target === 'time_field') { m.time_field = { field, format: 'auto' }; continue; }
      if (target === 'trace_id') { m.trace_id = { field }; continue; }
      if (target === 'span_id') { m.span_id = { field }; continue; }
    }

    const ra = rowsToResourceAttrs(rr);
    if (Object.keys(ra).length > 0) m.resource_attributes = ra;

    const la = rowsToLogAttrs(lr);
    if (Object.keys(la).length > 0) m.log_attributes = la;

    onChange({ ...definition, mapping: m });
  };

  const handleFieldTarget = (field: string, target: string) => {
    const next = { ...fieldTargets, [field]: target };
    setFieldTargets(next);
    propagate(next, resourceRows, logRows);
  };

  const handleResourceChange = (updated: ResourceAttrRow[]) => {
    setResourceRows(updated);
    propagate(fieldTargets, updated, logRows);
  };

  const handleLogChange = (updated: LogAttrRow[]) => {
    setLogRows(updated);
    propagate(fieldTargets, resourceRows, updated);
  };

  const standardFields = currentFields.filter(f =>
    STANDARD_TARGETS.includes(fieldTargets[f] ?? defaultTarget(f)) ||
    (fieldTargets[f] ?? defaultTarget(f)) === 'drop',
  );

  return (
    <div>
      <h2>Step 3 — OTLP Mapping</h2>

      {currentFields.length === 0 && (
        <p style={{ color: '#c66', fontSize: 13, background: '#fff0f0', padding: '8px 12px', borderRadius: 4 }}>
          ⚠ No fields available — go back to Step 2, paste sample data, and wait for the parse preview.
        </p>
      )}

      {/* ── Standard field mappings ─────────────────────────────────────── */}
      {standardFields.length > 0 && (
        <section style={{ marginBottom: 24 }}>
          <h3 style={{ fontSize: 14, marginBottom: 8 }}>Standard field mappings</h3>
          <table style={{ width: '100%', borderCollapse: 'collapse' }}>
            <thead>
              <tr style={{ background: '#f4f4f4' }}>
                <th style={{ textAlign: 'left', padding: '6px 8px', fontSize: 13 }}>Extracted field</th>
                <th style={{ textAlign: 'left', padding: '6px 8px', fontSize: 13 }}>OTLP target</th>
              </tr>
            </thead>
            <tbody>
              {standardFields.map(field => (
                <tr key={field} style={{ borderBottom: '1px solid #eee' }}>
                  <td style={{ padding: '6px 8px', fontFamily: 'monospace', fontSize: 13 }}>{field}</td>
                  <td style={{ padding: '6px 8px' }}>
                    <select
                      value={fieldTargets[field] ?? defaultTarget(field)}
                      onChange={e => handleFieldTarget(field, e.target.value)}
                    >
                      {STANDARD_TARGETS.map(t => <option key={t} value={t}>{t}</option>)}
                    </select>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </section>
      )}

      {/* ── Log Attributes ──────────────────────────────────────────────── */}
      <section style={{ marginBottom: 24 }}>
        <h3 style={{ fontSize: 14, marginBottom: 4 }}>Log Attributes <span style={{ fontWeight: 'normal', color: '#666', fontSize: 12 }}>(per-record, from parsed fields)</span></h3>
        <table style={{ width: '100%', borderCollapse: 'collapse', marginBottom: 8 }}>
          <thead>
            <tr style={{ background: '#f4f4f4' }}>
              <th style={{ textAlign: 'left', padding: '6px 8px', fontSize: 13 }}>OTLP attribute key</th>
              <th style={{ textAlign: 'left', padding: '6px 8px', fontSize: 13 }}>Parsed field</th>
              <th style={{ padding: '6px 8px' }}></th>
            </tr>
          </thead>
          <tbody>
            {logRows.map((row, i) => (
              <tr key={row.id} style={{ borderBottom: '1px solid #eee' }}>
                <td style={{ padding: '6px 8px' }}>
                  <input
                    type="text"
                    value={row.otlpKey}
                    style={{ width: '100%', fontFamily: 'monospace', fontSize: 13 }}
                    placeholder="attribute.name"
                    onChange={e => {
                      const updated = logRows.map((r, j) => j === i ? { ...r, otlpKey: e.target.value } : r);
                      handleLogChange(updated);
                    }}
                  />
                </td>
                <td style={{ padding: '6px 8px' }}>
                  <select
                    value={row.field}
                    onChange={e => {
                      const updated = logRows.map((r, j) => j === i ? { ...r, field: e.target.value } : r);
                      handleLogChange(updated);
                    }}
                  >
                    <option value="">— select field —</option>
                    {currentFields.map(f => <option key={f} value={f}>{f}</option>)}
                  </select>
                </td>
                <td style={{ padding: '6px 8px' }}>
                  <button onClick={() => handleLogChange(logRows.filter((_, j) => j !== i))}>✕</button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        <button onClick={() => handleLogChange([...logRows, { id: uid(), otlpKey: '', field: currentFields[0] ?? '' }])}>
          + Add log attribute
        </button>
      </section>

      {/* ── Resource Attributes ─────────────────────────────────────────── */}
      <section style={{ marginBottom: 24 }}>
        <h3 style={{ fontSize: 14, marginBottom: 4 }}>Resource Attributes <span style={{ fontWeight: 'normal', color: '#666', fontSize: 12 }}>(static, set once at startup)</span></h3>
        <p style={{ fontSize: 12, color: '#666', margin: '0 0 8px' }}>
          Values are evaluated once when the binary starts — before any log lines are read.
          Use <code>env</code> for environment variables, <code>command</code> to run a shell command
          (e.g. <code>hostname -f</code>), or <code>literal</code> for a constant string.
        </p>
        <datalist id="otel-resource-keys">
          {OTEL_RESOURCE_KEYS.map(k => <option key={k} value={k} />)}
        </datalist>
        <table style={{ width: '100%', borderCollapse: 'collapse', marginBottom: 8 }}>
          <thead>
            <tr style={{ background: '#f4f4f4' }}>
              <th style={{ textAlign: 'left', padding: '6px 8px', fontSize: 13 }}>OTLP attribute key</th>
              <th style={{ textAlign: 'left', padding: '6px 8px', fontSize: 13 }}>Source</th>
              <th style={{ textAlign: 'left', padding: '6px 8px', fontSize: 13 }}>Value</th>
              <th style={{ padding: '6px 8px' }}></th>
            </tr>
          </thead>
          <tbody>
            {resourceRows.map((row, i) => (
              <tr key={row.id} style={{ borderBottom: '1px solid #eee' }}>
                <td style={{ padding: '6px 8px' }}>
                  <input
                    type="text"
                    list="otel-resource-keys"
                    value={row.otlpKey}
                    style={{ width: '100%', fontFamily: 'monospace', fontSize: 13 }}
                    placeholder="host.name"
                    onChange={e => {
                      const updated = resourceRows.map((r, j) => j === i ? { ...r, otlpKey: e.target.value } : r);
                      handleResourceChange(updated);
                    }}
                  />
                </td>
                <td style={{ padding: '6px 8px' }}>
                  <select
                    value={row.sourceType}
                    onChange={e => {
                      const updated = resourceRows.map((r, j) =>
                        j === i ? { ...r, sourceType: e.target.value as ResourceSourceType, value: '' } : r
                      );
                      handleResourceChange(updated);
                    }}
                  >
                    <option value="env">env</option>
                    <option value="command">command</option>
                    <option value="literal">literal</option>
                  </select>
                </td>
                <td style={{ padding: '6px 8px' }}>
                  {row.sourceType === 'env' && (
                    <input
                      type="text"
                      value={row.value}
                      style={{ width: '100%', fontFamily: 'monospace', fontSize: 13 }}
                      placeholder="HOSTNAME"
                      onChange={e => {
                        const updated = resourceRows.map((r, j) => j === i ? { ...r, value: e.target.value } : r);
                        handleResourceChange(updated);
                      }}
                    />
                  )}
                  {row.sourceType === 'command' && (
                    <span style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
                      <span style={{ fontFamily: 'monospace', fontSize: 13, color: '#888' }}>$( </span>
                      <input
                        type="text"
                        value={row.value}
                        style={{ flex: 1, fontFamily: 'monospace', fontSize: 13 }}
                        placeholder="hostname -f"
                        onChange={e => {
                          const updated = resourceRows.map((r, j) => j === i ? { ...r, value: e.target.value } : r);
                          handleResourceChange(updated);
                        }}
                      />
                      <span style={{ fontFamily: 'monospace', fontSize: 13, color: '#888' }}> )</span>
                    </span>
                  )}
                  {row.sourceType === 'literal' && (
                    <input
                      type="text"
                      value={row.value}
                      style={{ width: '100%', fontFamily: 'monospace', fontSize: 13 }}
                      placeholder="production"
                      onChange={e => {
                        const updated = resourceRows.map((r, j) => j === i ? { ...r, value: e.target.value } : r);
                        handleResourceChange(updated);
                      }}
                    />
                  )}
                </td>
                <td style={{ padding: '6px 8px' }}>
                  <button onClick={() => handleResourceChange(resourceRows.filter((_, j) => j !== i))}>✕</button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        <button onClick={() => handleResourceChange([...resourceRows, { id: uid(), otlpKey: '', sourceType: 'env', value: '' }])}>
          + Add resource attribute
        </button>
      </section>

      {/* ── OTLP output config ──────────────────────────────────────────── */}
      <section style={{ marginBottom: 24 }}>
        <h3 style={{ fontSize: 14, marginBottom: 8 }}>OTLP output</h3>
        <label>Endpoint<br />
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
      </section>

      <button onClick={onNext} disabled={currentFields.length === 0}>
        Next →
      </button>
    </div>
  );
}

