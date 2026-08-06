//! tokscale integration: binary resolution, invocation, tolerant JSON parse.
//!
//! Real CLI (verified 2026-07-17, v4.5.3 — see docs/design.md §4.1):
//!   tokscale --json [--today|--month] [-c clients] [--group-by ...] [--no-spinner]
//!   tokscale graph [--week|--month]   (native JSON, no --json flag)
//!
//! tokscale writes its JSON to **stdout** and any `[tokscale] ...` progress /
//! network warnings to **stderr**; we capture the two streams separately, so
//! stdout is normally already clean JSON. The tolerant parser below is a safety
//! net for the rare case of leading log lines / whitespace on stdout.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde_json::Value;
use tauri::path::BaseDirectory;
use tauri::{AppHandle, Manager};

/// Errors raised by the tokscale layer.
#[derive(Debug, thiserror::Error)]
pub enum TokscaleError {
    /// No binary could be resolved (not installed, no custom path, not on PATH).
    #[error("tokscale binary not found; install it or set a custom path")]
    NotFound,
    /// stdout contained no `{` or `[` — nothing to parse.
    #[error("tokscale stdout contained no JSON")]
    NoJson,
    #[error("tokscale JSON parse failed: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("tokscale exited with status {code}: {stderr}")]
    NonZeroExit { code: i32, stderr: String },
    #[error("tokscale timed out after {0}s")]
    Timeout(u64),
    #[error("tokscale io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("tokscale download failed: {0}")]
    Download(String),
}

/// Global time period — maps to the popover DAY / MONTH / TOTAL switcher
/// (docs/design.md §F7). `All` corresponds to no range flag (full history).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Period {
    Today,
    Month,
    All,
}

impl Period {
    /// The tokscale range flag, if any. `All` → no flag (full range).
    pub fn flag(self) -> Option<&'static str> {
        match self {
            Period::Today => Some("--today"),
            Period::Month => Some("--month"),
            Period::All => None,
        }
    }
}

