//! Workspace-key security validation (defense-in-depth).

use std::path::Path;

/// Reject workspace keys that could escape the intended directory.
/// Valid keys: encoded (`-foo`), absolute (`/foo` or `C:\foo`), or plain
/// alphanumeric names (e.g. `ZCodeProject` from tokscale output). All are
/// checked for `..` components via `Path` decomposition.
pub(crate) fn is_safe_workspace_key(key: &str) -> bool {
    if key.is_empty() {
        return false;
    }
    if Path::new(key)
        .components()
        .any(|c| c == std::path::Component::ParentDir)
    {
        return false;
    }
    // Accept encoded keys, absolute paths (any OS), or plain alphanumeric names.
    key.starts_with('-')
        || key.starts_with('/')
        || key.chars().next().is_some_and(|c| c.is_alphabetic())
}
