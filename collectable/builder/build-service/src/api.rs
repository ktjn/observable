/// HTTP API routes for the build service.
use crate::{codegen, compiler, definition::PipelineDefinition, packaging};
use axum::{
    extract::Json,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Router,
};
use serde::Deserialize;
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Deserialize)]
struct BuildRequest {
    definition: PipelineDefinition,
    target: String,
}

async fn build(Json(req): Json<BuildRequest>) -> Response {
    let id = Uuid::new_v4().to_string();
    let work_dir = PathBuf::from(format!("/tmp/collectable-build-{id}"));

    let result = async {
        std::fs::create_dir_all(&work_dir)?;

        // Generate Rust source
        codegen::generate(&req.definition, &work_dir)?;

        // Compile
        let binary = compiler::compile(&work_dir, &req.target).await?;

        // Assemble package
        let zip_path = work_dir.join(format!("{}.zip", req.definition.name));
        let deploy_dir = work_dir.join("deploy");
        std::fs::create_dir_all(&deploy_dir)?;
        let pipeline_json = serde_json::to_string_pretty(&req.definition)?;

        packaging::assemble(
            &req.definition.name,
            &req.target,
            &binary,
            &work_dir.join("src"),
            &deploy_dir,
            &pipeline_json,
            &zip_path,
        )?;

        let bytes = std::fs::read(&zip_path)?;
        anyhow::Ok(bytes)
    }
    .await;

    let _ = std::fs::remove_dir_all(&work_dir);

    match result {
        Ok(bytes) => (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "application/zip"),
                (
                    header::CONTENT_DISPOSITION,
                    "attachment; filename=\"mediator.zip\"",
                ),
            ],
            bytes,
        )
            .into_response(),
        Err(e) => {
            tracing::error!("build failed: {e:#}");
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

pub fn router() -> Router {
    Router::new().route("/build", post(build))
}
