//! StepFun passport login flow — username/password → Oasis-Token.
//!
//! The browser-console Cookie route dies within minutes: the web console
//! rotates its session server-side (single-device policy) and revokes the
//! copied token. This module performs the app's OWN login through the
//! platform's Passport Connect-RPC API, mirroring the web client:
//!
//! 1. `GET https://platform.stepfun.com` → `INGRESSCOOKIE` (from Set-Cookie)
//! 2. `POST …/passport/proto.api.passport.v1.PassportService/RegisterDevice`
//!    (empty body) → anonymous token pair
//! 3. `POST …/passport/…/SignInByPassword` `{"username","password"}` →
//!    authenticated token pair
//! 4. `POST …/passport/…/RefreshToken` (empty body, current token) → renewed
//!    pair — keeps the session alive without re-sending credentials
//!
//! Tokens are combined as `access...refresh` (three-dot separator, matching
//! the web client's own storage format). The `Oasis-Webid` header/cookie must
//! equal the token's `device_id` JWT claim or the server answers
//! "oasis-token is embezzled".

use super::VendorError;
use tracing::debug;

const PLATFORM_URL: &str = "https://platform.stepfun.com";
const REGISTER_DEVICE_URL: &str =
    "https://platform.stepfun.com/passport/proto.api.passport.v1.PassportService/RegisterDevice";
const SIGN_IN_URL: &str =
    "https://platform.stepfun.com/passport/proto.api.passport.v1.PassportService/SignInByPassword";
const REFRESH_URL: &str =
    "https://platform.stepfun.com/passport/proto.api.passport.v1.PassportService/RefreshToken";
const OASIS_APPID: &str = "10300";
/// Fallback webid for the pre-login device-registration step only — the real
/// value comes from the token's `device_id` claim afterwards.
const DEFAULT_WEBID: &str = "c8a1002d2c457e758785a9979832217c7c0b884c";
const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
const TIMEOUT_SECS: u64 = 15;

/// HTTP abstraction for the passport flow (testable; see `UreqPassportHttp`).
pub trait PassportHttp {
    /// GET `url` and return every `Set-Cookie` header value from the response.
    fn get_set_cookies(&self, url: &str) -> Result<Vec<String>, VendorError>;
    /// POST `body` with the given `Cookie` header and `oasis-webid`.
    fn post_json(
        &self,
        url: &str,
        cookie: &str,
        webid: &str,
        body: &str,
    ) -> Result<String, VendorError>;
}

// ── Token helpers ───────────────────────────────────────────────────────────

/// Decode a JWT payload (no signature verification) and pull `device_id`.
pub fn device_id_from_jwt(jwt: &str) -> Option<String> {
    use base64::Engine;
    let parts: Vec<&str> = jwt.split('.').collect();
    if parts.len() < 2 {
        return None;
    }
    let padded = {
        let mut p = parts[1].replace('-', "+").replace('_', "/");
        while p.len() % 4 != 0 {
            p.push('=');
        }
        p
    };
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(padded.as_bytes())
        .ok()?;
    let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    v.get("device_id")
        .and_then(|d| d.as_str())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
}

/// The `Oasis-Webid` for a combined `access...refresh` token: prefer the
/// refresh half (the device_id claim lives there), fall back to the access
/// half, then to the constant default.
pub fn webid_for_token(combined: &str) -> String {
    for half in combined.rsplit("...") {
        if let Some(id) = device_id_from_jwt(half) {
            return id;
        }
    }
    DEFAULT_WEBID.to_string()
}

/// The access half of a combined token (a bare JWT passes through).
pub fn access_half(combined: &str) -> &str {
    combined.split("...").next().unwrap_or(combined)
}

/// Access half + refresh half (when present) joined the way the web client
/// stores them.
fn combined_token(access: &str, refresh: Option<&str>) -> String {
    match refresh.filter(|r| !r.is_empty()) {
        Some(r) => format!("{access}...{r}"),
        None => access.to_string(),
    }
}

