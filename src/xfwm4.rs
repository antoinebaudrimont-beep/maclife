//! Read-only detection of the installed xfwm4 title-bar hook.

use crate::ipc::LIFECYCLE_PROTOCOL_VERSION;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

const BINARY: &str = "/usr/bin/xfwm4";
const V2_MARKER: &[u8] = b"_MACLIFE_WINDOW_CLOSE_REQUEST";
const V1_MARKER: &[u8] = b"_MACLIFE_CLOSE_REQUEST";
// The physically validated 4.20.0-1+maclife2 amd64 package predates manifests.
const VERIFIED_BASELINE_VERSION: &str = "4.20.0-1+maclife2";
const VERIFIED_BASELINE_SHA256: &str =
    "24fa19a0aa82262e5d4210d0f0839c4f3642cbe4b62eed9c09dcc0c155dcedb2";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntegrationState {
    Compatible,
    Stock,
    ProtocolMismatch,
    Unverified,
}

impl IntegrationState {
    pub fn name(self) -> &'static str {
        match self {
            Self::Compatible => "compatible",
            Self::Stock => "stock",
            Self::ProtocolMismatch => "protocol-mismatch",
            Self::Unverified => "unverified",
        }
    }
}

#[derive(Debug)]
pub struct IntegrationProbe {
    pub state: IntegrationState,
    pub installed_version: String,
    pub reason: &'static str,
}

#[derive(Default)]
struct Manifest {
    version: String,
    protocol: String,
    binary_sha256: String,
}

fn manifest_path() -> Option<PathBuf> {
    let data_dir = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))?;
    Some(data_dir.join("maclife/xfwm4/current.manifest"))
}

fn read_manifest() -> Option<Manifest> {
    let contents = fs::read_to_string(manifest_path()?).ok()?;
    let mut manifest = Manifest::default();
    for line in contents.lines() {
        let (key, value) = line.split_once('=')?;
        match key {
            "version" => manifest.version = value.to_string(),
            "protocol" => manifest.protocol = value.to_string(),
            "binary_sha256" => manifest.binary_sha256 = value.to_string(),
            _ => return None,
        }
    }
    Some(manifest)
}

fn output(program: &str, arguments: &[&str]) -> Option<String> {
    let result = Command::new(program).args(arguments).output().ok()?;
    result.status.success().then(|| String::from_utf8(result.stdout).ok())?
}

fn installed_version() -> Option<String> {
    let version = output("dpkg-query", &["-W", "-f=${Version}", "xfwm4"])?;
    let version = version.trim();
    (!version.is_empty()).then(|| version.to_string())
}

fn binary_sha256() -> Option<String> {
    let output = output("sha256sum", &[BINARY])?;
    let hash = output.split_whitespace().next()?;
    (hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| hash.to_string())
}

fn contains(bytes: &[u8], marker: &[u8]) -> bool {
    bytes.windows(marker.len()).any(|window| window == marker)
}

fn classify(
    version: &str,
    v2_marker: bool,
    v1_marker: bool,
    sha256: Option<&str>,
    manifest: Option<&Manifest>,
) -> (IntegrationState, &'static str) {
    if v1_marker && !v2_marker {
        return (IntegrationState::ProtocolMismatch, "legacy protocol-v1 hook");
    }
    if !v2_marker {
        return (IntegrationState::Stock, "installed binary lacks the MacLife v2 hook");
    }
    let baseline = version == VERIFIED_BASELINE_VERSION
        && sha256 == Some(VERIFIED_BASELINE_SHA256);
    let recorded = manifest.is_some_and(|record| {
        record.version == version
            && record.protocol == LIFECYCLE_PROTOCOL_VERSION.to_string()
            && sha256 == Some(record.binary_sha256.as_str())
    });
    if baseline || recorded {
        return (IntegrationState::Compatible, "verified protocol-v2 package and binary");
    }
    if manifest.is_some_and(|record| record.version == version
        && record.protocol != LIFECYCLE_PROTOCOL_VERSION.to_string())
    {
        return (IntegrationState::ProtocolMismatch, "recorded hook protocol differs from MacLife");
    }
    (IntegrationState::Unverified, "hook marker exists but package/binary is unverified")
}

pub fn probe() -> IntegrationProbe {
    let Some(version) = installed_version() else {
        return IntegrationProbe {
            state: IntegrationState::Unverified,
            installed_version: "unavailable".to_string(),
            reason: "xfwm4 package version is unavailable",
        };
    };
    let Ok(binary) = fs::read(BINARY) else {
        return IntegrationProbe {
            state: IntegrationState::Unverified,
            installed_version: version,
            reason: "installed xfwm4 binary is unreadable",
        };
    };
    let (state, reason) = classify(
        &version,
        contains(&binary, V2_MARKER),
        contains(&binary, V1_MARKER),
        binary_sha256().as_deref(),
        read_manifest().as_ref(),
    );
    IntegrationProbe {
        state,
        installed_version: version,
        reason,
    }
}

#[cfg(test)]
mod tests {
    use super::{classify, IntegrationState, Manifest, VERIFIED_BASELINE_SHA256};

    #[test]
    fn identifies_validated_current_and_stock_packages() {
        assert_eq!(
            classify("4.20.0-1+maclife2", true, false, Some(VERIFIED_BASELINE_SHA256), None).0,
            IntegrationState::Compatible
        );
        assert_eq!(
            classify("4.20.0-1", false, false, None, None).0,
            IntegrationState::Stock
        );
    }

    #[test]
    fn requires_matching_manifest_for_future_builds() {
        let manifest = Manifest {
            version: "4.20.1-1+maclife1".to_string(),
            protocol: "2".to_string(),
            binary_sha256: "a".repeat(64),
        };
        assert_eq!(
            classify("4.20.1-1+maclife1", true, false, Some(&"a".repeat(64)), Some(&manifest)).0,
            IntegrationState::Compatible
        );
        assert_eq!(
            classify("4.20.1-1+maclife1", true, false, Some(&"b".repeat(64)), Some(&manifest)).0,
            IntegrationState::Unverified
        );
    }

    #[test]
    fn distinguishes_old_or_mismatched_protocol() {
        assert_eq!(
            classify("4.20.0-1+maclife1", false, true, None, None).0,
            IntegrationState::ProtocolMismatch
        );
        let manifest = Manifest {
            version: "4.20.1-1+maclife1".to_string(),
            protocol: "3".to_string(),
            binary_sha256: "a".repeat(64),
        };
        assert_eq!(
            classify("4.20.1-1+maclife1", true, false, Some(&"a".repeat(64)), Some(&manifest)).0,
            IntegrationState::ProtocolMismatch
        );
    }
}
