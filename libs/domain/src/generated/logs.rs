// Generated artifacts for the `logs` domain (models/logs.mdl).
// Regenerate with:
//   modelable compile models --target rust --out <tmp>
// then copy logs.LogRecord.v1.rs / logs.LogRow.v1.rs from <tmp>/logs/ into this
// directory, renaming to snake_case file names. Do not hand-edit the generated
// files themselves.
#![allow(dead_code, unused_imports, clippy::useless_conversion)]

mod logs_log_record_v1;
#[cfg(feature = "storage")]
mod logs_log_row_v1;

#[cfg(feature = "storage")]
pub(crate) use logs_log_row_v1::LogsLogRowV1;
