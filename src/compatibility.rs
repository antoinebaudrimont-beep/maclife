use crate::identity::{self, normalized_app_identity, normalized_process_identity};
use crate::model::{Disposition, Inspection, WindowFacts};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompatibilityQuit {
    CloseLogicalWindows,
    CloseFamilyWindows,
}

impl CompatibilityQuit {
    pub fn name(self) -> &'static str {
        match self {
            Self::CloseLogicalWindows => "wm-delete-logical-app-windows",
            Self::CloseFamilyWindows => "wm-delete-application-family-windows",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompatibilityAdapter {
    pub name: &'static str,
    pub window_identities: &'static [&'static str],
    pub process_identities: &'static [&'static str],
    pub family: Option<&'static str>,
    pub quit: CompatibilityQuit,
    pub shared_process: bool,
}

const THUNDERBIRD_WINDOWS: &[&str] = &["thunderbird-default"];
const GIMP_WINDOWS: &[&str] = &["gimp"];
const LIBREOFFICE_WINDOWS: &[&str] = &[
    "libreoffice",
    "libreoffice-base",
    "libreoffice-calc",
    "libreoffice-draw",
    "libreoffice-impress",
    "libreoffice-math",
    "libreoffice-startcenter",
    "libreoffice-writer",
];
const SPOTIFY_WEB_WINDOWS: &[&str] = &["spotifyweb"];

const ADAPTERS: &[CompatibilityAdapter] = &[
    CompatibilityAdapter {
        name: "thunderbird",
        window_identities: THUNDERBIRD_WINDOWS,
        process_identities: &["thunderbird-bin"],
        family: Some("thunderbird"),
        quit: CompatibilityQuit::CloseLogicalWindows,
        shared_process: false,
    },
    CompatibilityAdapter {
        name: "gimp",
        window_identities: GIMP_WINDOWS,
        process_identities: &["gimp-3-0"],
        family: Some("gimp"),
        quit: CompatibilityQuit::CloseLogicalWindows,
        shared_process: false,
    },
    CompatibilityAdapter {
        name: "libreoffice",
        window_identities: LIBREOFFICE_WINDOWS,
        process_identities: &["soffice-bin"],
        family: Some("libreoffice"),
        quit: CompatibilityQuit::CloseFamilyWindows,
        shared_process: true,
    },
    CompatibilityAdapter {
        name: "brave-pwa-spotify",
        window_identities: SPOTIFY_WEB_WINDOWS,
        process_identities: &["brave-browser"],
        family: Some("brave-pwa"),
        quit: CompatibilityQuit::CloseLogicalWindows,
        shared_process: true,
    },
];

pub fn adapter_for(identity: &str) -> Option<&'static CompatibilityAdapter> {
    ADAPTERS
        .iter()
        .find(|adapter| adapter.window_identities.contains(&identity))
}

fn validate_window(
    adapter: &CompatibilityAdapter,
    window: &WindowFacts,
) -> Result<u32, String> {
    let window_identity = normalized_app_identity(window);
    if !adapter.window_identities.contains(&window_identity.as_str()) {
        return Err(format!(
            "adapter {} does not accept window identity {window_identity}",
            adapter.name
        ));
    }
    if !window.pid_validated {
        return Err(format!(
            "adapter {} requires a validated local same-user PID ({})",
            adapter.name, window.pid_validation
        ));
    }
    let pid = window
        .pid
        .ok_or_else(|| format!("adapter {} found no window PID", adapter.name))?;
    let process = window
        .process
        .as_ref()
        .ok_or_else(|| format!("adapter {} found no process metadata", adapter.name))?;
    if process.pid != pid {
        return Err(format!(
            "adapter {} found different window and process PIDs",
            adapter.name
        ));
    }
    let process_identity = normalized_process_identity(process);
    if !adapter
        .process_identities
        .contains(&process_identity.as_str())
    {
        return Err(format!(
            "adapter {} rejects process identity {process_identity}",
            adapter.name
        ));
    }
    Ok(pid)
}

pub fn validate_inspection(
    adapter: &CompatibilityAdapter,
    inspection: &Inspection,
) -> Result<u32, String> {
    if !adapter
        .window_identities
        .contains(&inspection.app_identity.as_str())
    {
        return Err(format!(
            "adapter {} does not apply to {}",
            adapter.name, inspection.app_identity
        ));
    }
    let pid = validate_window(adapter, &inspection.identity_window)?;
    if inspection.meaningful_windows.is_empty() {
        return Err(format!(
            "adapter {} found no meaningful application windows",
            adapter.name
        ));
    }
    for window in &inspection.meaningful_windows {
        if validate_window(adapter, window)? != pid {
            return Err(format!(
                "adapter {} found multiple application PIDs",
                adapter.name
            ));
        }
    }
    Ok(pid)
}

pub fn family_windows(
    adapter: &CompatibilityAdapter,
    windows: &[WindowFacts],
) -> Result<Vec<WindowFacts>, String> {
    let mut matches: Vec<_> = windows
        .iter()
        .filter(|window| {
            identity::disposition(window) == Disposition::Meaningful
                && adapter
                    .window_identities
                    .contains(&normalized_app_identity(window).as_str())
        })
        .cloned()
        .collect();
    matches.sort_by_key(|window| window.xid);
    let first = matches.first().ok_or_else(|| {
        format!(
            "adapter {} found no meaningful family windows",
            adapter.name
        )
    })?;
    let pid = validate_window(adapter, first)?;
    for window in &matches {
        if validate_window(adapter, window)? != pid {
            return Err(format!(
                "adapter {} family spans multiple validated PIDs",
                adapter.name
            ));
        }
    }
    Ok(matches)
}

