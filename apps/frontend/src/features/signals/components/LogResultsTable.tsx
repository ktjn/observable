import type { LogRecord } from "../../../api/logs";
import { VirtualTable } from "../../../components/ui/VirtualTable";
import { formatTimestamp } from "../../../utils/formatTimestamp";
import { formatLogMessage, getSeverityColor, otelSeverity } from "../../../utils/logFormatting";
import type { TimeFormat } from "../../../lib/timeDisplay";

export function LogResultsTable({
  logs,
  selectedLogId,
  onSelectLog,
  timeFormat,
  showServiceColumn = true,
  ariaLabel = showServiceColumn ? "Log results" : "Service logs",
}: {
  logs: LogRecord[];
  selectedLogId: string | undefined;
  onSelectLog: (logId: string) => void;
  timeFormat: TimeFormat;
  showServiceColumn?: boolean;
  ariaLabel?: string;
}) {
  return (
    <VirtualTable
      rows={logs}
      ariaLabel={ariaLabel}
      renderHead={() => (
        <tr>
          <th aria-label="Time">Time</th>
          <th>Level</th>
          {showServiceColumn && <th>Service</th>}
          <th>Message</th>
        </tr>
      )}
      renderRow={(log, ref, index) => (
        <LogResultsRow
          key={log.log_id}
          log={log}
          timeFormat={timeFormat}
          selected={selectedLogId === log.log_id}
          onSelect={() => onSelectLog(log.log_id)}
          showServiceColumn={showServiceColumn}
          measureRef={ref}
          index={index}
        />
      )}
    />
  );
}

function LogResultsRow({
  log,
  timeFormat,
  selected,
  onSelect,
  showServiceColumn,
  measureRef,
  index,
}: {
  log: LogRecord;
  timeFormat: TimeFormat;
  selected: boolean;
  onSelect: () => void;
  showServiceColumn: boolean;
  measureRef: (el: Element | null) => void;
  index: number;
}) {
  const severity = otelSeverity(log.severity_number);
  const message = formatLogMessage(log.body);

  const accentClass =
    severity.tone === "bad"
      ? "border-l-2 border-l-[var(--bad)]"
      : severity.tone === "warn"
        ? "border-l-2 border-l-[var(--warn)]"
        : "";

  return (
    <tr
      ref={measureRef}
      data-index={index}
      className={`modern-table-row cursor-pointer ${accentClass} ${selected ? "bg-[var(--surface-subtle)]" : ""}`}
      onClick={onSelect}
      onKeyDown={(e) => (e.key === "Enter" || e.key === " ") && onSelect()}
      tabIndex={0}
      role="button"
      aria-label={`Open log context for ${message}`}
      aria-pressed={selected}
    >
      <td className="whitespace-nowrap">{formatTimestamp(log.timestamp_unix_nano, timeFormat)}</td>
      <td>
        <span
          className="text-[9px] font-bold uppercase tracking-wide"
          style={{ color: getSeverityColor(log.severity_number) }}
        >
          {severity.label}
        </span>
      </td>
      {showServiceColumn && <td>{log.service_name}</td>}
      <td className="whitespace-normal break-all">{message}</td>
    </tr>
  );
}
