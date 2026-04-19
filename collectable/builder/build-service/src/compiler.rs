/// Cross-compilation: invoke `cargo build --release --target <abi>`.
/// The Rust toolchain with the required targets is pre-installed in the
/// build-service Docker image. No Docker-in-Docker required.
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

pub async fn compile(workspace: &PathBuf, target: &str, name: &str) -> Result<PathBuf> {
    if !SUPPORTED_TARGETS.contains(&target) {
        anyhow::bail!("unsupported target: {target}");
    }

    // Force static libc embedding for musl targets so the binary has no
    // dynamic interpreter dependency on ld-musl-*.so.
    let mut cmd = Command::new("cargo");
    cmd.args(["build", "--release", "--target", target])
        .current_dir(workspace);
    if target.contains("musl") {
        // musl.cc cross-compiler defaults to static linking; +crt-static
        // statically embeds musl libc. No extra link flags needed.
        cmd.env("RUSTFLAGS", "-C target-feature=+crt-static");
    }
    let status = cmd
        .status()
        .await
        .context("failed to invoke cargo")?;

    if !status.success() {
        anyhow::bail!("cargo build failed for target {target}");
    }

    let binary_name = if target.contains("windows") {
        format!("{name}.exe")
    } else {
        name.to_string()
    };

    Ok(workspace
        .join("target")
        .join(target)
        .join("release")
        .join(binary_name))
}