/// Index of the first `{`, preferring it over `[` so that tokscale log lines
/// like `[tokscale] ...` don't shadow an object payload. Returns the first `[`
/// only when no `{` is present. `None` if neither char occurs.
pub fn json_start(raw: &str) -> Option<usize> {
    match (raw.find('{'), raw.find('[')) {
        (Some(a), _) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

/// Tolerantly parse tokscale stdout into `T`. Tries, in order:
///   1. the whole string as-is (the common clean case);
///   2. from the first `{` (object payload after `[tokscale]` log lines);
///   3. from the first `[` (array payload with no object).
///
/// Limitation: a `[tokscale]`-prefixed *array* payload is not handled, but this
/// never occurs on real tokscale stdout (logs go to stderr).
pub fn parse_stdout<T: DeserializeOwned>(raw: &str) -> Result<T, TokscaleError> {
    // 1. fast path: clean output
    if let Ok(v) = serde_json::from_str(raw) {
        return Ok(v);
    }
    // 2/3. tolerant: skip leading logs/whitespace
    let start = json_start(raw).ok_or(TokscaleError::NoJson)?;
    serde_json::from_str(&raw[start..]).map_err(TokscaleError::Parse)
}

/// Build the argv for a `tokscale --json` report query.
pub fn report_args(period: Period, clients: &[String], group_by: &str) -> Vec<String> {
    let mut args = vec!["--json".to_string(), "--no-spinner".to_string()];
    if let Some(flag) = period.flag() {
        args.push(flag.to_string());
    }
    if !clients.is_empty() {
        args.push("-c".to_string());
        args.push(clients.join(","));
    }
    if !group_by.is_empty() {
        args.push("--group-by".to_string());
        args.push(group_by.to_string());
    }
    args
}

/// App data dir that holds the tokscale binary (created on first install):
/// `{data_local_dir}/token-usage/tokscale-bin`. Independent of the Tauri
/// handle so it stays unit-testable.
pub fn app_bin_dir() -> Option<PathBuf> {
    dirs::data_local_dir().map(|d| d.join("token-usage").join("tokscale-bin"))
}

/// tokscale version used for install fallback. Automatically bumped by
/// `scripts/fetch-tokscale.mjs --latest` at build time (reads npm registry
/// for the latest published @tokscale/cli-{triple} and writes back here).
pub const TOKSCALE_VERSION: &str = "4.10.0";

/// tokscale platform package suffix for the current compile target, matching
/// `@tokscale/cli-<suffix>` optionalDependencies.
pub fn platform_triple() -> &'static str {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let musl = cfg!(all(target_os = "linux", target_env = "musl"));
    match (os, arch) {
        ("macos", "aarch64") => "darwin-arm64",
        ("macos", "x86_64") => "darwin-x64",
        ("linux", "x86_64") if musl => "linux-x64-musl",
        ("linux", "x86_64") => "linux-x64-gnu",
        ("linux", "aarch64") => "linux-arm64-gnu",
        ("windows", "x86_64") => "win32-x64-msvc",
        ("windows", "aarch64") => "win32-arm64-msvc",
        _ => "linux-x64-gnu",
    }
}

/// Where the native binary lands after installing `@tokscale/cli` into `data_dir`:
/// `data_dir/node_modules/@tokscale/cli-<triple>/bin/tokscale[.exe]`.
pub fn installed_bin_path(data_dir: &Path) -> PathBuf {
    let exe = if cfg!(target_os = "windows") {
        "tokscale.exe"
    } else {
        "tokscale"
    };
    data_dir
        .join("node_modules")
        .join(format!("@tokscale/cli-{}", platform_triple()))
        .join("bin")
        .join(exe)
}

/// Scan `$PATH` for a `tokscale` executable (e.g. user did `npm i -g`).
pub fn find_on_path() -> Option<PathBuf> {
    let exe = if cfg!(target_os = "windows") {
        "tokscale.exe"
    } else {
        "tokscale"
    };
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(exe);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Candidate binary paths in priority order (three-tier strategy, §4.1):
///   1. user-supplied custom path;
///   2. the app-installed platform binary in `data_dir`;
///   3. a `tokscale` found on `$PATH`.
pub fn candidate_paths(custom: Option<&Path>, data_dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(c) = custom {
        out.push(c.to_path_buf());
    }
    out.push(installed_bin_path(data_dir));
    if let Some(p) = find_on_path() {
        out.push(p);
    }
    out
}

/// Pick the first existing candidate, else `NotFound`.
pub fn resolve_bin(custom: Option<&Path>, data_dir: &Path) -> Result<PathBuf, TokscaleError> {
    candidate_paths(custom, data_dir)
        .into_iter()
        .find(|p| p.is_file())
        .ok_or(TokscaleError::NotFound)
}

/// Resolve the bundled tokscale binary, if present.
///
/// - **dev (debug)**: `{src-tauri}/bin/tokscale` — the source-of-truth binary
///   that `fetch-tokscale.mjs` keeps fresh on every `pretauri` run. We use the
///   compile-time `CARGO_MANIFEST_DIR` rather than `BaseDirectory::Resource`
///   because the latter points at `target/debug/`, where a STALE copy from an
///   earlier build can linger (Tauri doesn't re-copy resources when only the
///   binary changed and `bundle.resources` may be undeclared).
/// - **prod (release)**: `$RESOURCE/bin/tokscale` inside the installed bundle.
///
/// Best-effort: returns `None` on any resolution error so callers fall back to
/// the legacy install path. On unix, ensures the exec bit is set (bundled files
/// may lose mode bits through signing/repackaging).
pub fn bundled_bin_path(app: &AppHandle) -> Option<PathBuf> {
    let rel = if cfg!(target_os = "windows") {
        "bin/tokscale.exe"
    } else {
        "bin/tokscale"
    };

    // Dev: prefer the fresh source binary over a possibly-stale target copy.
    #[cfg(debug_assertions)]
    {
        let dev_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
        if dev_path.is_file() {
            let _ = ensure_executable(&dev_path);
            return Some(dev_path);
        }
    }

    // Prod (or dev without a source checkout): use the bundled resource.
    let path = app.path().resolve(rel, BaseDirectory::Resource).ok()?;
    if !path.is_file() {
        return None;
    }
    // Best-effort chmod; if it fails we still return the path — the file exists
    // and exec may still work if the mode bit survived bundling.
    let _ = ensure_executable(&path);
    Some(path)
}

/// Ensure the bundled binary is executable on unix. No-op on Windows.
/// Idempotent. Mirrors the chmod in `install_from_tarball`.
#[cfg(unix)]
pub fn ensure_executable(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let perms = std::fs::metadata(path)?.permissions();
    let mode = perms.mode();
    if mode & 0o111 != 0o111 {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode | 0o755))?;
    }
    Ok(())
}