// ── Passport responses ──────────────────────────────────────────────────────

fn token_from_response(body: &str, step: &str) -> Result<String, VendorError> {
    let v: serde_json::Value = serde_json::from_str(body)
        .map_err(|e| VendorError::Parse(format!("stepfun {step}: {e}")))?;
    let access = v
        .pointer("/accessToken/raw")
        .and_then(|x| x.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| VendorError::Parse(format!("stepfun {step}: no accessToken")))?;
    let refresh = v
        .pointer("/refreshToken/raw")
        .and_then(|x| x.as_str())
        .unwrap_or("");
    Ok(combined_token(access, Some(refresh)))
}

// ── Login flow ──────────────────────────────────────────────────────────────

/// Full 3-step login: homepage → RegisterDevice → SignInByPassword.
pub fn full_login(
    http: &dyn PassportHttp,
    username: &str,
    password: &str,
) -> Result<String, VendorError> {
    if username.trim().is_empty() || password.is_empty() {
        return Err(VendorError::Auth(
            "StepFun 用户名或密码为空，请检查设置".into(),
        ));
    }
    // 1. INGRESSCOOKIE from the homepage's Set-Cookie headers.
    let cookies = http.get_set_cookies(PLATFORM_URL)?;
    let ingress = cookies
        .iter()
        .filter_map(|c| {
            let mut it = c.split(';');
            let first = it.next()?;
            let (name, value) = first.split_once('=')?;
            (name.trim() == "INGRESSCOOKIE").then(|| value.trim().to_string())
        })
        .next()
        .ok_or_else(|| VendorError::Network("stepfun login: no INGRESSCOOKIE".into()))?;

    // 2. RegisterDevice → anonymous token.
    let anon = http.post_json(
        REGISTER_DEVICE_URL,
        &format!("INGRESSCOOKIE={ingress}"),
        DEFAULT_WEBID,
        "{}",
    )?;
    let anon_token = token_from_response(&anon, "RegisterDevice")?;

    // 3. SignInByPassword with the anonymous session. The passport endpoints
    // require the FULL combined `access...refresh` string in Oasis-Token —
    // the access half alone is rejected with CODE_TOKEN_ILLEGAL (verified
    // against the live API 2026-08-29).
    let webid = webid_for_token(&anon_token);
    let body = serde_json::json!({ "username": username, "password": password }).to_string();
    let login = http.post_json(
        SIGN_IN_URL,
        &format!("Oasis-Token={anon_token}; Oasis-Webid={webid}; INGRESSCOOKIE={ingress}"),
        &webid,
        &body,
    )?;
    token_from_response(&login, "SignInByPassword")
}

/// Renew a combined token via the RefreshToken endpoint.
pub fn refresh(http: &dyn PassportHttp, combined: &str) -> Result<String, VendorError> {
    if combined.trim().is_empty() {
        return Err(VendorError::Auth("stepfun refresh: empty token".into()));
    }
    let webid = webid_for_token(combined);
    let body = http.post_json(
        REFRESH_URL,
        // Full combined token, same requirement as SignInByPassword.
        &format!("Oasis-Token={combined}; Oasis-Webid={webid}"),
        &webid,
        "{}",
    )?;
    token_from_response(&body, "RefreshToken")
}

// ── Production HTTP impl ────────────────────────────────────────────────────

pub struct UreqPassportHttp;

