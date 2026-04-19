import React from 'react';

const TRANSPORTS = [
  { id: 'syslog_tcp', label: 'Syslog TCP (RFC3164 / RFC5424)' },
  { id: 'syslog_udp', label: 'Syslog UDP' },
  { id: 'http_webhook', label: 'HTTP Webhook (Firehose, Heroku, Splunk HEC, generic)' },
  { id: 'mqtt', label: 'MQTT topic subscriber' },
  { id: 'kafka', label: 'Kafka consumer group' },
  { id: 'file_tail', label: 'File tail' },
  { id: 'stdin', label: 'Standard input' },
];

/** Seed sensible defaults into the transport object when a type is first selected. */
function defaultsForType(type: string): Record<string, unknown> {
  if (type === 'syslog_tcp' || type === 'syslog_udp') return { port: 514 };
  if (type === 'http_webhook') return { port: 8080 };
  if (type === 'kafka') return { bootstrap_servers: '', topic: '', group_id: 'collectable' };
  if (type === 'mqtt') return { broker: '', topic: '' };
  if (type === 'file_tail') return { path: '' };
  return {};
}

interface Props {
  definition: Record<string, unknown>;
  onChange: (d: Record<string, unknown>) => void;
  onNext: () => void;
}

export function TransportSelector({ definition, onChange, onNext }: Props) {
  const transport = (definition.transport as Record<string, unknown>) ?? {};

  const set = (patch: Record<string, unknown>) =>
    onChange({ ...definition, transport: { ...transport, ...patch } });

  const handleTypeChange = (type: string) => {
    const existing = (definition.transport as Record<string, unknown>) ?? {};
    const defaults = existing.type === type ? {} : defaultsForType(type);
    onChange({ ...definition, transport: { ...defaults, ...existing, type } });
  };

  const canProceed = !!(definition.name as string)?.trim() && !!transport.type;

  return (
    <div>
      <h2>Step 1 — Transport</h2>

      <label style={{ display: 'block', marginBottom: 16 }}>Pipeline name
        <span style={{ color: '#c00', marginLeft: 2 }}>*</span><br />
        <input type="text" style={{ width: '100%' }}
          placeholder="e.g. syslog-to-otlp"
          value={(definition.name as string) ?? ''}
          onChange={e => onChange({ ...definition, name: e.target.value.replace(/\s+/g, '-').toLowerCase() })} />
        <small style={{ color: '#888' }}>Used as the binary name. Only lowercase letters, digits, and hyphens.</small>
      </label>

      <label>Transport type<br />
        <select value={(transport.type as string) ?? ''} onChange={e => handleTypeChange(e.target.value)}>
          <option value="">Select…</option>
          {TRANSPORTS.map(t => <option key={t.id} value={t.id}>{t.label}</option>)}
        </select>
      </label>

      {(transport.type === 'syslog_tcp' || transport.type === 'syslog_udp') && (
        <div style={{ marginTop: 12 }}>
          <label>Port<br />
            <input type="number"
              value={(transport.port as number) ?? 514}
              onChange={e => set({ port: Number(e.target.value) })} />
          </label>
        </div>
      )}

      {transport.type === 'http_webhook' && (
        <div style={{ marginTop: 12 }}>
          <label>Port<br />
            <input type="number"
              value={(transport.port as number) ?? 8080}
              onChange={e => set({ port: Number(e.target.value) })} />
          </label>
        </div>
      )}

      {transport.type === 'mqtt' && (
        <div style={{ marginTop: 12 }}>
          <label>Broker URL<br />
            <input type="text" placeholder="mqtt://localhost:1883"
              value={(transport.broker as string) ?? ''}
              onChange={e => set({ broker: e.target.value })} />
          </label>
          <label style={{ marginTop: 8, display: 'block' }}>Topic<br />
            <input type="text" placeholder="logs/#"
              value={(transport.topic as string) ?? ''}
              onChange={e => set({ topic: e.target.value })} />
          </label>
        </div>
      )}

      {transport.type === 'kafka' && (
        <div style={{ marginTop: 12 }}>
          <label>Bootstrap servers<br />
            <input type="text" placeholder="localhost:9092"
              value={(transport.bootstrap_servers as string) ?? ''}
              onChange={e => set({ bootstrap_servers: e.target.value })} />
          </label>
          <label style={{ marginTop: 8, display: 'block' }}>Topic<br />
            <input type="text"
              value={(transport.topic as string) ?? ''}
              onChange={e => set({ topic: e.target.value })} />
          </label>
          <label style={{ marginTop: 8, display: 'block' }}>Consumer group<br />
            <input type="text"
              value={(transport.group_id as string) ?? 'collectable'}
              onChange={e => set({ group_id: e.target.value })} />
          </label>
        </div>
      )}

      {transport.type === 'file_tail' && (
        <div style={{ marginTop: 12 }}>
          <label>File path (glob supported)<br />
            <input type="text" placeholder="/var/log/app/*.log"
              value={(transport.path as string) ?? ''}
              onChange={e => set({ path: e.target.value })} />
          </label>
        </div>
      )}

      <button style={{ marginTop: 24 }} onClick={onNext} disabled={!canProceed}>
        Next →
      </button>
    </div>
  );
}
