/// Assemble the download package ZIP.
use anyhow::Result;
use std::fs::File;
use std::path::PathBuf;
use zip::write::SimpleFileOptions;
use zip::DateTime;
use zip::ZipWriter;

/// Convert the current **local** wall-clock time to a [`zip::DateTime`].
///
/// ZIP's MS-DOS timestamp has no timezone field; tools like `unzip` treat it
/// as local time. We use `libc::localtime_r` to obtain the local date/time
/// components so the timestamp matches what the user sees in their shell.
/// Falls back to the ZIP epoch (1980-01-01) on any failure.
fn zip_now() -> DateTime {
    use std::time::{SystemTime, UNIX_EPOCH};

    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as libc::time_t;

    let mut tm = unsafe { std::mem::zeroed::<libc::tm>() };
    let ok = unsafe { !libc::localtime_r(&secs, &mut tm).is_null() };
    if !ok {
        return DateTime::default();
    }

    let year = (1900 + tm.tm_year).clamp(1980, 2107) as u16;
    let month = (tm.tm_mon + 1) as u8;
    let day = tm.tm_mday as u8;
    let hour = tm.tm_hour as u8;
    let minute = tm.tm_min as u8;
    let second = tm.tm_sec.clamp(0, 60) as u8; // 60 = leap second

    DateTime::from_date_and_time(year, month, day, hour, minute, second).unwrap_or_default()
}

pub fn assemble(
    name: &str,
    target: &str,
    binary: &PathBuf,
    src_dir: &PathBuf,
    deploy_dir: &PathBuf,
    pipeline_json: &str,
    out_path: &PathBuf,
) -> Result<()> {
    let zip_file = File::create(out_path)?;
    let mut zip = ZipWriter::new(zip_file);
    let now = zip_now();
    let opts = SimpleFileOptions::default().last_modified_time(now);
    let exec_opts = SimpleFileOptions::default()
        .last_modified_time(now)
        .unix_permissions(0o100755);
    let prefix = format!("{name}-{target}");

    // Binary — set executable bit so it runs without a manual chmod
    zip.start_file(format!("{prefix}/{name}"), exec_opts)?;
    std::io::copy(&mut File::open(binary)?, &mut zip)?;

    // Source
    for entry in std::fs::read_dir(src_dir)? {
        let entry = entry?;
        let rel = entry.file_name();
        zip.start_file(format!("{prefix}/src/{}", rel.to_string_lossy()), opts)?;
        std::io::copy(&mut File::open(entry.path())?, &mut zip)?;
    }

    // Deploy artifacts
    for entry in std::fs::read_dir(deploy_dir)? {
        let entry = entry?;
        let rel = entry.file_name();
        zip.start_file(format!("{prefix}/deploy/{}", rel.to_string_lossy()), opts)?;
        std::io::copy(&mut File::open(entry.path())?, &mut zip)?;
    }

    // Pipeline definition
    zip.start_file(format!("{prefix}/pipeline.json"), opts)?;
    std::io::Write::write_all(&mut zip, pipeline_json.as_bytes())?;

    zip.finish()?;
    Ok(())
}
