/// Code generation: render a Rust source package from a pipeline definition.
use crate::definition::PipelineDefinition;
use anyhow::Result;
use std::path::PathBuf;
use tera::{Context, Tera};

fn templates_dir() -> String {
    std::env::var("MEDIATOR_TEMPLATES_DIR")
        .unwrap_or_else(|_| "../../mediator/templates".into())
}

fn build_context(def: &PipelineDefinition) -> Context {
    let mut ctx = Context::new();
    ctx.insert("name", &def.name);
    ctx.insert("transport_kind", &def.transport.kind);
    ctx.insert("parser_kind", &def.parser.kind);
    ctx.insert("transport_params", &def.transport.params);
    ctx.insert("parser_params", &def.parser.params);
    ctx.insert("mapping", &def.mapping);
    ctx.insert("output", &def.output);
    ctx
}

pub fn generate(def: &PipelineDefinition, out_dir: &PathBuf) -> Result<()> {
    let ctx = build_context(def);
    let glob = format!("{}/**/*", templates_dir());
    let tera = Tera::new(&glob)?;

    let main_rs = tera.render("main.rs.tmpl", &ctx)?;
    let cargo_toml = tera.render("Cargo.toml.tmpl", &ctx)?;

    let src_dir = out_dir.join("src");
    std::fs::create_dir_all(&src_dir)?;
    std::fs::write(src_dir.join("main.rs"), main_rs)?;
    std::fs::write(out_dir.join("Cargo.toml"), cargo_toml)?;

    // Write .cargo/config.toml so musl targets are always fully static,
    // regardless of how cargo is invoked.
    let cargo_config_dir = out_dir.join(".cargo");
    std::fs::create_dir_all(&cargo_config_dir)?;
    std::fs::write(
        cargo_config_dir.join("config.toml"),
        r#"[target.x86_64-unknown-linux-musl]
rustflags = ["-C", "target-feature=+crt-static", "-C", "link-arg=-static"]

[target.aarch64-unknown-linux-musl]
rustflags = ["-C", "target-feature=+crt-static", "-C", "link-arg=-static"]
"#,
    )?;

    Ok(())
}

/// Render deployment artefacts (systemd unit, init.d script, Dockerfile,
/// docker-compose snippet) into `deploy_dir`.
pub fn render_deploy(def: &PipelineDefinition, deploy_dir: &PathBuf) -> Result<()> {
    let ctx = build_context(def);
    let glob = format!("{}/**/*", templates_dir());
    let tera = Tera::new(&glob)?;

    let deploy_templates = [
        ("systemd.service.tmpl", format!("{}.service", def.name)),
        ("initd.sh.tmpl", format!("{}-initd.sh", def.name)),
        ("Dockerfile.tmpl", "Dockerfile".to_string()),
        (
            "docker-compose-snippet.yml.tmpl",
            "docker-compose-snippet.yml".to_string(),
        ),
    ];

    std::fs::create_dir_all(deploy_dir)?;
    for (tmpl_name, out_name) in &deploy_templates {
        match tera.render(tmpl_name, &ctx) {
            Ok(content) => {
                std::fs::write(deploy_dir.join(out_name), content)?;
            }
            Err(e) => {
                tracing::warn!("skipping deploy template {tmpl_name}: {e}");
            }
        }
    }

    Ok(())
}
