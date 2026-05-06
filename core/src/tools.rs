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
//! Mesen2 is never version-probed (SPEC §17.1 — launched manually).
//! M1.6 added a PATH fallback after PM confirmed Mesen2 ships as a
//! single GUI binary `Mesen.exe` (no separate CLI); the resolver
//! looks for `Mesen.exe` / `Mesen` / `Mesen2.exe` after the env-var
//! check.

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

/// Resolve the `snes_spc_oracle` wrapper.
///
/// Resolution order:
/// 1. `SFCWC_SNES_SPC_ORACLE` env var (if set and points at a real file).
/// 2. `<workspace>/tools/snes_spc_oracle/build/Release/snes_spc_oracle(.exe)`
///    — the CMake/MSVC default location M0.5 added.
/// 3. `<workspace>/tools/snes_spc_oracle/build/snes_spc_oracle(.exe)`
///    — single-config generators (Ninja, Unix Makefiles, clang).
/// 4. Missing.
pub fn resolve_snes_spc_oracle(workspace_root: &Path) -> ResolvedTool {
    let exe = if cfg!(windows) {
        "snes_spc_oracle.exe"
    } else {
        "snes_spc_oracle"
    };
    let mut searched = vec![format!("env:{ORACLE_ENV}")];

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

    let candidates: [(PathBuf, String); 2] = [
        (
            workspace_root
                .join("tools")
                .join("snes_spc_oracle")
                .join("build")
                .join("Release")
                .join(exe),
            format!("tools/snes_spc_oracle/build/Release/{exe}"),
        ),
        (
            workspace_root
                .join("tools")
                .join("snes_spc_oracle")
                .join("build")
                .join(exe),
            format!("tools/snes_spc_oracle/build/{exe}"),
        ),
    ];
    for (cand, display) in &candidates {
        searched.push(display.clone());
        if cand.is_file() {
            return ResolvedTool {
                name: "snes_spc_oracle".to_string(),
                resolved: true,
                version: probe_version(cand),
                path: Some(cand.clone()),
                source: ToolSource::Default,
                searched: Vec::new(),
            };
        }
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

/// Resolve Mesen2: env → PATH → missing. No version probe per
/// SPEC §17.1 (Mesen2 is the manual-audition path only).
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

    // PATH fallback (M1.6). Mesen2 ships as a single GUI binary.
    // Engineer trial: user has `C:\tools\Mesen.exe`. Defensive list
    // covers the canonical name plus the bare POSIX form and an
    // explicit `Mesen2.exe` rename in case a user's binary follows
    // that convention.
    let names: &[&str] = if cfg!(windows) {
        &["Mesen.exe", "Mesen2.exe", "Mesen"]
    } else {
        &["Mesen", "Mesen2", "mesen", "mesen2"]
    };
    for name in names {
        searched.push((*name).to_string());
        if let Some(p) = which_on_path(name) {
            return ResolvedTool {
                name: "mesen2".to_string(),
                resolved: true,
                path: Some(p),
                version: None,
                source: ToolSource::Path,
                searched: Vec::new(),
            };
        }
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

    /// Sentinel-based PATH-fallback test: drop a fake `Mesen.exe`
    /// (or `Mesen` on POSIX) into a tempdir, prepend that dir to PATH,
    /// clear `SFCWC_MESEN2`, and confirm the resolver finds it. Using
    /// process-wide env mutation here is racy in parallel tests; this
    /// is the only test that touches PATH, so the parallel surface is
    /// tiny — restore PATH at the end regardless of failure.
    #[test]
    fn mesen2_path_fallback_finds_mesen_exe() {
        let dir = tempfile::tempdir().expect("tempdir");
        let exe_name = if cfg!(windows) { "Mesen.exe" } else { "Mesen" };
        let sentinel = dir.path().join(exe_name);
        std::fs::write(&sentinel, b"fake mesen").expect("write sentinel");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perm = std::fs::metadata(&sentinel).unwrap().permissions();
            perm.set_mode(0o755);
            std::fs::set_permissions(&sentinel, perm).unwrap();
        }

        let saved_path = std::env::var_os("PATH");
        let saved_env = std::env::var_os(MESEN2_ENV);

        // Build a PATH with our tempdir at the front.
        let new_path = {
            let mut paths: Vec<PathBuf> = vec![dir.path().to_path_buf()];
            if let Some(existing) = saved_path.as_ref() {
                paths.extend(std::env::split_paths(existing));
            }
            std::env::join_paths(paths).expect("join_paths")
        };
        // SAFETY: tests in this module are the only callers; the
        // restore-on-drop pattern below covers panic-paths via the
        // explicit set_var calls at the end.
        unsafe {
            std::env::set_var("PATH", &new_path);
            std::env::remove_var(MESEN2_ENV);
        }

        let result = resolve_mesen2();

        // Restore env regardless of assertion outcome.
        unsafe {
            match saved_path {
                Some(p) => std::env::set_var("PATH", p),
                None => std::env::remove_var("PATH"),
            }
            match saved_env {
                Some(p) => std::env::set_var(MESEN2_ENV, p),
                None => std::env::remove_var(MESEN2_ENV),
            }
        }

        assert!(result.resolved, "expected mesen2 to resolve via PATH");
        assert_eq!(result.source, ToolSource::Path);
        let resolved_path = result.path.expect("resolved path");
        // Compare canonical-ish: file_name should match.
        assert_eq!(
            resolved_path.file_name().and_then(|s| s.to_str()),
            Some(exe_name)
        );
    }

    #[test]
    fn mesen2_resolution_is_env_or_path_or_missing() {
        // With M1.6's PATH fallback, mesen2 may now resolve via PATH
        // when `Mesen.exe` lives in a directory on PATH (e.g. user's
        // `C:\tools` if that's listed). Either way, the call must
        // return a structurally valid ResolvedTool.
        let r = resolve_mesen2();
        if r.resolved {
            assert!(matches!(r.source, ToolSource::Env | ToolSource::Path));
            assert!(r.path.is_some());
            assert!(r.path.as_ref().unwrap().is_file());
        } else {
            assert_eq!(r.source, ToolSource::Missing);
            // searched list mentions both env and at least one PATH name.
            assert!(r.searched.iter().any(|s| s.starts_with("env:")));
            assert!(r
                .searched
                .iter()
                .any(|s| s.contains("Mesen") || s.contains("mesen")));
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
