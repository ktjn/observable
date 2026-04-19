import React, { useState, useEffect } from 'react';

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

function extractFields(
  parserType: string,
  params: Record<string, unknown>,
  sample: string,
): string[] {
  const fields: string[] = [];
  const firstLine = sample.split('\n').find(l => l.trim()) ?? '';

  try {
    switch (parserType) {
      case 'json': {
        const obj = JSON.parse(firstLine);
        fields.push(...Object.keys(obj));
        break;
      }
      case 'regex': {
        const pattern = (params.pattern as string) ?? '';
        const groupRe = /\(\?P?<(\w+)>/g;
        let m;
        while ((m = groupRe.exec(pattern)) !== null) fields.push(m[1]);
        break;
      }
      case 'grok': {
        const pattern = (params.pattern as string) ?? '';
        const grokRe = /%\{[^:}]+:(\w+)\}/g;
        let m;
        while ((m = grokRe.exec(pattern)) !== null) fields.push(m[1]);
        break;
      }
      case 'log4j2_pattern': {
        const p = (params.pattern as string) ?? '';
        if (/%d/.test(p)) fields.push('timestamp');
        if (/%-?\d*level|%level/.test(p)) fields.push('level');
        if (/%msg|%m(?!\w)/.test(p)) fields.push('message');
        if (/%logger|%c(?!\w)/.test(p)) fields.push('logger');
        if (/%thread|%t(?!\w)/.test(p)) fields.push('thread');
        if (/%ex|%exception|%throwable/.test(p)) fields.push('exception');
        break;
      }
      case 'log4j2_json':
        fields.push('timeMillis', 'thread', 'level', 'loggerName', 'message');
        break;
      case 'key_value': {
        const sep = (params.separator as string) ?? ' ';
        firstLine.split(sep).forEach(pair => {
          const eq = pair.indexOf('=');
          if (eq > 0) fields.push(pair.substring(0, eq).trim());
        });
        break;
      }
      case 'csv': {
        const delim = (params.delimiter as string) ?? ',';
        fields.push(
          ...firstLine.split(delim).map(h => h.trim().replace(/^"|"$/g, '')).filter(Boolean),
        );
        break;
      }
      case 'passthrough':
      default:
        break;
    }
  } catch {
    // ignore parse errors — user will see 0 fields and can still proceed
  }

  // Always include $raw and deduplicate
  return ['$raw', ...new Set(fields)];
}

interface Props {
  definition: Record<string, unknown>;
  onChange: (d: Record<string, unknown>) => void;
  onNext: () => void;
}

export function ParserEditor({ definition, onChange, onNext }: Props) {
  const parser = (definition.parser as Record<string, unknown>) ?? {};
  const [sample, setSample] = useState((definition._sample as string) ?? '');
  const [fields, setFields] = useState<string[]>((definition.parsed_fields as string[]) ?? []);

  // Re-extract fields whenever sample or parser config changes
  useEffect(() => {
    if (!parser.type || !sample.trim()) return;
    const extracted = extractFields(parser.type as string, parser, sample);
    setFields(extracted);
    onChange({ ...definition, parsed_fields: extracted, _sample: sample });
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sample, parser.type, (parser as Record<string,unknown>).pattern, (parser as Record<string,unknown>).separator, (parser as Record<string,unknown>).delimiter]);

  const set = (patch: Record<string, unknown>) =>
    onChange({ ...definition, parser: { ...parser, ...patch } });

  return (
    <div>
      <h2>Step 2 — Parser</h2>

      <label>Sample log lines (paste 5–50 lines)<br />
        <textarea rows={8} style={{ width: '100%', fontFamily: 'monospace', fontSize: 12 }}
          value={sample}
          onChange={e => setSample(e.target.value)}
          placeholder="Paste sample log output here…" />
      </label>

      <label style={{ display: 'block', marginTop: 12 }}>Parser type<br />
        <select value={(parser.type as string) ?? ''} onChange={e => set({ type: e.target.value })}>
          <option value="">Select…</option>
          {PARSERS.map(p => <option key={p.id} value={p.id}>{p.label}</option>)}
        </select>
      </label>

      {parser.type === 'grok' && (
        <label style={{ display: 'block', marginTop: 12 }}>Grok pattern<br />
          <input type="text" style={{ width: '100%' }}
            placeholder="%{TIMESTAMP_ISO8601:timestamp} %{LOGLEVEL:level} %{GREEDYDATA:message}"
            onChange={e => set({ pattern: e.target.value })} />
        </label>
      )}

      {parser.type === 'regex' && (
        <label style={{ display: 'block', marginTop: 12 }}>Regex (named groups)<br />
          <input type="text" style={{ width: '100%' }}
            placeholder="(?P<timestamp>\d{4}-\d{2}-\d{2}) (?P<level>\w+) (?P<message>.+)"
            onChange={e => set({ pattern: e.target.value })} />
        </label>
      )}

      {parser.type === 'log4j2_pattern' && (
        <label style={{ display: 'block', marginTop: 12 }}>PatternLayout string<br />
          <input type="text" style={{ width: '100%' }}
            placeholder="%d{ISO8601} [%t] %-5level %logger{36} - %msg%n"
            onChange={e => set({ pattern: e.target.value })} />
        </label>
      )}

      {parser.type === 'key_value' && (
        <label style={{ display: 'block', marginTop: 12 }}>Field separator<br />
          <input type="text" defaultValue=" " style={{ width: 60 }}
            onChange={e => set({ separator: e.target.value })} />
        </label>
      )}

      {parser.type === 'csv' && (
        <label style={{ display: 'block', marginTop: 12 }}>Delimiter<br />
          <input type="text" defaultValue="," style={{ width: 60 }}
            onChange={e => set({ delimiter: e.target.value })} />
        </label>
      )}

      {fields.length > 1 && (
        <div style={{ marginTop: 16, background: '#f0f8f0', padding: 10, borderRadius: 4 }}>
          <strong>Extracted fields:</strong>{' '}
          {fields.map(f => (
            <code key={f} style={{ marginRight: 8, background: '#e0ece0', padding: '2px 6px', borderRadius: 3 }}>
              {f}
            </code>
          ))}
        </div>
      )}

      <button style={{ marginTop: 24 }} onClick={onNext} disabled={!parser.type}>
        Next →
      </button>
    </div>
  );
}