#[cfg(not(unix))]
pub fn ensure_executable(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

/// Spawn the resolved tokscale binary, return parsed JSON (tolerant).
/// stdout is parsed; on non-zero exit the stderr is surfaced.
///
/// A 90s timeout prevents a hung tokscale process from blocking the collector
/// forever (e.g. when the LiteLLM pricing fetch stalls on CN networks without
/// a warm cache). `TOKSCALE_PRICING_CACHE_ONLY=1` is forced so tokscale never
/// fetches the pricing table over the network; the timeout is a safety net.
pub const TOKSCALE_TIMEOUT_SECS: u64 = 90;

pub async fn run_json(bin: &Path, args: &[String]) -> Result<Value, TokscaleError> {
    let output = tokio::time::timeout(
        Duration::from_secs(TOKSCALE_TIMEOUT_SECS),
        tokio::process::Command::new(bin)
            .args(args)
            .env("TOKSCALE_PRICING_CACHE_ONLY", "1")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output(),
    )
    .await
    .map_err(|_| TokscaleError::Timeout(TOKSCALE_TIMEOUT_SECS))?
    .map_err(TokscaleError::Io)?;
    if !output.status.success() {
        return Err(TokscaleError::NonZeroExit {
            code: output.status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_stdout(&stdout)
}

// ── install (auto-tier): tarball download, no node dependency ─────────────

// ── pricing cache ──────────────────────────────────────────────────────────

/// Re-warm the pricing cache when it's older than this.
const PRICING_CACHE_MAX_AGE_SECS: u64 = 7 * 24 * 3600;

/// tokscale config dir: `$TOKSCALE_CONFIG_DIR` if set, else `~/.config/tokscale`
/// (tokscale uses XDG-style paths even on macOS — verified on this machine).
pub fn config_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("TOKSCALE_CONFIG_DIR") {
        return Some(PathBuf::from(d));
    }
    dirs::home_dir().map(|h| h.join(".config").join("tokscale"))
}

/// Path to the cached LiteLLM pricing table, when the config dir is resolvable.
pub fn pricing_cache_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("cache").join("pricing-litellm.json"))
}

/// Pure: a cache file is stale when missing or older than the threshold.
fn cache_file_stale(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return true;
    };
    let Ok(m) = meta.modified() else {
        return true;
    };
    let Ok(age) = m.elapsed() else {
        return true;
    };
    age > std::time::Duration::from_secs(PRICING_CACHE_MAX_AGE_SECS)
}

/// Whether the pricing cache is missing or too old.
pub fn pricing_cache_stale() -> bool {
    pricing_cache_path()
        .map(|p| cache_file_stale(&p))
        .unwrap_or(true)
}

/// Fire-and-forget background warm of the pricing cache. Runs one tokscale
/// report **without** `TOKSCALE_PRICING_CACHE_ONLY` so it fetches + caches the
/// table; no-op when the cache is fresh. Returns immediately; failures only
/// mean cost estimates stay $0 until the next successful warm.
pub fn ensure_pricing_cache(bin: &Path) {
    if !pricing_cache_stale() {
        return;
    }
    let bin = bin.to_path_buf();
    tokio::task::spawn_blocking(move || {
        // Any cost-bearing report fetches+caches pricing as a side effect;
        // --today keeps the scan small. Result ignored.
        let _ = std::process::Command::new(&bin)
            .args(["--json", "--no-spinner", "--today"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    });
}

/// Default npm registry. Override with the `TOKSCALE_REGISTRY` env var
/// (e.g. `https://registry.npmmirror.com` on CN networks where npmjs.org is slow).
pub const DEFAULT_REGISTRY: &str = "https://registry.npmjs.org";

/// Effective registry, honoring the `TOKSCALE_REGISTRY` override.
pub fn registry() -> String {
    std::env::var("TOKSCALE_REGISTRY").unwrap_or_else(|_| DEFAULT_REGISTRY.to_string())
}

/// npm tarball URL for a platform package.
/// e.g. https://registry.npmjs.org/@tokscale/cli-darwin-arm64/-/cli-darwin-arm64-4.5.3.tgz
pub fn tarball_url(triple: &str, version: &str, registry_base: &str) -> String {
    format!("{registry_base}/@tokscale/cli-{triple}/-/cli-{triple}-{version}.tgz")
}

/// Extract the native binary from a downloaded platform tarball and place it
/// where `installed_bin_path` expects. Tarball layout: `package/bin/tokscale`.
/// Splits download (network) from extraction (fs) so extraction is unit-testable.
pub fn install_from_tarball(data_dir: &Path, bytes: &[u8]) -> Result<PathBuf, TokscaleError> {
    let exe = if cfg!(target_os = "windows") {
        "tokscale.exe"
    } else {
        "tokscale"
    };
    let target = installed_bin_path(data_dir);
    std::fs::create_dir_all(target.parent().unwrap())?;

    let gz = flate2::read::GzDecoder::new(bytes);
    let mut ar = tar::Archive::new(gz);
    for entry in ar.entries()? {
        let mut e = entry?;
        let path = e.path()?;
        let s = path.to_string_lossy().into_owned();
        // Normalize to forward slashes for cross-platform matching
        // (tar stores / on all platforms, but Path::to_string_lossy may
        // produce \ on Windows).
        let norm = s.replace('\\', "/");
        let exe_in_archive = format!("package/bin/{}", exe);
        let exe_no_ext = "package/bin/tokscale";
        // platform tarball stores the binary at package/bin/tokscale[.exe]
        // Always accept the no-extension form (test tarballs use it on all
        // platforms); also accept the platform-specific form.
        if norm == exe_no_ext
            || norm.ends_with(exe_no_ext)
            || norm == exe_in_archive
            || norm.ends_with(&exe_in_archive)
        {
            let mut buf = Vec::new();
            e.read_to_end(&mut buf)?;
            std::fs::write(&target, &buf)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755))?;
            }
            return Ok(target);
        }
    }
    Err(TokscaleError::NotFound)
}

