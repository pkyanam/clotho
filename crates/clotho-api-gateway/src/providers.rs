//! Provider Fabric skeleton (ADR-0019 / Stage 17).
//!
//! Shared list metadata across layers: compute | storage | network | auth.
//! Compute is real (CCI registry). Storage/network are honest stubs until
//! Stages 18–19. Auth reports the active AuthProvider.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::actions::{self, ComputeProviderJson};
use crate::error::ApiError;
use crate::AppState;

/// Fabric layer ids (ADR-0019).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderLayer {
    Compute,
    Storage,
    Network,
    Auth,
}

impl ProviderLayer {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Compute => "compute",
            Self::Storage => "storage",
            Self::Network => "network",
            Self::Auth => "auth",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "compute" => Some(Self::Compute),
            "storage" => Some(Self::Storage),
            "network" => Some(Self::Network),
            "auth" => Some(Self::Auth),
            _ => None,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ListProvidersQuery {
    /// Optional fabric layer filter. When omitted, returns compute providers
    /// (backward compatible with Stage 12) plus fabric stubs when `all=true`.
    pub layer: Option<String>,
    /// When true (and layer omitted), include all fabric layers.
    #[serde(default)]
    pub all: bool,
}

/// Shared fabric provider metadata (non-secret).
#[derive(Clone, Debug, Serialize)]
pub struct FabricProvider {
    pub id: String,
    pub name: String,
    pub layer: String,
    /// Implementation kind: direct | bridge | stub | auth.
    pub kind: String,
    pub enabled: bool,
    pub configured: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub configured_reason: String,
    pub capabilities: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub notes: String,
}

#[derive(Serialize)]
pub struct FabricProviderListResponse {
    pub providers: Vec<FabricProvider>,
    pub default_provider_id: String,
    /// Echo of the layer filter when set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layer: Option<String>,
}

impl From<ComputeProviderJson> for FabricProvider {
    fn from(p: ComputeProviderJson) -> Self {
        Self {
            id: p.id,
            name: p.name,
            layer: ProviderLayer::Compute.as_str().into(),
            kind: p.kind,
            enabled: p.enabled,
            configured: p.configured,
            configured_reason: p.configured_reason,
            capabilities: p.capabilities,
            notes: p.notes,
        }
    }
}

fn storage_stubs() -> Vec<FabricProvider> {
    vec![FabricProvider {
        id: "minio".into(),
        name: "Clotho MinIO (managed default)".into(),
        layer: ProviderLayer::Storage.as_str().into(),
        kind: "stub".into(),
        enabled: true,
        configured: false,
        configured_reason: "BYO object store connect ships in Stage 18 (ADR-0020)".into(),
        capabilities: vec!["object-store".into()],
        notes: "Not connected — ObjectStoreProvider skeleton only".into(),
    }]
}

fn network_stubs() -> Vec<FabricProvider> {
    vec![
        FabricProvider {
            id: "public".into(),
            name: "Public egress".into(),
            layer: ProviderLayer::Network.as_str().into(),
            kind: "stub".into(),
            enabled: true,
            configured: true,
            configured_reason: "Default network path (no mesh)".into(),
            capabilities: vec![],
            notes: "Default NetworkProvider until Tailscale (Stage 19)".into(),
        },
        FabricProvider {
            id: "tailscale".into(),
            name: "Tailscale".into(),
            layer: ProviderLayer::Network.as_str().into(),
            kind: "stub".into(),
            enabled: true,
            configured: false,
            configured_reason: "not connected — Tailscale NetworkProvider ships in Stage 19".into(),
            capabilities: vec!["private-net".into()],
            notes: "Connect/disconnect not implemented in Stage 17".into(),
        },
    ]
}

fn auth_providers(state: &AppState) -> Vec<FabricProvider> {
    let id = state.auth_provider.id();
    vec![
        FabricProvider {
            id: "bootstrap".into(),
            name: "Bootstrap (local/dev)".into(),
            layer: ProviderLayer::Auth.as_str().into(),
            kind: "auth".into(),
            enabled: true,
            configured: id == crate::auth_provider::AuthProviderId::Bootstrap,
            configured_reason: if id == crate::auth_provider::AuthProviderId::Bootstrap {
                "Active AuthProvider".into()
            } else {
                "Inactive — CLOTHO_AUTH_PROVIDER is not bootstrap".into()
            },
            capabilities: vec!["human-api-tokens".into()],
            notes: "ADR-0015 clotho_tok_… + optional open local auth".into(),
        },
        FabricProvider {
            id: "clerk".into(),
            name: "Clerk".into(),
            layer: ProviderLayer::Auth.as_str().into(),
            kind: "auth".into(),
            enabled: true,
            configured: id == crate::auth_provider::AuthProviderId::Clerk,
            configured_reason: if id == crate::auth_provider::AuthProviderId::Clerk {
                "Active AuthProvider for managed humans/orgs".into()
            } else {
                "not connected — set CLOTHO_AUTH_PROVIDER=clerk for managed deploy".into()
            },
            capabilities: vec!["sso".into(), "orgs".into(), "human-sessions".into()],
            notes: "Humans/orgs only; agents stay on Clotho agent tokens (ADR-0005)".into(),
        },
    ]
}

/// `GET /api/v1/providers` — compute by default; `?layer=` for fabric filter;
/// `?all=true` returns every layer.
pub async fn list_fabric_providers(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListProvidersQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if let Some(raw) = query.layer.as_deref() {
        let layer = ProviderLayer::parse(raw).ok_or_else(|| {
            ApiError::InvalidRequest(format!(
                "unknown layer {raw:?}; expected compute|storage|network|auth"
            ))
        })?;
        let list = fabric_for_layer(&state, layer).await;
        return Ok(Json(serde_json::to_value(list).unwrap()));
    }

    if query.all {
        let compute = actions::list_providers_for(&state).await;
        let default_provider_id = compute.default_provider_id.clone();
        let mut providers = Vec::new();
        providers.extend(compute.providers.into_iter().map(FabricProvider::from));
        providers.extend(storage_stubs());
        providers.extend(network_stubs());
        providers.extend(auth_providers(&state));
        let list = FabricProviderListResponse {
            providers,
            default_provider_id,
            layer: None,
        };
        return Ok(Json(serde_json::to_value(list).unwrap()));
    }

    // Backward compatible: bare GET returns compute registry shape.
    let list = actions::list_providers_for(&state).await;
    Ok(Json(serde_json::to_value(list).unwrap()))
}

async fn fabric_for_layer(state: &AppState, layer: ProviderLayer) -> FabricProviderListResponse {
    match layer {
        ProviderLayer::Compute => {
            let list = actions::list_providers_for(state).await;
            FabricProviderListResponse {
                default_provider_id: list.default_provider_id.clone(),
                providers: list
                    .providers
                    .into_iter()
                    .map(FabricProvider::from)
                    .collect(),
                layer: Some(layer.as_str().into()),
            }
        }
        ProviderLayer::Storage => FabricProviderListResponse {
            providers: storage_stubs(),
            default_provider_id: "minio".into(),
            layer: Some(layer.as_str().into()),
        },
        ProviderLayer::Network => FabricProviderListResponse {
            providers: network_stubs(),
            default_provider_id: "public".into(),
            layer: Some(layer.as_str().into()),
        },
        ProviderLayer::Auth => {
            let providers = auth_providers(state);
            let default = state.auth_provider.id().as_str().to_string();
            FabricProviderListResponse {
                providers,
                default_provider_id: default,
                layer: Some(layer.as_str().into()),
            }
        }
    }
}

pub async fn get_fabric_provider(
    State(state): State<Arc<AppState>>,
    Path(provider): Path<String>,
    Query(query): Query<ListProvidersQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if let Some(raw) = query.layer.as_deref() {
        let layer = ProviderLayer::parse(raw).ok_or_else(|| {
            ApiError::InvalidRequest(format!(
                "unknown layer {raw:?}; expected compute|storage|network|auth"
            ))
        })?;
        let list = fabric_for_layer(&state, layer).await;
        let found = list
            .providers
            .into_iter()
            .find(|p| p.id.eq_ignore_ascii_case(&provider))
            .ok_or_else(|| ApiError::NotFound(format!("provider {provider:?} not found")))?;
        return Ok(Json(serde_json::to_value(found).unwrap()));
    }

    for p in storage_stubs()
        .into_iter()
        .chain(network_stubs())
        .chain(auth_providers(&state))
    {
        if p.id.eq_ignore_ascii_case(&provider) {
            return Ok(Json(serde_json::to_value(p).unwrap()));
        }
    }

    let list = actions::list_providers_for(&state).await;
    let found = list
        .providers
        .into_iter()
        .find(|p| p.id.eq_ignore_ascii_case(&provider))
        .ok_or_else(|| ApiError::NotFound(format!("provider {provider:?} not found")))?;
    Ok(Json(serde_json::to_value(found).unwrap()))
}
