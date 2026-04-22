import React, { useState, useEffect, useRef } from 'react';

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

type ParsedRow = Record<string, string>;

/**
 * Fast client-side field extraction — no network.
 * Used immediately to populate definition.parsed_fields so Step 3
 * always has fields available even before the server preview returns.
 */
function extractFieldsImmediate(
  parserType: string,
  params: Record<string, unknown>,
  sample: string,
  includeRaw: boolean,
): string[] {
  if (parserType === 'passthrough') return ['$raw'];

  const firstLine = sample.split('\n').find(l => l.trim()) ?? '';
  let fields: string[] = [];

  try {
    if (parserType === 'json' || parserType === 'log4j2_json') {
      const obj = JSON.parse(firstLine) as Record<string, unknown>;
      fields = Object.keys(obj);
    } else if (parserType === 'key_value') {
      const sep = (params.separator as string) ?? ' ';
      firstLine.split(sep).forEach(pair => {
        const eq = pair.indexOf('=');
        if (eq > 0) { const k = pair.substring(0, eq).trim(); if (k) fields.push(k); }
      });
    } else if (parserType === 'csv') {
      const delim = (params.delimiter as string) ?? ',';
      fields = firstLine.split(delim).map(h => h.trim().replace(/^"|"$/g, '')).filter(Boolean);
    } else if (parserType === 'log4j2_pattern') {
      const p = (params.pattern as string) ?? '';
      if (/%d/.test(p)) fields.push('timestamp');
      if (/%-?\d*level|%level/.test(p)) fields.push('level');
      if (/%msg|%m(?!\w)/.test(p)) fields.push('message');
      if (/%logger|%c(?!\w)/.test(p)) fields.push('logger');
      if (/%thread|%t(?!\w)/.test(p)) fields.push('thread');
      if (/%ex|%exception|%throwable/.test(p)) fields.push('exception');
    } else if (parserType === 'grok') {
      const re = /%\{[^:}]+:(\w+)\}/g;
      let m; while ((m = re.exec((params.pattern as string) ?? '')) !== null) fields.push(m[1]);
    } else if (parserType === 'regex') {
      const re = /\(\?P?<(\w+)>/g;
      let m; while ((m = re.exec((params.pattern as string) ?? '')) !== null) fields.push(m[1]);
    }
  } catch { /* ignore */ }

  const unique = [...new Set(fields.filter(Boolean))];
  return includeRaw && unique.length > 0 ? ['$raw', ...unique] : unique;
}

const tdStyle: React.CSSProperties = {
  padding: '4px 8px', fontSize: 12, fontFamily: 'monospace',
  borderBottom: '1px solid #eee', maxWidth: 240,
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
  const [rows, setRows] = useState<ParsedRow[]>([]);
  const [parseError, setParseError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  // Keep a ref to the latest definition to avoid stale closure in async callbacks
  const definitionRef = useRef(definition);
  useEffect(() => { definitionRef.current = definition; });

  const set = (patch: Record<string, unknown>) =>
    onChange({ ...definition, parser: { ...parser, ...patch } });

  useEffect(() => {
    if (!parser.type || !sample.trim()) {
      setRows([]); setParseError(null); return;
    }

    // ── 1. Immediately store fields (client-side, no network) ──────────────
    // This ensures definition.parsed_fields is always set before the user
    // navigates to Step 3, even if the server fetch hasn't completed yet.
    const immediateFields = extractFieldsImmediate(parser.type as string, parser, sample, includeRaw);
    onChange({
      ...definitionRef.current,
      parsed_fields: immediateFields,
      _sample: sample,
      _includeRaw: includeRaw,
    });

    // ── 2. Debounced server fetch for live preview table ───────────────────
    const controller = new AbortController();
    const timer = setTimeout(async () => {
      setLoading(true);
      setParseError(null);
      try {
        const lines = sample.split('\n').filter(l => l.trim()).slice(0, 25);
        const resp = await fetch('/api/parse', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ parser, lines }),
          signal: controller.signal,
        });
        const data = await resp.json() as { rows: ParsedRow[]; error?: string };
        if (data.error) {
          setParseError(data.error);
          setRows([]);
        } else {
          setRows(data.rows ?? []);
          setParseError(null);
          // Refine field names from actual server-parsed row keys
          const rowKeys = [...new Set(
            (data.rows ?? []).flatMap(r => Object.keys(r)).filter(k => k !== '⚠'),
          )];
          const serverFields = rowKeys.length > 0 ? rowKeys : immediateFields;
          const withRaw = includeRaw && parser.type !== 'passthrough'
            ? ['$raw', ...serverFields.filter(f => f !== '$raw')]
            : serverFields;
          onChange({ ...definitionRef.current, parsed_fields: withRaw, _sample: sample, _includeRaw: includeRaw });
        }
      } catch (e) {
        if ((e as Error).name !== 'AbortError') { setParseError(String(e)); setRows([]); }
      } finally {
        setLoading(false);
      }
    }, 350);

    return () => { clearTimeout(timer); controller.abort(); };
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sample, includeRaw, parser.type,
    (parser as Record<string, unknown>).pattern,
    (parser as Record<string, unknown>).separator,
    (parser as Record<string, unknown>).delimiter]);

  const previewCols = rows.length ? [...new Set(rows.flatMap(r => Object.keys(r)))] : [];

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

      {!!parser.type && parser.type !== 'passthrough' && (
        <label style={{ display: 'block', marginTop: 12, fontSize: 13, color: '#555' }}>
          <input type="checkbox" checked={includeRaw}
            onChange={e => setIncludeRaw(e.target.checked)} />
          {' '}Include <code>$raw</code> field (original unparsed line)
        </label>
      )}

      {/* Live parse preview */}
      {!!parser.type && sample.trim() && (
        <div style={{ marginTop: 20 }}>
          <strong style={{ fontSize: 13 }}>Parse preview{loading ? ' ⟳' : ''}</strong>
          {parseError && (
            <pre style={{ color: '#c00', fontSize: 12, margin: '6px 0', whiteSpace: 'pre-wrap' }}>
              ⚠ {parseError}
            </pre>
          )}
          {!parseError && rows.length > 0 && (
            <div style={{ overflowX: 'auto', marginTop: 6 }}>
              <table style={{ borderCollapse: 'collapse', fontSize: 12, width: '100%' }}>
                <thead>
                  <tr style={{ background: '#f4f4f4' }}>
                    {previewCols.map(col => (
                      <th key={col} style={{ ...tdStyle, fontWeight: 600, background: '#f4f4f4' }}>{col}</th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {rows.map((row, i) => (
                    <tr key={i} style={{ background: i % 2 === 0 ? '#fff' : '#fafafa' }}>
                      {previewCols.map(col => (
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
          {!parseError && !loading && rows.length === 0 && (
            <p style={{ color: '#888', fontSize: 12, margin: '4px 0 0' }}>
              No rows returned — check the parser config above.
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
