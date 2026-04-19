import React, { useState } from 'react';
import { TransportSelector } from './components/TransportSelector';
import { ParserEditor } from './components/ParserEditor';
import { OtlpMapper } from './components/OtlpMapper';
import { PreviewPanel } from './components/PreviewPanel';
import { DownloadPanel } from './components/DownloadPanel';

export type Step = 'transport' | 'parser' | 'mapping' | 'download';

export default function App() {
  const [step, setStep] = useState<Step>('transport');
  const [definition, setDefinition] = useState<Record<string, unknown>>({
    version: '1',
  });

  return (
    <div style={{ fontFamily: 'sans-serif', maxWidth: 960, margin: '0 auto', padding: 24 }}>
      <h1>Collectable — Pipeline Builder</h1>
      <nav style={{ display: 'flex', gap: 16, marginBottom: 24 }}>
        {(['transport', 'parser', 'mapping', 'download'] as Step[]).map((s) => (
          <button key={s} onClick={() => setStep(s)}
            style={{ fontWeight: step === s ? 'bold' : 'normal' }}>
            {s.charAt(0).toUpperCase() + s.slice(1)}
          </button>
        ))}
      </nav>

      {step === 'transport' && (
        <TransportSelector definition={definition} onChange={setDefinition} onNext={() => setStep('parser')} />
      )}
      {step === 'parser' && (
        <ParserEditor definition={definition} onChange={setDefinition} onNext={() => setStep('mapping')} />
      )}
      {step === 'mapping' && (
        <OtlpMapper definition={definition} onChange={setDefinition} onNext={() => setStep('download')} />
      )}
      {step === 'download' && (
        <DownloadPanel definition={definition} />
      )}

      <details style={{ marginTop: 32 }}>
        <summary>Pipeline definition (JSON)</summary>
        <pre style={{ background: '#f4f4f4', padding: 16, overflow: 'auto' }}>
          {JSON.stringify(definition, null, 2)}
        </pre>
      </details>

      <PreviewPanel definition={definition} />
    </div>
  );
}
