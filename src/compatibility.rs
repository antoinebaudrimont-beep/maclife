use crate::identity::{self, normalized_app_identity, normalized_process_identity};
use crate::model::{Disposition, Inspection, WindowFacts};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompatibilityQuit {
    CloseSingleLogicalWindow,
    CloseSingleFamilyWindow,
}

impl CompatibilityQuit {
    pub fn name(self) -> &'static str {
        match self {
            Self::CloseSingleLogicalWindow => "native-wm-delete-single-logical-window",
            Self::CloseSingleFamilyWindow => "native-wm-delete-single-family-window",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompatibilityAdapter {
    pub name: &'static str,
    pub window_identities: &'static [&'static str],
    pub window_identity_prefixes: &'static [&'static str],
    pub process_identities: &'static [&'static str],
    pub process_executables: &'static [&'static str],
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
        window_identity_prefixes: &[],
        process_identities: &["thunderbird-bin"],
        process_executables: &[],
        family: Some("thunderbird"),
        quit: CompatibilityQuit::CloseSingleLogicalWindow,
        shared_process: false,
    },
    CompatibilityAdapter {
        name: "gimp",
        window_identities: GIMP_WINDOWS,
        window_identity_prefixes: &[],
        process_identities: &["gimp-3-0"],
        process_executables: &[],
        family: Some("gimp"),
        quit: CompatibilityQuit::CloseSingleLogicalWindow,
        shared_process: false,
    },
    CompatibilityAdapter {
        name: "libreoffice",
        window_identities: LIBREOFFICE_WINDOWS,
        window_identity_prefixes: &[],
        process_identities: &["soffice-bin"],
        process_executables: &[],
        family: Some("libreoffice"),
        quit: CompatibilityQuit::CloseSingleFamilyWindow,
        shared_process: true,
    },
    CompatibilityAdapter {
        name: "brave-origin",
        window_identities: &["brave-origin"],
        window_identity_prefixes: &[],
        process_identities: &["brave-browser"],
        process_executables: &["/opt/brave.com/brave-origin/brave"],
        family: Some("brave"),
        quit: CompatibilityQuit::CloseSingleLogicalWindow,
        shared_process: false,
    },
    CompatibilityAdapter {
        name: "brave-browser",
        window_identities: &["brave-browser"],
        window_identity_prefixes: &[],
        process_identities: &["brave-browser"],
        process_executables: &["/opt/brave.com/brave/brave"],
        family: Some("brave"),
        quit: CompatibilityQuit::CloseSingleLogicalWindow,
        shared_process: false,
    },
    CompatibilityAdapter {
        name: "brave-pwa-spotify",
        window_identities: SPOTIFY_WEB_WINDOWS,
        window_identity_prefixes: &[],
        process_identities: &["brave-browser"],
        process_executables: &[],
        family: Some("brave-pwa"),
        quit: CompatibilityQuit::CloseSingleLogicalWindow,
        shared_process: true,
    },
    CompatibilityAdapter {
        name: "brave-origin-pwa",
        window_identities: &[],
        window_identity_prefixes: &["brave-origin-crx-"],
        process_identities: &["brave-browser"],
        process_executables: &["/opt/brave.com/brave-origin/brave"],
        family: Some("brave-pwa"),
        quit: CompatibilityQuit::CloseSingleLogicalWindow,
        shared_process: true,
    },
    CompatibilityAdapter {
        name: "brave-browser-pwa",
        window_identities: &[],
        window_identity_prefixes: &["brave-browser-crx-"],
        process_identities: &["brave-browser"],
        process_executables: &["/opt/brave.com/brave/brave"],
        family: Some("brave-pwa"),
        quit: CompatibilityQuit::CloseSingleLogicalWindow,
        shared_process: true,
    },
];

fn accepts_window_identity(adapter: &CompatibilityAdapter, identity: &str) -> bool {
    adapter.window_identities.contains(&identity)
        || adapter
            .window_identity_prefixes
            .iter()
            .any(|prefix| identity.starts_with(prefix))
}

pub fn adapter_for(identity: &str) -> Option<&'static CompatibilityAdapter> {
    ADAPTERS
        .iter()
        .find(|adapter| accepts_window_identity(adapter, identity))
}

