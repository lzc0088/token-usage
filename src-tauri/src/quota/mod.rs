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

pub mod claude;
pub mod codex;
pub mod copilot;
pub mod cursor;
pub mod deepseek;
pub mod glm;
pub mod glm_team;
pub mod iflytek;
pub mod kimi;
pub mod mimo;
pub mod minimax;
pub mod ollama;
pub mod opencode;
pub mod qoder;
pub mod scheduler;
pub mod stepfun;
pub mod types;
pub mod volcengine;

pub use types::{Quota, QuotaBalance, QuotaStatus, QuotaWindow};

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
    Glm,
    Minimax,
    Kimi,
    Volcengine,
    Mimo,
    Stepfun,
    Iflytek,
    GlmTeam,
    Qoder,
    Cursor,
    Copilot,
    Ollama,
    Opencode,
    Claude,
    Codex,
}

impl VendorId {
    pub fn label(self) -> &'static str {
        match self {
            VendorId::Deepseek => "DeepSeek",
            VendorId::Glm => "GLM",
            VendorId::Minimax => "MiniMax",
            VendorId::Kimi => "Kimi",
            VendorId::Volcengine => "Volcengine",
            VendorId::Mimo => "MiMo",
            VendorId::Stepfun => "StepFun",
            VendorId::Iflytek => "iFlytek",
            VendorId::GlmTeam => "GLM Team",
            VendorId::Qoder => "Qoder ( 阿里 )",
            VendorId::Cursor => "Cursor ( Anysphere )",
            VendorId::Copilot => "GitHub Copilot",
            VendorId::Ollama => "Ollama ( Ollama Cloud )",
            VendorId::Opencode => "OpenCode ( OpenCode AI )",
            VendorId::Claude => "Claude Code",
            VendorId::Codex => "Codex",
        }
    }

    pub fn category(self) -> CredentialCategory {
        match self {
            VendorId::Deepseek
            | VendorId::Glm
            | VendorId::Minimax
            | VendorId::Kimi
            | VendorId::Volcengine
            | VendorId::Mimo
            | VendorId::GlmTeam => CredentialCategory::ApiKey,
            // stepfun / iflytek / qoder / cursor / copilot / ollama / opencode / claude / codex use cookie / OAuth token.
            VendorId::Stepfun | VendorId::Iflytek | VendorId::Qoder | VendorId::Cursor | VendorId::Copilot | VendorId::Ollama | VendorId::Opencode | VendorId::Claude | VendorId::Codex => {
                CredentialCategory::Cookie
            }
        }
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum VendorError {
    #[error("network: {0}")]
    Network(String),
    #[error("vendor api error: status={status} body={body}")]
    Api { status: u16, body: String },
    #[error("parse: {0}")]
    Parse(String),
    #[error("vendor returned no usable quota payload")]
    Empty,
    /// Authentication / session failure (expired cookie, invalid token, etc.).
    /// Adapters should use this for any clearly auth-related failure so the
    /// scheduler can surface a cookie/credential error to the frontend.
    #[error("authentication failed: {0}")]
    Auth(String),
}

/// Reject control chars (CRLF → HTTP header injection) and enforce sane length
/// before a credential is placed in a request header. Defense-in-depth: the
/// credential comes from keyring, but a corrupt/hostile value must not smuggle
/// extra headers.
pub fn validate_header_safe(s: &str) -> Result<(), VendorError> {
    if s.is_empty() {
        return Err(VendorError::Parse("empty credential".into()));
    }
    if s.len() > 4096 {
        return Err(VendorError::Parse("credential too long".into()));
    }
    if s.chars().any(|c| c.is_control()) {
        return Err(VendorError::Parse(
            "credential contains control characters".into(),
        ));
    }
    Ok(())
}

/// Extract the API key from a credential string.
///
/// The frontend sends credentials as JSON (`{"key":"sk-..."}`), but some
/// adapters expect a plain key string. This helper unwraps the `key` field
/// when the input is JSON, otherwise returns the string as-is.
pub fn extract_key(credential: &str) -> String {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(credential) {
        if let Some(key) = v.get("key").and_then(|k| k.as_str()) {
            return key.trim().to_string();
        }
    }
    credential.trim().to_string()
}

/// Fetch a vendor's quota. `credential` comes from the keyring (caller's job).
pub async fn fetch(vendor: VendorId, credential: &str) -> Result<Quota, VendorError> {
    match vendor {
        VendorId::Deepseek => deepseek::fetch(credential).await,
        VendorId::Glm => glm::fetch(credential).await,
        VendorId::Minimax => minimax::fetch(credential).await,
        VendorId::Kimi => kimi::fetch(credential).await,
        VendorId::Volcengine => volcengine::fetch(credential).await,
        VendorId::Mimo => mimo::fetch(credential).await,
        VendorId::Stepfun => stepfun::fetch(credential).await,
        VendorId::Iflytek => iflytek::fetch(credential).await,
        VendorId::GlmTeam => glm_team::fetch(credential).await,
        VendorId::Qoder => qoder::fetch(credential).await,
        VendorId::Cursor => cursor::fetch(credential).await,
        VendorId::Copilot => copilot::fetch(credential).await,
        VendorId::Ollama => ollama::fetch(credential).await,
        VendorId::Opencode => opencode::fetch(credential).await,
        VendorId::Claude => claude::fetch(credential).await,
        VendorId::Codex => codex::fetch(credential).await,
    }
}

/// Validate a credential by making a single API call. Returns Ok only when
/// the API responds successfully with valid data. ANY failure rejects the
/// credential, so invalid/expired keys are caught immediately at save time.
pub async fn validate(vendor: VendorId, credential: &str) -> Result<(), String> {
    match fetch(vendor, credential).await {
        Ok(_) => Ok(()),
        Err(e) => {
            let msg = e.to_string();
            eprintln!("[quota:validate] {vendor:?} failed: {msg}");
            Err(format_validate_error(&msg))
        }
    }
}

/// True when a vendor error looks like an auth/credential failure
/// (401 / 403 / unauthorized / explicit Auth variant). Used by adapters + the
/// scheduler to decide whether to surface a "凭证/Cookie 已失效" hint vs. a
/// generic network error.
pub fn is_auth_error(e: &VendorError) -> bool {
    if matches!(e, VendorError::Auth(_)) {
        return true;
    }
    let msg = e.to_string();
    msg.contains("401")
        || msg.contains("403")
        || msg.contains("unauthorized")
        || msg.contains("Unauthorized")
        || msg.contains("Forbidden")
}

/// Format a vendor error into a user-friendly message.
pub fn format_validate_error(msg: &str) -> String {
    if msg.contains("401")
        || msg.contains("403")
        || msg.contains("unauthorized")
        || msg.contains("Unauthorized")
    {
        "API 密钥无效或已过期，请检查后重试".into()
    } else if msg.contains("timeout")
        || msg.contains("timed out")
        || msg.contains("resolve")
        || msg.contains("No address")
        || msg.contains("connection refused")
    {
        "网络连接失败，请检查网络后重试".into()
    } else if msg.contains("empty credential") || msg.contains("缺少必需") {
        "请填写完整的凭证".into()
    } else {
        format!("凭证验证失败: {msg}")
    }
}
