import React, { useRef, useState } from 'react';
import { TransportSelector } from './components/TransportSelector';
import { ParserEditor } from './components/ParserEditor';
import { OtlpMapper } from './components/OtlpMapper';
import { PublisherPanel } from './components/PublisherPanel';
import { PreviewPanel } from './components/PreviewPanel';
import { DownloadPanel } from './components/DownloadPanel';

export type Step = 'transport' | 'parser' | 'mapping' | 'publisher' | 'download';

const STEP_LABELS: Record<Step, string> = {
  transport: '1 Collector',
  parser: '2 Parser',
  mapping: '3 Mapping',
  publisher: '4 Publisher',
  download: '5 Download',
};

export default function App() {
  const [step, setStep] = useState<Step>('transport');
  const [definition, setDefinition] = useState<Record<string, unknown>>({ version: '1' });
  const [importError, setImportError] = useState<string | null>(null);
  const fileRef = useRef<HTMLInputElement>(null);

  const loadDefinition = (json: string) => {
    try {
      const parsed = JSON.parse(json) as Record<string, unknown>;
      // Accept either a bare pipeline definition or a build-request wrapper
      const def = (parsed.definition as Record<string, unknown> | undefined) ?? parsed;
      if (!def.version) throw new Error('Not a valid pipeline definition (missing "version")');
      setDefinition(def);
      setStep('transport');
      setImportError(null);
    } catch (e: unknown) {
      setImportError(e instanceof Error ? e.message : String(e));
    }
  };

  const onFileChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = () => loadDefinition(reader.result as string);
    reader.readAsText(file);
    // Reset so re-selecting same file triggers change
    e.target.value = '';
  };

  const exportDefinition = () => {
    const json = JSON.stringify(definition, null, 2);
    const blob = new Blob([json], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `${(definition.name as string) ?? 'pipeline'}.json`;
    a.click();
    URL.revokeObjectURL(url);
  };

  return (
    <div style={{ fontFamily: 'sans-serif', maxWidth: 960, margin: '0 auto', padding: 24 }}>
      <div style={{ display: 'flex', alignItems: 'baseline', justifyContent: 'space-between', flexWrap: 'wrap', gap: 8 }}>
        <h1 style={{ margin: 0 }}>Collectable — Pipeline Builder</h1>
        <div style={{ display: 'flex', gap: 8 }}>
          <button
            style={{ fontSize: 13 }}
            title="Load a previously saved pipeline.json"
            onClick={() => fileRef.current?.click()}
          >
            📂 Load pipeline.json
          </button>
          <button
            style={{ fontSize: 13 }}
            title="Save current definition as pipeline.json"
            onClick={exportDefinition}
          >
            💾 Save pipeline.json
          </button>
          <input
            ref={fileRef}
            type="file"
            accept=".json,application/json"
            style={{ display: 'none' }}
            onChange={onFileChange}
          />
        </div>
      </div>

      {importError && (
        <div style={{ color: 'red', fontSize: 13, marginTop: 8 }}>
          Import error: {importError}
        </div>
      )}

      <nav style={{ display: 'flex', gap: 16, margin: '20px 0' }}>
        {(Object.keys(STEP_LABELS) as Step[]).map((s) => (
          <button
            key={s}
            onClick={() => setStep(s)}
            style={{
              fontWeight: step === s ? 'bold' : 'normal',
              textDecoration: step === s ? 'underline' : 'none',
            }}
          >
            {STEP_LABELS[s]}
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
        <OtlpMapper definition={definition} onChange={setDefinition} onNext={() => setStep('publisher')} />
      )}
      {step === 'publisher' && (
        <PublisherPanel definition={definition} onChange={setDefinition} onNext={() => setStep('download')} />
      )}
      {step === 'download' && (
        <DownloadPanel definition={definition} onChange={setDefinition} />
      )}

      <details style={{ marginTop: 32 }}>
        <summary style={{ cursor: 'pointer', fontSize: 13 }}>Pipeline definition (JSON)</summary>
        <pre style={{ background: '#f4f4f4', padding: 16, overflow: 'auto', fontSize: 12 }}>
          {JSON.stringify(definition, null, 2)}
        </pre>
      </details>

      <PreviewPanel definition={definition} />
    </div>
  );
}
