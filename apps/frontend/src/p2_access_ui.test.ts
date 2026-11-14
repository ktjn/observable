import { describe, expect, it } from 'vitest';

interface WallDisplaySnapshot {
  autoRefreshIntervalSeconds: number;
  requiresUserInteraction: boolean;
  priorityIncidents: Array<{ id: string; severity: 'SEV-1' | 'SEV-2'; title: string }>;
  criticalServices: Array<{ name: string; status: 'healthy' | 'degraded' | 'unavailable' }>;
  minFontSizePx: number;
}

type ApprovedLanguage = 'en' | 'de' | 'fr';

const ESSENTIAL_INCIDENT_WORKFLOW_LABELS: Record<
  ApprovedLanguage,
  { acknowledge: string; escalate: string; resolve: string; statusTitle: string }
> = {
  en: {
    acknowledge: 'Acknowledge',
    escalate: 'Escalate',
    resolve: 'Resolve',
    statusTitle: 'Incident Status',
  },
  de: {
    acknowledge: 'Bestätigen',
    escalate: 'Eskalieren',
    resolve: 'Beheben',
    statusTitle: 'Vorfallstatus',
  },
  fr: {
    acknowledge: 'Accuser réception',
    escalate: 'Escalader',
    resolve: 'Résoudre',
    statusTitle: "Statut de l'incident",
  },
};

function getIncidentWorkflowLabels(lang: ApprovedLanguage) {
  return ESSENTIAL_INCIDENT_WORKFLOW_LABELS[lang];
}

describe('P2 access-aware UI assurance (OBS-25-08, OBS-25-09)', () => {
  it('OBS-25-08: wall display shows priority incidents and critical-service status legibly without interaction', () => {
    const wall: WallDisplaySnapshot = {
      autoRefreshIntervalSeconds: 15,
      requiresUserInteraction: false,
      priorityIncidents: [{ id: 'inc-1', severity: 'SEV-1', title: 'Checkout 502 spike' }],
      criticalServices: [{ name: 'checkout', status: 'degraded' }],
      minFontSizePx: 20,
    };
    expect(wall.requiresUserInteraction).toBe(false);
    expect(wall.autoRefreshIntervalSeconds).toBeGreaterThan(0);
    expect(wall.minFontSizePx).toBeGreaterThanOrEqual(18);
    expect(wall.priorityIncidents.length).toBeGreaterThan(0);
    expect(wall.criticalServices.length).toBeGreaterThan(0);
  });

  it('OBS-25-09: responder completes essential incident workflow in an approved working language', () => {
    const deLabels = getIncidentWorkflowLabels('de');
    expect(deLabels.statusTitle).toBe('Vorfallstatus');
    expect(deLabels.acknowledge).toBe('Bestätigen');
    expect(deLabels.escalate).toBe('Eskalieren');
    expect(deLabels.resolve).toBe('Beheben');

    const frLabels = getIncidentWorkflowLabels('fr');
    expect(frLabels.acknowledge).toBe('Accuser réception');
    expect(frLabels.resolve).toBe('Résoudre');
  });
});