/// Download the platform tarball and install the binary into `data_dir`.
/// Network IO; kept thin. Resolves open question §16.1 (direct download, no npm).
pub async fn install(data_dir: &Path) -> Result<PathBuf, TokscaleError> {
    let url = tarball_url(platform_triple(), TOKSCALE_VERSION, &registry());
    let dir = data_dir.to_path_buf();
    tokio::task::spawn_blocking(move || -> Result<PathBuf, TokscaleError> {
        let resp = ureq::get(&url)
            .call()
            .map_err(|e| TokscaleError::Download(e.to_string()))?;
        let mut bytes = Vec::new();
        resp.into_reader()
            .read_to_end(&mut bytes)
            .map_err(TokscaleError::Io)?;
        install_from_tarball(&dir, &bytes)
    })
    .await
    .map_err(|e| TokscaleError::Download(format!("join error: {e}")))?
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── pricing cache ─────────────────────────────────────────────────────
    #[test]
    fn cache_file_stale_when_missing() {
        assert!(cache_file_stale(Path::new(
            "/no/such/path/here/pricing.json"
        )));
    }

    #[test]
    fn cache_file_not_stale_when_fresh() {
        let tmp = std::env::temp_dir().join("tu_pricing_fresh.json");
        let _ = std::fs::remove_file(&tmp);
        std::fs::write(&tmp, b"{}").unwrap();
        // Just created → mtime is now → well within the 7-day threshold.
        assert!(!cache_file_stale(&tmp));
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn config_dir_respects_env_override() {
        // env var wins over the default ~/.config/tokscale.
        let saved = std::env::var_os("TOKSCALE_CONFIG_DIR");
        std::env::set_var("TOKSCALE_CONFIG_DIR", "/custom/dir");
        assert_eq!(
            config_dir().as_deref(),
            Some(std::path::Path::new("/custom/dir"))
        );
        // restore
        match saved {
            Some(v) => std::env::set_var("TOKSCALE_CONFIG_DIR", v),
            None => std::env::remove_var("TOKSCALE_CONFIG_DIR"),
        }
    }

    #[test]
    fn pricing_cache_path_under_config_dir() {
        let saved = std::env::var_os("TOKSCALE_CONFIG_DIR");
        std::env::set_var("TOKSCALE_CONFIG_DIR", "/custom/dir");
        assert_eq!(
            pricing_cache_path().as_deref(),
            Some(std::path::Path::new(
                "/custom/dir/cache/pricing-litellm.json"
            ))
        );
        match saved {
            Some(v) => std::env::set_var("TOKSCALE_CONFIG_DIR", v),
            None => std::env::remove_var("TOKSCALE_CONFIG_DIR"),
        }
    }

    // ── json_start ─────────────────────────────────────────────────────────
    #[test]
    fn json_start_prefers_object_over_array() {
        // A tokscale log line starts with '['; the object must win.
        let raw = "[tokscale] network error\n{\"groupBy\":\"client\"}";
        let i = json_start(raw).unwrap();
        assert_eq!(&raw[i..i + 1], "{");
    }

    #[test]
    fn json_start_falls_back_to_array() {
        assert_eq!(json_start("[1,2,3]"), Some(0));
    }

    #[test]
    fn json_start_none_when_absent() {
        assert_eq!(json_start("just plain text"), None);
    }

    // ── parse_stdout (tolerant) ──────────────────────────────────────────
    #[test]
    fn parse_clean_object() {
        let v: Value = parse_stdout(r#"{"a":1,"b":2}"#).unwrap();
        assert_eq!(v["a"], 1);
        assert_eq!(v["b"], 2);
    }

    #[test]
    fn parse_clean_array() {
        let v: Vec<Value> = parse_stdout("[1,2,3]").unwrap();
        assert_eq!(v, vec![1, 2, 3]);
    }

    #[test]
    fn parse_object_after_tokscale_log_lines() {
        // Realistic: stderr merged in or a stray stdout log, then clean object.
        let raw = "[tokscale] LiteLLM network error (attempt 1/3)\n[tokscale] done\n{\"groupBy\":\"client,model\",\"totalInput\":204980}";
        let v: Value = parse_stdout(raw).unwrap();
        assert_eq!(v["groupBy"], "client,model");
        assert_eq!(v["totalInput"], 204980);
    }

    #[test]
    fn parse_leading_whitespace() {
        let v: Value = parse_stdout("\n   \n  {\"x\":42}").unwrap();
        assert_eq!(v["x"], 42);
    }

    #[test]
    fn parse_trailing_newline_ok() {
        let v: Value = parse_stdout("{\"a\":1}\n\n").unwrap();
        assert_eq!(v["a"], 1);
    }

    #[test]
    fn parse_no_json_errors() {
        assert!(matches!(
            parse_stdout::<Value>("no json here"),
            Err(TokscaleError::NoJson)
        ));
    }

    #[test]
    fn parse_invalid_json_errors_as_parse() {
        // Has a '{' but not valid JSON → Parse error (not NoJson).
        assert!(matches!(
            parse_stdout::<Value>("{ broken"),
            Err(TokscaleError::Parse(_))
        ));
    }

    // ── report_args ─────────────────────────────────────────────────────
    #[test]
    fn args_today_minimal() {
        let a = report_args(Period::Today, &[], "");
        assert_eq!(a, vec!["--json", "--no-spinner", "--today"]);
    }

    #[test]
    fn args_month_with_clients_and_group() {
        let clients = vec!["claude".to_string(), "codex".to_string()];
        let a = report_args(Period::Month, &clients, "client,model");
        assert_eq!(
            a,
            vec![
                "--json",
                "--no-spinner",
                "--month",
                "-c",
                "claude,codex",
                "--group-by",
                "client,model"
            ]
        );
    }

    #[test]
    fn args_all_has_no_range_flag() {
        let a = report_args(Period::All, &[], "");
        // --json/--no-spinner are not range flags.
        assert!(!a
            .iter()
            .any(|x| matches!(x.as_str(), "--today" | "--month")));
        assert!(a.contains(&"--json".to_string()));
    }

    // ── platform_triple / installed_bin_path ────────────────────────────
    #[test]
    fn platform_triple_matches_current_target() {
        let t = platform_triple();
        // Must be one of the 8 tokscale platform packages.
        const VALID: &[&str] = &[
            "darwin-arm64",
            "darwin-x64",
            "linux-x64-gnu",
            "linux-x64-musl",
            "linux-arm64-gnu",
            "linux-arm64-musl",
            "win32-x64-msvc",
            "win32-arm64-msvc",
        ];
        assert!(VALID.contains(&t), "unexpected triple {t}");
    }

    #[test]
    fn installed_bin_path_contains_platform_pkg() {
        let p = installed_bin_path(Path::new("/data"));
        let s = p.to_string_lossy();
        assert!(s.contains("@tokscale/cli-"), "{s}");
        assert!(s.contains("bin"), "{s}");
        assert!(
            s.ends_with("tokscale") || s.ends_with("tokscale.exe"),
            "{s}"
        );
    }

    // ── resolve / candidate_paths ───────────────────────────────────────
    #[test]
    fn custom_path_takes_priority() {
        let dir = std::env::temp_dir().join("tu_test_custom");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let bin = dir.join(if cfg!(windows) {
            "tokscale.exe"
        } else {
            "tokscale"
        });
        std::fs::write(&bin, b"#!/bin/sh\necho hi").unwrap();
        let data = std::env::temp_dir().join("tu_test_data_empty");
        let resolved = resolve_bin(Some(&bin), &data).unwrap();
        assert_eq!(resolved, bin);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_finds_installed_binary_in_data_dir() {
        // Simulate an install: place the binary where installed_bin_path expects.
        let data = std::env::temp_dir().join("tu_test_installed");
        let _ = std::fs::remove_dir_all(&data);
        let bin = installed_bin_path(&data);
        std::fs::create_dir_all(bin.parent().unwrap()).unwrap();
        std::fs::write(&bin, b"#!/bin/sh\necho hi").unwrap();
        assert_eq!(resolve_bin(None, &data).unwrap(), bin);
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn resolve_returns_not_found_when_absent() {
        let empty = std::env::temp_dir().join("tu_test_resolve_empty");
        let _ = std::fs::remove_dir_all(&empty);
        assert!(matches!(
            resolve_bin(None, &empty),
            Err(TokscaleError::NotFound)
        ));
    }

    // ── install (tarball) ───────────────────────────────────────────────
    #[test]
    fn tarball_url_format() {
        let u = tarball_url("darwin-arm64", "4.5.3", "https://registry.npmjs.org");
        assert_eq!(
            u,
            "https://registry.npmjs.org/@tokscale/cli-darwin-arm64/-/cli-darwin-arm64-4.5.3.tgz"
        );
    }

    #[test]
    fn tarball_url_respects_mirror() {
        let u = tarball_url("linux-x64-gnu", "4.5.3", "https://registry.npmmirror.com");
        assert_eq!(
            u,
            "https://registry.npmmirror.com/@tokscale/cli-linux-x64-gnu/-/cli-linux-x64-gnu-4.5.3.tgz"
        );
    }

    #[test]
    fn install_from_tarball_extracts_binary() {
        // Build an in-memory tarball mimicking the platform package layout:
        // package/bin/tokscale  (a fake payload).
        let mut tar_bytes = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut tar_bytes);
            let mut header = tar::Header::new_gnu();
            let payload = b"#!/bin/sh\necho fake-tokscale";
            header.set_size(payload.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder
                .append_data(&mut header, "package/bin/tokscale", &payload[..])
                .unwrap();
            builder.finish().unwrap();
        }
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        use std::io::Write;
        gz.write_all(&tar_bytes).unwrap();
        let tarball = gz.finish().unwrap();

        let data = std::env::temp_dir().join("tu_test_install_tarball");
        let _ = std::fs::remove_dir_all(&data);
        let installed = install_from_tarball(&data, &tarball).unwrap();
        assert_eq!(installed, installed_bin_path(&data));
        let content = std::fs::read(&installed).unwrap();
        assert_eq!(content, b"#!/bin/sh\necho fake-tokscale");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&installed).unwrap().permissions().mode();
            assert!(mode & 0o111 != 0, "expected exec bit, got mode {mode:o}");
        }
        // and resolve_bin must now find it.
        assert_eq!(resolve_bin(None, &data).unwrap(), installed);
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn install_from_tarball_missing_binary_errors() {
        // Tarball without package/bin/tokscale → NotFound.
        let mut tar_bytes = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut tar_bytes);
            let mut header = tar::Header::new_gnu();
            header.set_size(3);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(&mut header, "package/README", &b"hi"[..])
                .unwrap();
            builder.finish().unwrap();
        }
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        use std::io::Write;
        gz.write_all(&tar_bytes).unwrap();
        let tarball = gz.finish().unwrap();

        let data = std::env::temp_dir().join("tu_test_install_empty");
        let _ = std::fs::remove_dir_all(&data);
        assert!(matches!(
            install_from_tarball(&data, &tarball),
            Err(TokscaleError::NotFound)
        ));
        let _ = std::fs::remove_dir_all(&data);
    }

    // ── timeout ──────────────────────────────────────────────────────────
    #[test]
    fn timeout_error_variant_message() {
        let err = TokscaleError::Timeout(90);
        assert!(format!("{err}").contains("90"));
    }

    #[test]
    fn timeout_is_distinct_from_other_errors() {
        assert!(matches!(
            TokscaleError::Timeout(90),
            TokscaleError::Timeout(_)
        ));
        assert!(!matches!(
            TokscaleError::Timeout(90),
            TokscaleError::NonZeroExit { .. }
        ));
    }
}
