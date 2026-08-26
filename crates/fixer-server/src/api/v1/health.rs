use axum::Json;
use serde::Serialize;

const SCHEMA_VERSION: u8 = 1;

#[derive(Debug, Serialize)]
pub(crate) struct HealthDto {
    schema_version: u8,
    status: &'static str,
    version: &'static str,
}

pub(crate) async fn get() -> Json<HealthDto> {
    Json(HealthDto {
        schema_version: SCHEMA_VERSION,
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}
