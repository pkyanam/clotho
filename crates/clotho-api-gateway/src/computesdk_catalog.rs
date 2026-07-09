//! ComputeSDK upstream catalog mirrored from
//! `services/compute-sdk-bridge/src/providers.mjs` and
//! https://docs.computesdk.com/providers.md / installation.md.
//!
//! Used for connect UX, secret resolution, and disconnect. Keep in sync with
//! the bridge catalog when ComputeSDK adds providers.

use serde::Serialize;

/// One upstream provider behind the ComputeSDK bridge.
#[derive(Clone, Debug, Serialize)]
pub struct ComputesdkUpstream {
    pub id: &'static str,
    pub name: &'static str,
    pub pkg: &'static str,
    /// Clotho secret / env names required to configure this upstream.
    pub required: &'static [&'static str],
    #[serde(skip_serializing_if = "slice_is_empty")]
    pub optional: &'static [&'static str],
    #[serde(skip_serializing_if = "str::is_empty")]
    pub notes: &'static str,
}

fn slice_is_empty(s: &&[&str]) -> bool {
    s.is_empty()
}

/// Full catalog of ComputeSDK providers Clotho can wire through the bridge.
pub const UPSTREAMS: &[ComputesdkUpstream] = &[
    ComputesdkUpstream {
        id: "agentcore",
        name: "AWS Bedrock AgentCore",
        pkg: "@computesdk/agentcore",
        required: &["AWS_REGION"],
        optional: &[
            "AWS_ACCESS_KEY_ID",
            "AWS_SECRET_ACCESS_KEY",
            "AWS_SESSION_TOKEN",
            "AWS_PROFILE",
            "AWS_DEFAULT_REGION",
        ],
        notes: "Uses AWS credential chain; region required.",
    },
    ComputesdkUpstream {
        id: "agentuity",
        name: "Agentuity",
        pkg: "@computesdk/agentuity",
        required: &["AGENTUITY_SDK_KEY"],
        optional: &[],
        notes: "",
    },
    ComputesdkUpstream {
        id: "archil",
        name: "Archil",
        pkg: "@computesdk/archil",
        required: &["ARCHIL_API_KEY", "ARCHIL_REGION"],
        optional: &["ARCHIL_DISK_ID"],
        notes: "create may need ARCHIL_DISK_ID.",
    },
    ComputesdkUpstream {
        id: "beam",
        name: "Beam",
        pkg: "@computesdk/beam",
        required: &["BEAM_TOKEN", "BEAM_WORKSPACE_ID"],
        optional: &[],
        notes: "",
    },
    ComputesdkUpstream {
        id: "blaxel",
        name: "Blaxel",
        pkg: "@computesdk/blaxel",
        required: &["BL_API_KEY", "BL_WORKSPACE"],
        optional: &[],
        notes: "",
    },
    ComputesdkUpstream {
        id: "cloudflare",
        name: "Cloudflare",
        pkg: "@computesdk/cloudflare",
        required: &["CLOUDFLARE_SANDBOX_URL", "CLOUDFLARE_SANDBOX_SECRET"],
        optional: &["CLOUDFLARE_API_TOKEN", "CLOUDFLARE_ACCOUNT_ID"],
        notes: "Deploy gateway worker once; runtime uses SANDBOX_URL + SECRET.",
    },
    ComputesdkUpstream {
        id: "codesandbox",
        name: "CodeSandbox",
        pkg: "@computesdk/codesandbox",
        required: &["CSB_API_KEY"],
        optional: &[],
        notes: "",
    },
    ComputesdkUpstream {
        id: "daytona",
        name: "Daytona (via ComputeSDK)",
        pkg: "@computesdk/daytona",
        required: &["DAYTONA_API_KEY"],
        optional: &[],
        notes: "Clotho also has a direct Rust Daytona CCI provider.",
    },
    ComputesdkUpstream {
        id: "declaw",
        name: "Declaw",
        pkg: "@computesdk/declaw",
        required: &["DECLAW_API_KEY"],
        optional: &[],
        notes: "",
    },
    ComputesdkUpstream {
        id: "e2b",
        name: "E2B",
        pkg: "@computesdk/e2b",
        required: &["E2B_API_KEY"],
        optional: &[],
        notes: "",
    },
    ComputesdkUpstream {
        id: "freestyle",
        name: "Freestyle",
        pkg: "@computesdk/freestyle",
        required: &["FREESTYLE_API_KEY"],
        optional: &[],
        notes: "",
    },
    ComputesdkUpstream {
        id: "hopx",
        name: "HopX",
        pkg: "@computesdk/hopx",
        required: &["HOPX_API_KEY"],
        optional: &[],
        notes: "",
    },
    ComputesdkUpstream {
        id: "k8s",
        name: "Kubernetes",
        pkg: "@computesdk/k8s",
        required: &[],
        optional: &["KUBECONFIG_B64", "KUBECONFIG", "K8S_NAMESPACE"],
        notes: "Prefer KUBECONFIG_B64 as a Clotho secret for the bridge.",
    },
    ComputesdkUpstream {
        id: "leap0",
        name: "Leap0",
        pkg: "@computesdk/leap0",
        required: &["LEAP0_API_KEY"],
        optional: &[],
        notes: "",
    },
    ComputesdkUpstream {
        id: "modal",
        name: "Modal",
        pkg: "@computesdk/modal",
        required: &["MODAL_TOKEN_ID", "MODAL_TOKEN_SECRET"],
        optional: &[],
        notes: "",
    },
    ComputesdkUpstream {
        id: "namespace",
        name: "Namespace",
        pkg: "@computesdk/namespace",
        required: &["NSC_TOKEN"],
        optional: &[],
        notes: "",
    },
    ComputesdkUpstream {
        id: "runloop",
        name: "Runloop",
        pkg: "@computesdk/runloop",
        required: &["RUNLOOP_API_KEY"],
        optional: &[],
        notes: "",
    },
    ComputesdkUpstream {
        id: "tensorlake",
        name: "Tensorlake",
        pkg: "@computesdk/tensorlake",
        required: &["TENSORLAKE_API_KEY"],
        optional: &[],
        notes: "",
    },
    ComputesdkUpstream {
        id: "upstash",
        name: "Upstash",
        pkg: "@computesdk/upstash",
        required: &["UPSTASH_BOX_API_KEY"],
        optional: &[],
        notes: "",
    },
    ComputesdkUpstream {
        id: "vercel",
        name: "Vercel",
        pkg: "@computesdk/vercel",
        required: &["VERCEL_TOKEN", "VERCEL_TEAM_ID", "VERCEL_PROJECT_ID"],
        optional: &[],
        notes: "",
    },
];

