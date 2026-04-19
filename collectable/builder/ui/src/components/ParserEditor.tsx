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

/** Extract field names from parser config and sample (for Step 3 mapping). */
function extractFields(
  parserType: string,
  params: Record<string, unknown>,
  sample: string,
  includeRaw: boolean,
): string[] {
  const fields: string[] = [];
  const firstLine = sample.split('\n').find(l => l.trim()) ?? '';

  try {
    switch (parserType) {
      case 'json':
      case 'log4j2_json': {
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
        return ['$raw'];
      default:
        break;
    }
  } catch {
    // ignore — user will see 0 fields in preview
  }

  const result = [...new Set(fields)];
  if (includeRaw && parserType !== 'passthrough') result.unshift('$raw');
  return result;
}

type ParsedRow = Record<string, string>;

/** Parse sample lines into field→value objects for live preview. */
function parseLines(
  parserType: string,
  params: Record<string, unknown>,
  sample: string,
): { rows: ParsedRow[]; note?: string } {
  const lines = sample.split('\n').filter(l => l.trim()).slice(0, 8);
  if (!lines.length) return { rows: [] };

  try {
    switch (parserType) {
      case 'json':
      case 'log4j2_json': {
        const rows = lines.map(line => {
          try {
            const obj = JSON.parse(line) as Record<string, unknown>;
            return Object.fromEntries(
              Object.entries(obj).map(([k, v]) => [k, typeof v === 'object' ? JSON.stringify(v) : String(v ?? '')]),
            );
          } catch { return { '⚠': 'invalid JSON' }; }
        });
        return { rows };
      }

      case 'regex': {
        const pattern = (params.pattern as string) ?? '';
        if (!pattern) return { rows: [], note: 'Enter a regex pattern above to see preview' };
        const re = new RegExp(pattern);
        const rows = lines.map(line => {
          const m = re.exec(line);
          if (!m?.groups) return { '⚠': `no match: ${line.slice(0, 50)}` };
          return Object.fromEntries(Object.entries(m.groups).map(([k, v]) => [k, v ?? '']));
        });
        return { rows };
      }

      case 'key_value': {
        const sep = (params.separator as string) ?? ' ';
        const rows = lines.map(line => {
          const result: ParsedRow = {};
          line.split(sep).forEach(pair => {
            const eq = pair.indexOf('=');
            if (eq > 0) result[pair.substring(0, eq).trim()] = pair.substring(eq + 1).trim();
          });
          return Object.keys(result).length ? result : { '⚠': `no key=value pairs found` };
        });
        return { rows };
      }

      case 'csv': {
        const delim = (params.delimiter as string) ?? ',';
        const allLines = sample.split('\n').filter(l => l.trim());
        const headers = allLines[0]?.split(delim).map(h => h.trim().replace(/^"|"$/g, '')) ?? [];
        const rows = allLines.slice(1, 9).map(line => {
          const vals = line.split(delim).map(v => v.trim().replace(/^"|"$/g, ''));
          return Object.fromEntries(headers.map((h, i) => [h, vals[i] ?? '']));
        });
        return { rows };
      }

      case 'passthrough':
        return { rows: lines.map(line => ({ $raw: line })) };

      case 'grok':
      case 'log4j2_pattern':
      case 'multiline':
        return { rows: [], note: 'Live preview not available for this parser — field names are extracted from the pattern above' };

      default:
        return { rows: [] };
    }
  } catch (e) {
    return { rows: [], note: `Preview error: ${String(e)}` };
  }
}

const tdStyle: React.CSSProperties = {
  padding: '4px 8px', fontSize: 12, fontFamily: 'monospace',
  borderBottom: '1px solid #eee', maxWidth: 220,
  overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
};

interface Props {
  definition: Record<string, unknown>;
  onChange: (d: Record<string, unknown>) => void;
  onNext: () => void;
}

export function ParserEditor({ definition, onChange, onNext }: Props) {
  const parser = (definition.parser as Record<string, unknown>) ?? {};
  const [sample, setSample] = useState((definition._sample as string) ?? '');
  const [includeRaw, setIncludeRaw] = useState((definition._includeRaw as boolean) ?? false);
  const [fields, setFields] = useState<string[]>((definition.parsed_fields as string[]) ?? []);
  const [preview, setPreview] = useState<{ rows: ParsedRow[]; note?: string }>({ rows: [] });

  useEffect(() => {
    if (!parser.type || !sample.trim()) { setPreview({ rows: [] }); return; }
    const extracted = extractFields(parser.type as string, parser, sample, includeRaw);
    const parsed = parseLines(parser.type as string, parser, sample);
    setFields(extracted);
    setPreview(parsed);
    onChange({ ...definition, parsed_fields: extracted, _sample: sample, _includeRaw: includeRaw });
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sample, includeRaw, parser.type,
    (parser as Record<string,unknown>).pattern,
    (parser as Record<string,unknown>).separator,
    (parser as Record<string,unknown>).delimiter]);

  const set = (patch: Record<string, unknown>) =>
    onChange({ ...definition, parser: { ...parser, ...patch } });

  // Preview table column headers: union of all keys across rows
  const previewColumns = preview.rows.length
    ? [...new Set(preview.rows.flatMap(r => Object.keys(r)))]
    : [];

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
            value={(parser.pattern as string) ?? ''}
            onChange={e => set({ pattern: e.target.value })} />
        </label>
      )}

      {parser.type === 'regex' && (
        <label style={{ display: 'block', marginTop: 12 }}>Regex (named groups)<br />
          <input type="text" style={{ width: '100%' }}
            placeholder="(?P<timestamp>\d{4}-\d{2}-\d{2}) (?P<level>\w+) (?P<message>.+)"
            value={(parser.pattern as string) ?? ''}
            onChange={e => set({ pattern: e.target.value })} />
        </label>
      )}

      {parser.type === 'log4j2_pattern' && (
        <label style={{ display: 'block', marginTop: 12 }}>PatternLayout string<br />
          <input type="text" style={{ width: '100%' }}
            placeholder="%d{ISO8601} [%t] %-5level %logger{36} - %msg%n"
            value={(parser.pattern as string) ?? ''}
            onChange={e => set({ pattern: e.target.value })} />
        </label>
      )}

      {parser.type === 'key_value' && (
        <label style={{ display: 'block', marginTop: 12 }}>Field separator<br />
          <input type="text" style={{ width: 60 }}
            value={(parser.separator as string) ?? ' '}
            onChange={e => set({ separator: e.target.value })} />
        </label>
      )}

      {parser.type === 'csv' && (
        <label style={{ display: 'block', marginTop: 12 }}>Delimiter<br />
          <input type="text" style={{ width: 60 }}
            value={(parser.delimiter as string) ?? ','}
            onChange={e => set({ delimiter: e.target.value })} />
        </label>
      )}

      {/* $raw option — only relevant for structured parsers */}
      {parser.type && parser.type !== 'passthrough' && (
        <label style={{ display: 'block', marginTop: 12, fontSize: 13, color: '#555' }}>
          <input type="checkbox" checked={includeRaw}
            onChange={e => setIncludeRaw(e.target.checked)} />
          {' '}Include <code>$raw</code> field (original unparsed line — useful for debugging)
        </label>
      )}

      {/* Live parse preview */}
      {parser.type && sample.trim() && (
        <div style={{ marginTop: 20 }}>
          <strong style={{ fontSize: 13 }}>Parse preview</strong>
          {preview.note && (
            <p style={{ color: '#888', fontSize: 12, margin: '4px 0 0' }}>{preview.note}</p>
          )}
          {preview.rows.length > 0 && (
            <div style={{ overflowX: 'auto', marginTop: 6 }}>
              <table style={{ borderCollapse: 'collapse', fontSize: 12, width: '100%' }}>
                <thead>
                  <tr style={{ background: '#f4f4f4' }}>
                    {previewColumns.map(col => (
                      <th key={col} style={{ ...tdStyle, fontFamily: 'monospace', fontWeight: 600, background: '#f4f4f4' }}>
                        {col}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {preview.rows.map((row, i) => (
                    <tr key={i} style={{ background: i % 2 === 0 ? '#fff' : '#fafafa' }}>
                      {previewColumns.map(col => (
                        <td key={col} style={tdStyle} title={row[col] ?? ''}>
                          {row[col] ?? <span style={{ color: '#bbb' }}>—</span>}
                        </td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
          {!preview.rows.length && !preview.note && fields.length > 0 && (
            <p style={{ color: '#888', fontSize: 12, margin: '4px 0 0' }}>
              Fields identified from pattern: {fields.join(', ')}
            </p>
          )}
        </div>
      )}

      <button style={{ marginTop: 24 }} onClick={onNext} disabled={!parser.type}>
        Next →
      </button>
    </div>
  );
}
