//! GLM Team (智谱团队) Coding Plan adapter.
//!
//! Team plan only exists on the China side (bigmodel.cn) — region is fixed.
//! Credential is `{ key, orgid, projid }` (Account.svelte multi-field vendor).
//!   GET {base}/api/monitor/usage/quota/limit?type=2
//!     Headers: Authorization: Bearer {key}
//!              bigmodel-organization: {orgid}
//!              bigmodel-project: {projid}
//! The response body is identical in shape to the personal GLM quota API, so we
//! reuse [`super::glm::parse`] and only override the vendor id + plan label.
//!
//! Faithfully ported from token-monitor src/shared/zaiTeamLimits.js.

use serde::Deserialize;

use super::glm;
use super::types::{Quota, QuotaStatus};
use super::VendorError;

const BASE_CN: &str = "https://open.bigmodel.cn";
const QUOTA_PATH: &str = "/api/monitor/usage/quota/limit?type=2";

/// Parse the `{ key, orgid, projid }` credential blob.
#[derive(Debug, Deserialize)]
struct Credential {
    key: String,
    #[serde(default)]
    orgid: Option<String>,
    #[serde(default, alias = "projid")]
    projid: Option<String>,
}

/// HTTP client (GET + Bearer + org/project headers). Injected for unit tests.
pub trait Http {
    fn get_team(
        &self,
        url: &str,
        key: &str,
        organization: &str,
        project: &str,
    ) -> Result<String, VendorError>;
}

/// Fetch via `http`. `credential` is the JSON `{key, orgid, projid}` blob.
pub fn fetch_with(http: &dyn Http, credential: &str) -> Result<Quota, VendorError> {
    let cred: Credential =
        serde_json::from_str(credential).map_err(|e| VendorError::Parse(e.to_string()))?;
    super::validate_header_safe(&cred.key)?;
    let organization = cred.orgid.as_deref().unwrap_or("").trim();
    let project = cred.projid.as_deref().unwrap_or("").trim();
    if organization.is_empty() || project.is_empty() {
        return Err(VendorError::Parse(
            "缺少组织 ID 或项目 ID".into(),
        ));
    }

    let url = format!("{BASE_CN}{QUOTA_PATH}");
    let body = http.get_team(&url, &cred.key, organization, project)?;
    let mut q = glm::parse(&body)?;
    // Override identity fields: this is the team plan, vendor id matches the
    // frontend's `zai_team` so the quota card maps to the right account row.
    q.vendor = "zai_team".into();
    q.plan_label = Some("Team".into());
    // Recompute status in case overrides changed windows (they don't today,
    // but keep it correct if glm::parse evolves).
    q.status = QuotaStatus::worst_of(
        q.windows
            .iter()
            .map(|w| QuotaStatus::from_used_pct(w.used_pct)),
    );
    Ok(q)
}

/// Default fetch (real network).
pub async fn fetch(credential: &str) -> Result<Quota, VendorError> {
    let cred = credential.to_string();
    tokio::task::spawn_blocking(move || fetch_with(&UreqHttp, &cred))
        .await
        .map_err(|e| VendorError::Network(format!("join: {e}")))?
}

struct UreqHttp;
impl Http for UreqHttp {
    fn get_team(
        &self,
        url: &str,
        key: &str,
        organization: &str,
        project: &str,
    ) -> Result<String, VendorError> {
        let resp = ureq::get(url)
            .set("Authorization", &format!("Bearer {key}"))
            .set("bigmodel-organization", organization)
            .set("bigmodel-project", project)
            .set("Accept", "application/json")
            .call();
        match resp {
            Ok(r) => r
                .into_string()
                .map_err(|e| VendorError::Network(e.to_string())),
            Err(ureq::Error::Status(code, _r)) => {
                if code == 401 || code == 403 {
                    Err(VendorError::Network("status code 401".into()))
                } else if code == 429 {
                    Err(VendorError::Network("status code 429".into()))
                } else {
                    Err(VendorError::Network(format!("status code {code}")))
                }
            }
            Err(e) => Err(VendorError::Network(e.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Mock;
    impl Http for Mock {
        fn get_team(
            &self,
            _url: &str,
            _key: &str,
            _org: &str,
            _proj: &str,
        ) -> Result<String, VendorError> {
            // Reuse a multi-window GLM-shaped payload (5h + weekly + MCP).
            Ok(r#"{"data":{"limits":[
                {"type":"TOKENS_LIMIT","unit":5,"number":300,"usage":1000,"remaining":700,"nextResetTime":1893456000},
                {"type":"TOKENS_LIMIT","unit":6,"number":1,"usage":1000,"remaining":220},
                {"type":"TIME_LIMIT","unit":5,"number":1,"usage":100,"remaining":10}
            ]}}"#
            .into())
        }
    }

    #[test]
    fn fetch_team_reuses_glm_parse() {
        let q = fetch_with(&Mock, r#"{"key":"sk-x","orgid":"org-1","projid":"proj-1"}"#).unwrap();
        assert_eq!(q.vendor, "zai_team");
        assert_eq!(q.plan_label.as_deref(), Some("Team"));
        assert_eq!(q.windows.len(), 3);
        let labels: Vec<&str> = q.windows.iter().map(|w| w.label.as_str()).collect();
        assert_eq!(labels, vec!["5h", "周", "MCP 月"]);
    }

    #[test]
    fn fetch_team_requires_org_and_project() {
        let err = fetch_with(&Mock, r#"{"key":"sk-x","orgid":"","projid":""}"#).unwrap_err();
        assert!(matches!(err, VendorError::Parse(_)));
        // Missing projid entirely.
        let err = fetch_with(&Mock, r#"{"key":"sk-x","orgid":"org-1"}"#).unwrap_err();
        assert!(matches!(err, VendorError::Parse(_)));
    }
}