fn validate_window(
    adapter: &CompatibilityAdapter,
    window: &WindowFacts,
) -> Result<u32, String> {
    let window_identity = normalized_app_identity(window);
    if !accepts_window_identity(adapter, &window_identity) {
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
    if !adapter.process_executables.is_empty()
        && !process
            .executable
            .as_deref()
            .is_some_and(|executable| adapter.process_executables.contains(&executable))
    {
        return Err(format!(
            "adapter {} rejects process executable {}",
            adapter.name,
            process.executable.as_deref().unwrap_or("unknown")
        ));
    }
    Ok(pid)
}

pub fn validate_inspection(
    adapter: &CompatibilityAdapter,
    inspection: &Inspection,
) -> Result<u32, String> {
    if !accepts_window_identity(adapter, &inspection.app_identity) {
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
                && accepts_window_identity(adapter, &normalized_app_identity(window))
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
        adapter_for, family_windows, validate_inspection, CompatibilityQuit, ADAPTERS,
    };
    use crate::identity;
    use crate::model::{ProcessInfo, WindowFacts, WmClass};

    fn process_window_path(
        xid: u32,
        class: &str,
        pid: u32,
        process_name: &str,
        executable: &str,
    ) -> WindowFacts {
        let mut window = WindowFacts::test_window(xid, class);
        window.pid = Some(pid);
        window.pid_validated = true;
        window.pid_validation = format!("same-user local /proc/{pid}");
        window.process = Some(ProcessInfo {
            pid,
            uid: 1000,
            parent_pid: Some(1),
            name: process_name.to_string(),
            executable: Some(executable.to_string()),
            command_line: Some(executable.to_string()),
        });
        window
    }

    fn process_window(xid: u32, class: &str, pid: u32, executable: &str) -> WindowFacts {
        process_window_path(
            xid,
            class,
            pid,
            executable,
            &format!("/usr/bin/{executable}"),
        )
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
        assert_eq!(adapter.quit, CompatibilityQuit::CloseSingleFamilyWindow);
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
        assert_eq!(adapter.quit, CompatibilityQuit::CloseSingleLogicalWindow);
        assert!(adapter.shared_process);

        let spotify = process_window(10, "spotifyweb", 500, "brave-browser");
        let brave = process_window(20, "brave-browser", 500, "brave-browser");
        let inspection = identity::inspect(10, vec![spotify, brave]).expect("inspection");
        assert_eq!(validate_inspection(adapter, &inspection), Ok(500));
        assert_eq!(inspection.meaningful_windows.len(), 1);
    }

    #[test]
    fn brave_pwas_close_only_their_logical_windows_and_keep_variants_separate() {
        let mut origin_pwa = process_window_path(
            10,
            "Brave-origin",
            500,
            "brave",
            "/opt/brave.com/brave-origin/brave",
        );
        origin_pwa.wm_class = Some(WmClass {
            instance: "crx_mjoklplbddabcmpepnokjaffbmgbkkgg".to_string(),
            class: "Brave-origin".to_string(),
        });
        let origin_browser = process_window_path(
            20,
            "Brave-origin",
            500,
            "brave",
            "/opt/brave.com/brave-origin/brave",
        );
        let inspection = identity::inspect(10, vec![origin_pwa, origin_browser])
            .expect("origin PWA inspection");
        let adapter = adapter_for(&inspection.app_identity).expect("origin PWA adapter");
        assert_eq!(adapter.name, "brave-origin-pwa");
        assert_eq!(adapter.quit, CompatibilityQuit::CloseSingleLogicalWindow);
        assert!(adapter.shared_process);
        assert_eq!(validate_inspection(adapter, &inspection), Ok(500));
        assert_eq!(inspection.meaningful_windows.len(), 1);

        let mut wrong_variant = inspection.identity_window.clone();
        wrong_variant.process.as_mut().expect("process").executable =
            Some("/opt/brave.com/brave/brave".to_string());
        let wrong = identity::inspect(10, vec![wrong_variant]).expect("wrong variant");
        assert!(validate_inspection(adapter, &wrong).is_err());
    }

    #[test]
    fn brave_variants_accept_only_their_observed_executable_paths() {
        let origin = adapter_for("brave-origin").expect("origin adapter");
        let browser = adapter_for("brave-browser").expect("browser adapter");
        assert_eq!(origin.quit, CompatibilityQuit::CloseSingleLogicalWindow);
        assert_eq!(browser.quit, CompatibilityQuit::CloseSingleLogicalWindow);

        let origin_window = process_window_path(
            10,
            "Brave-origin",
            500,
            "brave",
            "/opt/brave.com/brave-origin/brave",
        );
        let browser_window = process_window_path(
            20,
            "Brave-browser",
            600,
            "brave",
            "/opt/brave.com/brave/brave",
        );
        let origin_inspection =
            identity::inspect(10, vec![origin_window.clone(), browser_window.clone()])
                .expect("origin inspection");
        let browser_inspection =
            identity::inspect(20, vec![origin_window, browser_window])
                .expect("browser inspection");

        assert_eq!(validate_inspection(origin, &origin_inspection), Ok(500));
        assert_eq!(validate_inspection(browser, &browser_inspection), Ok(600));
        assert!(validate_inspection(browser, &origin_inspection).is_err());
        assert!(validate_inspection(origin, &browser_inspection).is_err());
    }

    #[test]
    fn compatibility_adapters_never_select_process_termination() {
        for adapter in ADAPTERS {
            assert!(matches!(
                adapter.quit,
                CompatibilityQuit::CloseSingleLogicalWindow
                    | CompatibilityQuit::CloseSingleFamilyWindow
            ));
        }
    }

    #[test]
    fn brave_origin_refuses_browser_or_unexpected_executables() {
        let adapter = adapter_for("brave-origin").expect("origin adapter");
        for executable in [
            "/opt/brave.com/brave/brave",
            "/tmp/unexpected/brave",
            "/opt/brave.com/brave-origin/brave-helper",
        ] {
            let window = process_window_path(10, "Brave-origin", 500, "brave", executable);
            let inspection = identity::inspect(10, vec![window]).expect("inspection");
            assert!(validate_inspection(adapter, &inspection).is_err());
        }
    }

    #[test]
    fn ordinary_app_has_no_compatibility_adapter() {
        assert!(adapter_for("featherpad").is_none());
        assert!(adapter_for("xfdesktop").is_none());
    }
}