/// Every secret name that may be stored for any ComputeSDK upstream.
pub fn all_secret_names() -> Vec<&'static str> {
    let mut names = Vec::new();
    for u in UPSTREAMS {
        for n in u.required {
            if !names.contains(n) {
                names.push(*n);
            }
        }
        for n in u.optional {
            if !names.contains(n) {
                names.push(*n);
            }
        }
    }
    names.sort_unstable();
    names
}

pub fn find_upstream(id: &str) -> Option<&'static ComputesdkUpstream> {
    let id = id.to_lowercase();
    UPSTREAMS.iter().find(|u| u.id == id)
}

/// Single-key upstreams use one primary secret for connect convenience.
pub fn primary_secret_for_upstream(id: &str) -> Option<&'static str> {
    let u = find_upstream(id)?;
    if u.required.len() == 1 {
        Some(u.required[0])
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_twenty_upstreams() {
        assert_eq!(UPSTREAMS.len(), 20);
        assert!(find_upstream("e2b").is_some());
        assert!(find_upstream("vercel").is_some());
        assert!(find_upstream("modal").is_some());
        assert!(find_upstream("agentcore").is_some());
    }

    #[test]
    fn secret_names_include_common_keys() {
        let names = all_secret_names();
        assert!(names.contains(&"E2B_API_KEY"));
        assert!(names.contains(&"MODAL_TOKEN_SECRET"));
        assert!(names.contains(&"VERCEL_PROJECT_ID"));
        assert!(names.contains(&"UPSTASH_BOX_API_KEY"));
    }
}
