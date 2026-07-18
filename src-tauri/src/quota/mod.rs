//! Vendor quota/limit layer (T2.5, design.md §F9).
//!
//! Each vendor's quota is fetched via its own adapter and normalized into a
//! uniform [`Quota`] VM. Three credential-binding categories (see
//! memory vendor-quota-binding):
//!   ① subscription (read local CLI creds)   — claude, codex, grok
//!   ② API key                                 — deepseek, glm, minimax, kimi, volcengine, copilot
//!   ③ cookie / id                            — qoder, ollama, glm-team
//!
//! V1 ships the DeepSeek adapter (simplest balance-type with a clear API) as the
//! reference; other adapters land incrementally. Dispatch is by [`VendorId`]
//! (no trait objects, no async-trait dep).

pub mod deepseek;
pub mod types;

pub use types::{Quota, QuotaKind, QuotaStatus};

/// Credential binding category (design.md §F9, settings→账号 UI).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialCategory {
    /// Subscription; read local CLI auth files (claude/codex/grok).
    Subscription,
    /// API key in settings (deepseek/glm/minimax/kimi/volcengine/copilot).
    ApiKey,
    /// Cookie / org+project id (qoder/ollama/glm-team).
    Cookie,
}

/// Supported vendors. Add a variant + adapter as each lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VendorId {
    Deepseek,
    // V1 stubs — adapters land incrementally:
    // Claude, Codex, Grok,          // subscription
    // Glm, Minimax, Kimi, Volcengine, Copilot,  // api key
    // Qoder, Ollama, GlmTeam,       // cookie
}

impl VendorId {
    pub fn label(self) -> &'static str {
        match self {
            VendorId::Deepseek => "DeepSeek",
        }
    }

    pub fn category(self) -> CredentialCategory {
        match self {
            VendorId::Deepseek => CredentialCategory::ApiKey,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum VendorError {
    #[error("network: {0}")]
    Network(String),
    #[error("vendor api error: status={status} body={body}")]
    Api { status: u16, body: String },
    #[error("parse: {0}")]
    Parse(String),
    #[error("vendor returned no usable quota payload")]
    Empty,
}

/// Fetch a vendor's quota. `credential` comes from the keyring (caller's job).
pub async fn fetch(vendor: VendorId, credential: &str) -> Result<Quota, VendorError> {
    match vendor {
        VendorId::Deepseek => deepseek::fetch(credential).await,
    }
}
