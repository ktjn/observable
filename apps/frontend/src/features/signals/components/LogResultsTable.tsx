import type { LogRecord } from "../../../api/logs";
import { VirtualTable } from "../../../components/ui/VirtualTable";
import { CopyButton } from "../../../components/ui/copy-button";
import { formatLogMessage, otelSeverity, severityTextClass } from "../../../utils/logFormatting";
import { DEFAULT_LOG_COLUMNS, getLogFieldValue } from "../../../utils/logContext";
import type { TimeFormat } from "../../../lib/timeDisplay";

export function LogResultsTable({
  logs,
  selectedLogId,
  onSelectLog,
  timeFormat,
  visibleColumns = DEFAULT_LOG_COLUMNS,
  ariaLabel = "Log results",
}: {
  logs: LogRecord[];
  selectedLogId: string | undefined;
  onSelectLog: (logId: string) => void;
  timeFormat: TimeFormat;
  visibleColumns?: readonly string[];
  ariaLabel?: string;
}) {
  return (
    <VirtualTable
      rows={logs}
      ariaLabel={ariaLabel}
      renderHead={() => (
        <tr>
          {visibleColumns.length === 0 ? (
            <th aria-label="No columns selected">No columns selected</th>
          ) : (
            visibleColumns.map((key) => <th key={key}>{key}</th>)
          )}
        </tr>
      )}
      renderRow={(log, ref, index) => (
        <LogResultsRow
          key={log.log_id}
          log={log}
          timeFormat={timeFormat}
          selected={selectedLogId === log.log_id}
          onSelect={() => onSelectLog(log.log_id)}
          columns={visibleColumns}
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
  columns,
  measureRef,
  index,
}: {
  log: LogRecord;
  timeFormat: TimeFormat;
  selected: boolean;
  onSelect: () => void;
  columns: readonly string[];
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
      className={`modern-table-row group cursor-pointer ${accentClass} ${selected ? "bg-[var(--surface-subtle)]" : ""}`}
      onClick={onSelect}
      onKeyDown={(e) => (e.key === "Enter" || e.key === " ") && onSelect()}
      tabIndex={0}
      role="row"
      aria-label={`Open log context for ${message}`}
      aria-selected={selected}
    >
      {columns.length === 0 ? (
        <td aria-label="No columns selected">No columns selected</td>
      ) : (
        columns.map((key) => {
          const value = getLogFieldValue(log, key, timeFormat);
          return (
            <td key={key} className="whitespace-normal break-all">
              {key === "severity_number" ? (
                <span className={`text-[9px] font-bold uppercase tracking-wide ${severityTextClass(Number(value))}`}>
                  {otelSeverity(Number(value)).label}
                </span>
              ) : key === "message" ? (
                <span className="inline-flex min-w-0 max-w-full items-start gap-1">
                  <span className="min-w-0 break-all">{value}</span>
                  <CopyButton value={value} label="Copy message" />
                </span>
              ) : (
                value
              )}
            </td>
          );
        })
      )}
    </tr>
  );
}
