//! The pairing endpoint, the only route mounted without the auth middleware.
//! The Mac issues a code (`companion_start_pairing`); the phone posts it to
//! `POST /v1/pair/complete` after scanning the Mac's QR.

use crate::companion::dto::{PairCompleteRequest, PairCompleteResponse};
use crate::companion::errors::{CompanionError, CompanionResult};
use crate::companion::pairing::{PairingEndpointHint, PairingService};
use axum::extract::State;
use axum::Json;
use std::sync::Arc;

/// Shared state carried into the pairing routes.
pub struct PairingHttpState {
    pub service: Arc<PairingService>,
    pub endpoint_hint: PairingEndpointHint,
}

pub async fn complete(
    State(state): State<Arc<PairingHttpState>>,
    body: Result<Json<PairCompleteRequest>, axum::extract::rejection::JsonRejection>,
) -> CompanionResult<Json<PairCompleteResponse>> {
    let Json(request) = body.map_err(|err| CompanionError::BadRequest(err.to_string()))?;
    let now_ms = chrono::Utc::now().timestamp_millis();
    let resp = state
        .service
        .complete(now_ms, &state.endpoint_hint.mac_name, request)?;
    Ok(Json(resp))
}
