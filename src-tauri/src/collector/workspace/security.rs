//! Workspace-key security validation (defense-in-depth).

/// Reject workspace keys that could escape the intended directory.
/// Encoded keys must start with `-`; absolute keys must start with `/`.
/// Both are checked for `..` components via `Path` decomposition (handles
/// encoded and unencoded forms). Data source is trusted (local tokscale
/// binary); this is a defense-in-depth safety net.
pub(crate) fn is_safe_workspace_key(key: &str) -> bool {
    if key.is_empty() {
        return false;
    }
    if std::path::Path::new(key)
        .components()
        .any(|c| c == std::path::Component::ParentDir)
    {
        return false;
    }
    key.starts_with('-') || key.starts_with('/')
}
