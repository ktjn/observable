/// Cross-compilation: invoke `cross build --release --target <abi>`.
use anyhow::{Context, Result};
use std::path::PathBuf;
use tokio::process::Command;

pub const SUPPORTED_TARGETS: &[&str] = &[
    "x86_64-unknown-linux-musl",
    "aarch64-unknown-linux-musl",
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
    "x86_64-pc-windows-gnu",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
];

pub async fn compile(workspace: &PathBuf, target: &str) -> Result<PathBuf> {
    if !SUPPORTED_TARGETS.contains(&target) {
        anyhow::bail!("unsupported target: {target}");
    }

    let status = Command::new("cross")
        .args(["build", "--release", "--target", target])
        .current_dir(workspace)
        .status()
        .await
        .context("failed to invoke cross")?;

    if !status.success() {
        anyhow::bail!("cross build failed for target {target}");
    }

    let binary_name = if target.contains("windows") {
        format!("{}.exe", workspace.file_name().unwrap().to_string_lossy())
    } else {
        workspace.file_name().unwrap().to_string_lossy().to_string()
    };

    Ok(workspace
        .join("target")
        .join(target)
        .join("release")
        .join(binary_name))
}
