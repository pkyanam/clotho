//! Provider Fabric skeleton (ADR-0019 / Stage 17).
//!
//! Shared list metadata across layers: compute | storage | network | auth.
//! Compute is real (CCI registry). Storage probes the live Arachne service;
//! network remains an honest stub until Stage 19. Auth reports the active
//! AuthProvider.

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

async fn storage_providers(state: &AppState) -> Vec<FabricProvider> {
    use clotho_common::pb::storage::v1::GetStorageStatsRequest;

    let probe = state
        .storage
        .clone()
        .get_storage_stats(GetStorageStatsRequest {})
        .await;
    let (configured, configured_reason, notes) = match probe {
        Ok(response) => {
            let stats = response.into_inner();
            (
                true,
                format!(
                    "Arachne online · {} xorbs · {} stored bytes",
                    stats.xorb_count, stats.total_bytes
                ),
                "Managed default; large repo payloads are chunked and deduplicated".into(),
            )
        }
        Err(err) => (
            false,
            format!("Arachne probe failed: {}", err.message()),
            "Start clotho-storage or connect a storage provider".into(),
        ),
    };
    let mut providers = vec![FabricProvider {
        id: "minio".into(),
        name: "Arachne managed object store".into(),
        layer: ProviderLayer::Storage.as_str().into(),
        kind: "direct".into(),
        enabled: true,
        configured,
        configured_reason,
        capabilities: vec![
            "object-store".into(),
            "content-defined-chunking".into(),
            "dedup".into(),
            "git-lfs-pointer".into(),
        ],
        notes,
    }];
    let bridge_online = if state.storage_sdk_bridge_url.is_empty() {
        false
    } else {
        state
            .http
            .get(format!("{}/health", state.storage_sdk_bridge_url))
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await
            .map(|response| response.status().is_success())
            .unwrap_or(false)
    };
    providers.push(FabricProvider {
        id: "storagesdk".into(),
        name: "StorageSDK provider bridge".into(),
        layer: ProviderLayer::Storage.as_str().into(),
        kind: "bridge".into(),
        enabled: true,
        // Bridge availability alone never claims customer storage works.
        configured: false,
        configured_reason: if bridge_online {
            "Bridge online · connect and probe an external provider".into()
        } else {
            "Optional bridge offline · managed Arachne remains available".into()
        },
        capabilities: vec![
            "s3".into(),
            "minio".into(),
            "r2".into(),
            "snapshots".into(),
            "forks".into(),
        ],
        notes: "StorageSDK keeps external provider operations modular; no credentials are returned"
            .into(),
    });
    providers
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
        providers.extend(storage_providers(&state).await);
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
            providers: storage_providers(state).await,
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

    let mut non_compute = storage_providers(&state).await;
    non_compute.extend(network_stubs());
    non_compute.extend(auth_providers(&state));
    for p in non_compute {
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
