import { Badge } from "../../components/ui/badge";
import { Panel } from "../../components/ui/panel";

export interface TimelineDeploymentEntry {
  eventTime: string;
  deploymentId: string;
  version: string;
  actor: string;
}

export interface ShiftHandoverRecord {
  fromResponder: string;
  toResponder: string;
  handedOverAt: string;
  evidenceRefs: string[];
  decisions: string[];
  openQuestions: string[];
}

export interface IncidentLifecycleStage {
  stage: "detected" | "acknowledged" | "mitigated" | "resolved";
  timestamp: string;
  actor: string;
  summary: string;
}

export interface CriticalStatusBasis {
  statusLabel: string;
  ruleName: string;
  thresholdBasis: string;
  observedValue: string;
  evidenceRefs: string[];
}

export interface SharedResponderObservation {
  id: string;
  summary: string;
  selectedBy: string;
}

export interface IncidentInvestigationP1Props {
  deployments: TimelineDeploymentEntry[];
  handover?: ShiftHandoverRecord;
  lifecycleStages: IncidentLifecycleStage[];
  criticalBasis?: CriticalStatusBasis;
  selectedObservations: SharedResponderObservation[];
  openQuestions: string[];
  viewerRole?: string;
}

export function IncidentInvestigationP1Panel({
  deployments,
  handover,
  lifecycleStages,
  criticalBasis,
  selectedObservations,
  openQuestions,
  viewerRole = "second-responder",
}: IncidentInvestigationP1Props) {
  return (
    <div className="space-y-4">
      {criticalBasis && (
        <Panel title="Critical status basis">
          <div className="space-y-2 text-xs">
            <div className="text-[var(--muted)]">Explicit evidence and trigger basis</div>
            <div className="flex items-center gap-2">
              <Badge tone="bad">{criticalBasis.statusLabel}</Badge>
              <span className="font-medium">{criticalBasis.ruleName}</span>
            </div>
            <div>Threshold: {criticalBasis.thresholdBasis}</div>
            <div>Observed: {criticalBasis.observedValue}</div>
            <ul className="list-disc pl-4">
              {criticalBasis.evidenceRefs.map((ref) => (
                <li key={ref}>{ref}</li>
              ))}
            </ul>
          </div>
        </Panel>
      )}

      {deployments.length > 0 && (
        <Panel title="Related deployments in timeline">
          <ul className="space-y-1 text-xs">
            {deployments.map((dep) => (
              <li key={dep.deploymentId} className="flex items-center gap-2">
                <Badge tone="warn">deployment</Badge>
                <span>{dep.version}</span>
                <span>({dep.deploymentId})</span>
                <span>by {dep.actor}</span>
                <span>at {dep.eventTime}</span>
              </li>
            ))}
          </ul>
        </Panel>
      )}

      {lifecycleStages.length > 0 && (
        <Panel title="Detection to resolution">
          <ol className="space-y-1 text-xs">
            {lifecycleStages.map((s) => (
              <li key={`${s.stage}-${s.timestamp}`} className="flex items-center gap-2">
                <Badge tone={s.stage === "resolved" ? "good" : "neutral"}>{s.stage}</Badge>
                <span>{s.timestamp}</span>
                <span>{s.actor}</span>
                <span>{s.summary}</span>
              </li>
            ))}
          </ol>
        </Panel>
      )}

      {handover && (
        <Panel title="Shift handover">
          <div className="space-y-2 text-xs">
            <div className="text-[var(--muted)]">
              {handover.fromResponder} → {handover.toResponder} ({handover.handedOverAt})
            </div>
            <div>
              <div className="font-medium">Documented evidence</div>
              <ul className="list-disc pl-4">
                {handover.evidenceRefs.map((e) => (
                  <li key={e}>{e}</li>
                ))}
              </ul>
            </div>
            <div>
              <div className="font-medium">Decisions</div>
              <ul className="list-disc pl-4">
                {handover.decisions.map((d) => (
                  <li key={d}>{d}</li>
                ))}
              </ul>
            </div>
            <div>
              <div className="font-medium">Handover open questions</div>
              <ul className="list-disc pl-4">
                {handover.openQuestions.map((q) => (
                  <li key={q}>{q}</li>
                ))}
              </ul>
            </div>
          </div>
        </Panel>
      )}

      <Panel title="Shared responder observations">
        <div className="space-y-2 text-xs">
          <div className="text-[var(--muted)]">{`Opened as ${viewerRole}`}</div>
          <div>
            <div className="font-medium">Selected observations</div>
            <ul className="list-disc pl-4">
              {selectedObservations.map((obs) => (
                <li key={obs.id}>
                  {obs.summary} (selected by {obs.selectedBy})
                </li>
              ))}
            </ul>
          </div>
          <div>
            <div className="font-medium">Open questions</div>
            <ul className="list-disc pl-4">
              {openQuestions.map((q) => (
                <li key={q}>{q}</li>
              ))}
            </ul>
          </div>
        </div>
      </Panel>
    </div>
  );
}