pub fn resolution_summary(
    adapter: &CompatibilityAdapter,
    inspection: &Inspection,
) -> String {
    match validate_inspection(adapter, inspection) {
        Ok(pid) => {
            let process_identity = inspection
                .identity_window
                .process
                .as_ref()
                .map(normalized_process_identity)
                .unwrap_or_else(|| "unknown".to_string());
            let sharing = if adapter.shared_process {
                "; process may be shared, never signal it"
            } else {
                ""
            };
            format!(
                "compatibility alias {} -> {} on validated PID {pid}; quit={}{}",
                inspection.app_identity,
                process_identity,
                adapter.quit.name(),
                sharing
            )
        }
        Err(reason) => format!("refused: {reason}"),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        adapter_for, family_windows, validate_inspection, CompatibilityQuit,
    };
    use crate::identity;
    use crate::model::{ProcessInfo, WindowFacts};

    fn process_window(xid: u32, class: &str, pid: u32, executable: &str) -> WindowFacts {
        let mut window = WindowFacts::test_window(xid, class);
        window.pid = Some(pid);
        window.pid_validated = true;
        window.pid_validation = format!("same-user local /proc/{pid}");
        window.process = Some(ProcessInfo {
            pid,
            uid: 1000,
            parent_pid: Some(1),
            name: executable.to_string(),
            executable: Some(format!("/usr/bin/{executable}")),
            command_line: Some(executable.to_string()),
        });
        window
    }

    #[test]
    fn thunderbird_accepts_only_its_exact_process_alias() {
        let adapter = adapter_for("thunderbird-default").expect("adapter");
        let valid = process_window(10, "thunderbird-default", 100, "thunderbird-bin");
        let inspection = identity::inspect(10, vec![valid]).expect("inspection");
        assert_eq!(validate_inspection(adapter, &inspection), Ok(100));

        let invalid = process_window(10, "thunderbird-default", 100, "unrelated-process");
        let inspection = identity::inspect(10, vec![invalid]).expect("inspection");
        assert!(validate_inspection(adapter, &inspection).is_err());
    }

    #[test]
    fn gimp_accepts_current_gimp_three_process_and_rejects_pid_ambiguity() {
        let adapter = adapter_for("gimp").expect("adapter");
        let first = process_window(10, "gimp", 100, "gimp-3.0");
        let second = process_window(20, "gimp", 100, "gimp-3.0");
        let inspection = identity::inspect(10, vec![first, second]).expect("inspection");
        assert_eq!(validate_inspection(adapter, &inspection), Ok(100));

        let first = process_window(10, "gimp", 100, "gimp-3.0");
        let second = process_window(20, "gimp", 200, "gimp-3.0");
        let inspection = identity::inspect(10, vec![first, second]).expect("inspection");
        assert!(validate_inspection(adapter, &inspection).is_err());
    }

    #[test]
    fn libreoffice_family_is_explicit_and_never_absorbs_unrelated_windows() {
        let adapter = adapter_for("libreoffice-writer").expect("adapter");
        assert_eq!(adapter.quit, CompatibilityQuit::CloseFamilyWindows);
        let writer = process_window(10, "libreoffice-writer", 300, "soffice.bin");
        let calc = process_window(20, "libreoffice-calc", 300, "soffice.bin");
        let unrelated = process_window(30, "FeatherPad", 400, "featherpad");
        let windows = family_windows(adapter, &[writer, calc, unrelated]).expect("family");
        assert_eq!(windows.iter().map(|window| window.xid).collect::<Vec<_>>(), vec![10, 20]);
    }

    #[test]
    fn libreoffice_family_refuses_multiple_suite_processes() {
        let adapter = adapter_for("libreoffice-calc").expect("adapter");
        let writer = process_window(10, "libreoffice-writer", 300, "soffice.bin");
        let calc = process_window(20, "libreoffice-calc", 301, "soffice.bin");
        assert!(family_windows(adapter, &[writer, calc]).is_err());
    }

    #[test]
    fn spotify_pwa_can_only_close_its_logical_windows() {
        let adapter = adapter_for("spotifyweb").expect("adapter");
        assert_eq!(adapter.quit, CompatibilityQuit::CloseLogicalWindows);
        assert!(adapter.shared_process);

        let spotify = process_window(10, "spotifyweb", 500, "brave-browser");
        let brave = process_window(20, "brave-browser", 500, "brave-browser");
        let inspection = identity::inspect(10, vec![spotify, brave]).expect("inspection");
        assert_eq!(validate_inspection(adapter, &inspection), Ok(500));
        assert_eq!(inspection.meaningful_windows.len(), 1);
    }

    #[test]
    fn ordinary_app_has_no_compatibility_adapter() {
        assert!(adapter_for("featherpad").is_none());
        assert!(adapter_for("xfdesktop").is_none());
    }
}