impl PassportHttp for UreqPassportHttp {
    fn get_set_cookies(&self, url: &str) -> Result<Vec<String>, VendorError> {
        let agent = ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(TIMEOUT_SECS))
            .redirects(0)
            .build();
        let resp = agent
            .get(url)
            .set("Accept", "text/html,application/xhtml+xml")
            .set("User-Agent", USER_AGENT)
            .call();
        let resp = match resp {
            Ok(r) => r,
            Err(ureq::Error::Status(_code, r)) => r, // Set-Cookie may exist on redirects
            Err(e) => return Err(VendorError::Network(e.to_string())),
        };
        Ok(resp
            .headers_names()
            .into_iter()
            .filter(|n| n.eq_ignore_ascii_case("set-cookie"))
            .flat_map(|name| resp.all(&name).into_iter().map(|v| v.to_string()))
            .collect())
    }

    fn post_json(
        &self,
        url: &str,
        cookie: &str,
        webid: &str,
        body: &str,
    ) -> Result<String, VendorError> {
        let agent = ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(TIMEOUT_SECS))
            .redirects(0)
            .build();
        let resp = agent
            .post(url)
            .set("Content-Type", "application/json")
            .set("Accept", "*/*")
            .set("oasis-appid", OASIS_APPID)
            .set("oasis-platform", "web")
            .set("oasis-webid", webid)
            .set("Origin", PLATFORM_URL)
            .set("Referer", PLATFORM_URL)
            .set("User-Agent", USER_AGENT)
            .set("Cookie", cookie)
            .send_string(body);
        match resp {
            Ok(r) => r
                .into_string()
                .map_err(|e| VendorError::Network(e.to_string())),
            Err(ureq::Error::Status(code, r)) => {
                let body = r.into_string().unwrap_or_default();
                // The API reports a wrong password as HTTP 400 with
                // CODE_ACCOUNT_PASSWORD_IS_WRONG — surface it as an explicit
                // auth error instead of a raw body dump.
                if body.contains("CODE_ACCOUNT_PASSWORD_IS_WRONG") {
                    return Err(VendorError::Auth(
                        "StepFun 账号或密码错误，请检查后重试".into(),
                    ));
                }
                if code == 401 || code == 403 || (300..400).contains(&code) {
                    // Log the response body for debugging — 401s from
                    // SignInByPassword can carry useful detail (e.g. expired
                    // anonymous token) that would otherwise be lost.
                    if !body.is_empty() {
                        debug!(?code, ?body, "stepfun passport auth status");
                    }
                    return Err(VendorError::Network(format!("status code {code}")));
                }
                Err(VendorError::Api { status: code, body })
            }
            Err(e) => Err(VendorError::Network(e.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// JWT with `{"device_id":"dev-abc","exp":123}` payload.
    fn jwt_with_device(device: &str) -> String {
        use base64::Engine;
        let header =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b"{\"alg\":\"HS256\"}");
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(format!("{{\"device_id\":\"{device}\"}}"));
        format!("{header}.{payload}.sig")
    }

    // ── JWT / webid ──

    #[test]
    fn device_id_from_jwt_decodes_payload() {
        assert_eq!(
            device_id_from_jwt(&jwt_with_device("dev-abc")).as_deref(),
            Some("dev-abc")
        );
    }

    #[test]
    fn device_id_from_jwt_rejects_garbage() {
        assert!(device_id_from_jwt("not-a-jwt").is_none());
        assert!(device_id_from_jwt("").is_none());
    }

    #[test]
    fn webid_prefers_refresh_half() {
        let access = jwt_with_device("dev-access");
        let refresh = jwt_with_device("dev-refresh");
        let combined = format!("{access}...{refresh}");
        assert_eq!(webid_for_token(&combined), "dev-refresh");
        // Bare token falls back to its own half.
        assert_eq!(webid_for_token(&access), "dev-access");
        // Undecodable → default constant.
        assert_eq!(webid_for_token("garbage"), DEFAULT_WEBID);
    }

    #[test]
    fn access_half_splits_on_separator() {
        assert_eq!(access_half("a...b"), "a");
        assert_eq!(access_half("bare"), "bare");
    }

    // ── Mock HTTP ──

    struct MockPassport {
        set_cookies: Vec<String>,
        responses: Mutex<Vec<Result<String, VendorError>>>,
        calls: Mutex<Vec<(String, String)>>, // (url, cookie)
    }
    impl MockPassport {
        fn new(set_cookies: Vec<String>, responses: Vec<Result<String, VendorError>>) -> Self {
            Self {
                set_cookies,
                responses: Mutex::new(responses),
                calls: Mutex::new(Vec::new()),
            }
        }
    }
    impl PassportHttp for MockPassport {
        fn get_set_cookies(&self, _url: &str) -> Result<Vec<String>, VendorError> {
            Ok(self.set_cookies.clone())
        }
        fn post_json(
            &self,
            url: &str,
            cookie: &str,
            _webid: &str,
            _body: &str,
        ) -> Result<String, VendorError> {
            self.calls
                .lock()
                .unwrap()
                .push((url.to_string(), cookie.to_string()));
            self.responses.lock().unwrap().remove(0)
        }
    }

    fn token_body(device: &str) -> String {
        serde_json::json!({
            "accessToken": {"raw": jwt_with_device(device)},
            "refreshToken": {"raw": jwt_with_device(&(device.to_string() + "-r"))}
        })
        .to_string()
    }

    // ── full_login ──

    #[test]
    fn full_login_performs_three_steps() {
        let http = MockPassport::new(
            vec!["INGRESSCOOKIE=ing-123; Path=/; HttpOnly".into()],
            vec![Ok(token_body("dev-anon")), Ok(token_body("dev-user"))],
        );
        let token = full_login(&http, "user@example.com", "pw").unwrap();
        assert_eq!(webid_for_token(&token), "dev-user-r");
        // SignInByPassword must receive the FULL combined anon token — the
        // access half alone gets CODE_TOKEN_ILLEGAL from the live API.
        let calls = http.calls.lock().unwrap();
        assert!(calls[0].0.contains("RegisterDevice"));
        let (_, sign_in_cookie) = &calls[1];
        assert!(calls[1].0.contains("SignInByPassword"));
        assert!(sign_in_cookie.contains("..."), "combined token required");
        let anon_combined = format!(
            "{}...{}",
            jwt_with_device("dev-anon"),
            jwt_with_device("dev-anon-r")
        );
        assert!(
            sign_in_cookie.contains(&anon_combined),
            "cookie must carry the full RegisterDevice token pair"
        );
    }

    #[test]
    fn full_login_without_ingress_cookie_fails() {
        let http = MockPassport::new(vec!["OTHER=x".into()], vec![]);
        let err = full_login(&http, "u", "p").unwrap_err();
        assert!(err.to_string().contains("INGRESSCOOKIE"));
    }

    #[test]
    fn full_login_rejects_empty_credentials() {
        let http = MockPassport::new(vec!["INGRESSCOOKIE=x".into()], vec![]);
        assert!(full_login(&http, "", "p").is_err());
        assert!(full_login(&http, "u", "").is_err());
    }

    #[test]
    fn full_login_bad_credentials_is_auth_error() {
        let http = MockPassport::new(
            vec!["INGRESSCOOKIE=ing".into()],
            vec![
                Ok(token_body("dev-anon")),
                Err(VendorError::Network("status code 401".into())),
            ],
        );
        let err = full_login(&http, "u", "wrong").unwrap_err();
        assert!(err.to_string().contains("401"));
    }

    // ── refresh ──

    #[test]
    fn refresh_returns_new_combined_token() {
        let http = MockPassport::new(vec![], vec![Ok(token_body("dev-new"))]);
        let token = refresh(&http, "old...token").unwrap();
        assert_eq!(webid_for_token(&token), "dev-new-r");
        let calls = http.calls.lock().unwrap();
        assert!(calls[0].0.contains("RefreshToken"));
        // The refresh call must also carry the full combined token.
        assert!(calls[0].1.contains("Oasis-Token=old...token"));
    }

    #[test]
    fn refresh_rejects_empty_token() {
        let http = MockPassport::new(vec![], vec![]);
        assert!(refresh(&http, "").is_err());
    }
}
