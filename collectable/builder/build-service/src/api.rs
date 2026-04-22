/// HTTP API routes for the build service.
use crate::{codegen, compiler, definition::PipelineDefinition, packaging, parse, validate};
use axum::{
    extract::Json,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Router,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf};
use uuid::Uuid;

#[derive(Deserialize)]
struct BuildRequest {
    definition: PipelineDefinition,
    target: String,
}

// ── /api/parse ────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ParseRequest {
    /// Parser config (same shape as PipelineDefinition.parser).
    parser: ParseParams,
    /// Raw sample lines sent from the UI (max ~50).
    lines: Vec<String>,
}

#[derive(Deserialize)]
struct ParseParams {
    #[serde(rename = "type")]
    kind: String,
    #[serde(flatten)]
    params: HashMap<String, serde_json::Value>,
}

#[derive(Serialize)]
struct ParseResponse {
    rows: Vec<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

async fn parse_preview(Json(req): Json<ParseRequest>) -> Json<ParseResponse> {
    let capped: Vec<String> = req.lines.into_iter().take(20).collect();
    match parse::parse_lines(&req.parser.kind, &req.parser.params, &capped) {
        Ok(rows) => Json(ParseResponse { rows, error: None }),
        Err(e) => Json(ParseResponse {
            rows: vec![],
            error: Some(e),
        }),
    }
}

async fn build(Json(req): Json<BuildRequest>) -> Response {
    if let Err(e) = validate::validate(&req.definition) {
        return (StatusCode::BAD_REQUEST, e.to_string()).into_response();
    }

    let id = Uuid::new_v4().to_string();
    let work_dir = PathBuf::from(format!("/tmp/collectable-build-{id}"));

    let result = async {
        std::fs::create_dir_all(&work_dir)?;

        // Assemble deploy dir path early (needed before compile for render_deploy)
        let deploy_dir = work_dir.join("deploy");
        let zip_path = work_dir.join(format!("{}.zip", req.definition.name));

        // Generate Rust source
        codegen::generate(&req.definition, &work_dir)?;

        // Render deployment artefacts
        codegen::render_deploy(&req.definition, &deploy_dir)?;

        // Compile
        let binary = compiler::compile(&work_dir, &req.target, &req.definition.name).await?;

        // Assemble package
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
    Router::new()
        .route("/build", post(build))
        .route("/api/parse", post(parse_preview))
}
