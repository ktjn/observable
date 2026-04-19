/// Code generation: render a Rust source package from a pipeline definition.
use crate::definition::PipelineDefinition;
use anyhow::Result;
use std::path::PathBuf;
use tera::{Context, Tera};

fn templates_dir() -> String {
    std::env::var("MEDIATOR_TEMPLATES_DIR")
        .unwrap_or_else(|_| "../../mediator/templates".into())
}

pub fn generate(def: &PipelineDefinition, out_dir: &PathBuf) -> Result<()> {
    let mut ctx = Context::new();
    ctx.insert("name", &def.name);
    ctx.insert("transport_kind", &def.transport.kind);
    ctx.insert("parser_kind", &def.parser.kind);
    ctx.insert("transport_params", &def.transport.params);
    ctx.insert("parser_params", &def.parser.params);
    ctx.insert("mapping", &def.mapping);
    ctx.insert("output", &def.output);

    let glob = format!("{}/**/*", templates_dir());
    let tera = Tera::new(&glob)?;
    let main_rs = tera.render("main.rs.tmpl", &ctx)?;
    let cargo_toml = tera.render("Cargo.toml.tmpl", &ctx)?;

    let src_dir = out_dir.join("src");
    std::fs::create_dir_all(&src_dir)?;
    std::fs::write(src_dir.join("main.rs"), main_rs)?;
    std::fs::write(out_dir.join("Cargo.toml"), cargo_toml)?;

    Ok(())
}
