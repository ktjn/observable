import React from 'react';

interface Props {
  definition: Record<string, unknown>;
  onChange: (d: Record<string, unknown>) => void;
  onNext: () => void;
}

export function PublisherPanel({ definition, onChange, onNext }: Props) {
  const output = (definition.output as Record<string, unknown>) ?? {};

  const flushSecs = (output.flush_interval_secs as number) ?? 5;
  const batchSize = (output.batch_size as number) ?? 100;

  const set = (patch: Record<string, unknown>) =>
    onChange({ ...definition, output: { ...output, ...patch } });

  return (
    <div>
      <h2>Step 4 — Publisher</h2>
      <p style={{ fontSize: 13, color: '#555', marginBottom: 16 }}>
        Log records are buffered and sent to the OTLP endpoint in batches.
        A batch is flushed when <strong>either</strong> the send delay expires
        <strong> or</strong> the message count is reached — whichever comes first.
      </p>

      <div style={{ display: 'flex', flexDirection: 'column', gap: 16, maxWidth: 320 }}>
        <label>
          Send delay — N seconds<br />
          <input
            type="number"
            min={1}
            value={flushSecs}
            onChange={e => set({ flush_interval_secs: Math.max(1, Number(e.target.value)) })}
            style={{ width: '100%' }}
          />
          <small style={{ color: '#888' }}>Flush buffered records at least every N seconds.</small>
        </label>

        <label>
          Send count — M messages<br />
          <input
            type="number"
            min={1}
            value={batchSize}
            onChange={e => set({ batch_size: Math.max(1, Number(e.target.value)) })}
            style={{ width: '100%' }}
          />
          <small style={{ color: '#888' }}>Flush immediately when M records are buffered.</small>
        </label>
      </div>

      <div style={{ marginTop: 24 }}>
        <button onClick={onNext}>Next →</button>
      </div>
    </div>
  );
}
