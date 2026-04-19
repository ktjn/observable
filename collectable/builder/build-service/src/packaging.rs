/// Assemble the download package ZIP.
use anyhow::Result;
use std::fs::File;
use std::path::PathBuf;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

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
    let opts = SimpleFileOptions::default();
    let prefix = format!("{name}-{target}");

    // Binary
    zip.start_file(format!("{prefix}/{name}"), opts)?;
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
