//! Quota commands (M4 T4.5). Reads each vendor's credential from the keyring
//! and fetches its quota. V1 ships only DeepSeek; other vendors return nothing
//! here until their adapters + a binding exist.

use crate::quota::Quota;
use crate::{credentials, quota};

/// All configured vendors' quotas. Vendors without a stored credential or whose
/// fetch fails are skipped (the frontend shows an empty / unconfigured state).
#[tauri::command]
pub async fn get_quotas() -> Result<Vec<Quota>, String> {
    let mut out = Vec::new();

    // DeepSeek (balance-type, API key in keyring under "deepseek").
    if let Ok(cred) = credentials::get("deepseek") {
        if let Ok(q) = quota::fetch(quota::VendorId::Deepseek, &cred).await {
            out.push(q);
        }
    }

    Ok(out)
}
