import React, { useState } from 'react';

const PARSERS = [
  { id: 'json', label: 'JSON (one object per line)' },
  { id: 'grok', label: 'Grok pattern' },
  { id: 'regex', label: 'Regex (named capture groups)' },
  { id: 'key_value', label: 'Key=Value pairs' },
  { id: 'multiline', label: 'Multiline assembler' },
  { id: 'log4j2_pattern', label: 'Log4j2 PatternLayout' },
  { id: 'log4j2_json', label: 'Log4j2 JSONLayout' },
  { id: 'csv', label: 'CSV / delimiter-separated' },
  { id: 'passthrough', label: 'Passthrough (raw body)' },
];

interface Props {
  definition: Record<string, unknown>;
  onChange: (d: Record<string, unknown>) => void;
  onNext: () => void;
}

export function ParserEditor({ definition, onChange, onNext }: Props) {
  const parser = (definition.parser as Record<string, unknown>) ?? {};
  const [sample, setSample] = useState('');

  const set = (patch: Record<string, unknown>) =>
    onChange({ ...definition, parser: { ...parser, ...patch } });

  return (
    <div>
      <h2>Step 2 — Parser</h2>

      <label>Sample log lines (paste 5–50 lines)<br />
        <textarea rows={8} style={{ width: '100%', fontFamily: 'monospace', fontSize: 12 }}
          value={sample} onChange={e => setSample(e.target.value)}
          placeholder="Paste sample log output here…" />
      </label>

      <label style={{ display: 'block', marginTop: 12 }}>Parser type<br />
        <select value={(parser.type as string) ?? ''} onChange={e => set({ type: e.target.value })}>
          <option value="">Select…</option>
          {PARSERS.map(p => <option key={p.id} value={p.id}>{p.label}</option>)}
        </select>
      </label>

      {(parser.type === 'grok') && (
        <label style={{ display: 'block', marginTop: 12 }}>Grok pattern<br />
          <input type="text" style={{ width: '100%' }}
            placeholder="%{TIMESTAMP_ISO8601:timestamp} %{LOGLEVEL:level} %{GREEDYDATA:message}"
            onChange={e => set({ pattern: e.target.value })} />
        </label>
      )}

      {(parser.type === 'regex') && (
        <label style={{ display: 'block', marginTop: 12 }}>Regex (named groups)<br />
          <input type="text" style={{ width: '100%' }}
            placeholder="(?P<timestamp>\d{4}-\d{2}-\d{2}) (?P<level>\w+) (?P<message>.+)"
            onChange={e => set({ pattern: e.target.value })} />
        </label>
      )}

      {(parser.type === 'log4j2_pattern') && (
        <label style={{ display: 'block', marginTop: 12 }}>PatternLayout string<br />
          <input type="text" style={{ width: '100%' }}
            placeholder="%d{ISO8601} [%t] %-5level %logger{36} - %msg%n"
            onChange={e => set({ pattern: e.target.value })} />
        </label>
      )}

      {(parser.type === 'key_value') && (
        <label style={{ display: 'block', marginTop: 12 }}>Field separator<br />
          <input type="text" defaultValue=" " style={{ width: 60 }}
            onChange={e => set({ separator: e.target.value })} />
        </label>
      )}

      {(parser.type === 'csv') && (
        <label style={{ display: 'block', marginTop: 12 }}>Delimiter<br />
          <input type="text" defaultValue="," style={{ width: 60 }}
            onChange={e => set({ delimiter: e.target.value })} />
        </label>
      )}

      <button style={{ marginTop: 24 }} onClick={onNext} disabled={!parser.type}>
        Next →
      </button>
    </div>
  );
}
