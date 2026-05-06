//! External tool resolution per SPEC §17.1.
//!
//! Each [`resolve_*`] function follows the same precedence chain:
//!
//! 1. The tool's `SFCWC_*` env var, if set and pointing at an existing
//!    file.
//! 2. (Where applicable) PATH lookup for the tool's executable name(s).
//! 3. (Where applicable) a workspace-relative default location.
//! 4. Otherwise: missing.
//!
//! Resolved tools are version-probed with `--version`; failures are
//! non-fatal (the tool stays resolved, `version` stays `None`).
//!
//! Mesen2 is intentionally never PATH-resolved or version-probed —
//! SPEC §17.1 specifies it is launched manually.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

/// Where the resolver found a tool.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolSource {
    /// Resolved from the tool's `SFCWC_*` env var.
    Env,
    /// Resolved by walking `PATH`.
    Path,
    /// Resolved at the workspace-relative default location.
    Default,
    /// Not found.
    Missing,
}

/// Result of attempting to resolve one external tool.
#[derive(Debug, Clone)]
pub struct ResolvedTool {
    pub name: String,
    pub resolved: bool,
    pub path: Option<PathBuf>,
    pub version: Option<String>,
    pub source: ToolSource,
    /// Human-readable list of resolution attempts, populated when
    /// resolution fails so the caller can produce a diagnostic that
    /// points the user at the right env var.
    pub searched: Vec<String>,
}

const ASAR_ENV: &str = "SFCWC_ASAR";
const ORACLE_ENV: &str = "SFCWC_SNES_SPC_ORACLE";
const MESEN2_ENV: &str = "SFCWC_MESEN2";

/// Resolve `asar`: env → PATH → missing.
pub fn resolve_asar() -> ResolvedTool {
    let mut searched = Vec::new();
    searched.push(format!("env:{ASAR_ENV}"));

    if let Some(p) = env_path(ASAR_ENV) {
        return ResolvedTool {
            name: "asar".to_string(),
            resolved: true,
            version: probe_version(&p),
            path: Some(p),
            source: ToolSource::Env,
            searched: Vec::new(),
        };
    }

    let names: &[&str] = if cfg!(windows) {
        &["asar.exe", "asar"]
    } else {
        &["asar"]
    };
    for name in names {
        searched.push((*name).to_string());
        if let Some(p) = which_on_path(name) {
            return ResolvedTool {
                name: "asar".to_string(),
                resolved: true,
                version: probe_version(&p),
                path: Some(p),
                source: ToolSource::Path,
                searched: Vec::new(),
            };
        }
    }

    ResolvedTool {
        name: "asar".to_string(),
        resolved: false,
        path: None,
        version: None,
        source: ToolSource::Missing,
        searched,
    }
}

/// Resolve the snes_spc oracle wrapper: env → `<workspace>/tools/snes_spc_oracle` → missing.
pub fn resolve_snes_spc_oracle(workspace_root: &Path) -> ResolvedTool {
    let mut searched = Vec::new();
    searched.push(format!("env:{ORACLE_ENV}"));

    if let Some(p) = env_path(ORACLE_ENV) {
        return ResolvedTool {
            name: "snes_spc_oracle".to_string(),
            resolved: true,
            version: probe_version(&p),
            path: Some(p),
            source: ToolSource::Env,
            searched: Vec::new(),
        };
    }

    let exe_name = if cfg!(windows) {
        "snes_spc_oracle.exe"
    } else {
        "snes_spc_oracle"
    };
    let default = workspace_root.join("tools").join(exe_name);
    let default_display = format!("tools/{exe_name}");
    searched.push(default_display);
    if default.is_file() {
        return ResolvedTool {
            name: "snes_spc_oracle".to_string(),
            resolved: true,
            version: probe_version(&default),
            path: Some(default),
            source: ToolSource::Default,
            searched: Vec::new(),
        };
    }

    ResolvedTool {
        name: "snes_spc_oracle".to_string(),
        resolved: false,
        path: None,
        version: None,
        source: ToolSource::Missing,
        searched,
    }
}

/// Resolve Mesen2: env-only per SPEC §17.1, no version probe.
pub fn resolve_mesen2() -> ResolvedTool {
    let mut searched = Vec::new();
    searched.push(format!("env:{MESEN2_ENV}"));

    if let Some(p) = env_path(MESEN2_ENV) {
        return ResolvedTool {
            name: "mesen2".to_string(),
            resolved: true,
            path: Some(p),
            version: None,
            source: ToolSource::Env,
            searched: Vec::new(),
        };
    }

    ResolvedTool {
        name: "mesen2".to_string(),
        resolved: false,
        path: None,
        version: None,
        source: ToolSource::Missing,
        searched,
    }
}

fn env_path(var: &str) -> Option<PathBuf> {
    let raw = std::env::var(var).ok()?;
    let p = PathBuf::from(raw);
    if p.is_file() {
        Some(p)
    } else {
        None
    }
}

fn which_on_path(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Best-effort version probe: invoke `<exe> --version`, take the first
/// non-empty line of stdout (or stderr if stdout is empty). Failures
/// — spawn error, non-zero exit, empty output — yield `None`.
fn probe_version(exe: &Path) -> Option<String> {
    let output = Command::new(exe).arg("--version").output().ok()?;
    let pick = |bytes: &[u8]| -> Option<String> {
        let s = String::from_utf8_lossy(bytes);
        s.lines().find_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
    };
    pick(&output.stdout).or_else(|| pick(&output.stderr))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asar_missing_when_env_unset_and_not_on_path() {
        // Hard to test deterministically without scrubbing PATH, but at
        // minimum: the call should not panic and should return a
        // structurally valid `ResolvedTool`.
        let r = resolve_asar();
        if !r.resolved {
            assert_eq!(r.source, ToolSource::Missing);
            assert!(r.path.is_none());
            assert!(!r.searched.is_empty());
            assert!(r.searched.iter().any(|s| s.starts_with("env:")));
        } else {
            assert!(matches!(r.source, ToolSource::Env | ToolSource::Path));
            assert!(r.path.is_some());
        }
    }

    #[test]
    fn mesen2_no_path_fallback() {
        // Without SFCWC_MESEN2 set to a real file, mesen2 must report
        // missing — even if a binary called `mesen2` exists on PATH.
        let r = resolve_mesen2();
        match std::env::var("SFCWC_MESEN2") {
            Ok(p) if std::path::Path::new(&p).is_file() => {
                assert!(r.resolved);
                assert_eq!(r.source, ToolSource::Env);
            }
            _ => {
                assert!(!r.resolved);
                assert_eq!(r.source, ToolSource::Missing);
            }
        }
    }

    #[test]
    fn oracle_records_default_search_path() {
        // We can't deterministically scrub SFCWC_SNES_SPC_ORACLE without
        // mutating env, so split on whether it was already set.
        let dummy_root = Path::new("Z:\\sfcwc-nonexistent-test-root");
        let r = resolve_snes_spc_oracle(dummy_root);
        if r.resolved {
            assert_eq!(r.source, ToolSource::Env);
        } else {
            assert_eq!(r.source, ToolSource::Missing);
            assert!(r.searched.iter().any(|s| s.contains("snes_spc_oracle")));
            assert!(r.searched.iter().any(|s| s.starts_with("env:")));
        }
    }
}
