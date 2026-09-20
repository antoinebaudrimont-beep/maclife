use crate::model::{Disposition, Inspection, ProcessInfo, WindowDecision, WindowFacts};
use crate::DynError;
use std::collections::{HashMap, HashSet};
use std::path::Path;

const EXCLUDED_TYPES: &[&str] = &[
    "_NET_WM_WINDOW_TYPE_DESKTOP",
    "_NET_WM_WINDOW_TYPE_DOCK",
    "_NET_WM_WINDOW_TYPE_TOOLBAR",
    "_NET_WM_WINDOW_TYPE_MENU",
    "_NET_WM_WINDOW_TYPE_UTILITY",
    "_NET_WM_WINDOW_TYPE_SPLASH",
    "_NET_WM_WINDOW_TYPE_DROPDOWN_MENU",
    "_NET_WM_WINDOW_TYPE_POPUP_MENU",
    "_NET_WM_WINDOW_TYPE_TOOLTIP",
    "_NET_WM_WINDOW_TYPE_NOTIFICATION",
    "_NET_WM_WINDOW_TYPE_COMBO",
    "_NET_WM_WINDOW_TYPE_DND",
];

const ATTACHED_TYPES: &[&str] = &["_NET_WM_WINDOW_TYPE_DIALOG"];

const EXCLUDED_IDENTITIES: &[&str] = &[
    "conky",
    "xfce4-panel",
    "xfdesktop",
    "desktop",
    "panel",
];

fn slug(value: &str) -> String {
    let profile_free = value.split(" (").next().unwrap_or(value).trim();
    let without_desktop = profile_free
        .strip_suffix(".desktop")
        .unwrap_or(profile_free);
    let mut output = String::new();
    let mut separator = false;
    for character in without_desktop.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            output.push(character);
            separator = false;
        } else if !output.is_empty() && !separator {
            output.push('-');
            separator = true;
        }
    }
    while output.ends_with('-') {
        output.pop();
    }
    output
}

fn apply_alias(value: &str) -> &str {
    match value {
        "chat-gpt" | "chatgpt" => "chatgpt",
        "brave" | "brave-browser" | "brave-browser-stable" => "brave-browser",
        "brave-origin" | "brave-origin-stable" => "brave-origin",
        "org-xfce-thunar" | "thunar" => "thunar",
        "xfce-terminal" | "xfce4-terminal" => "xfce4-terminal",
        "strawberry" => "strawberry",
        other => other,
    }
}

pub fn normalized_app_identity(window: &WindowFacts) -> String {
    let class_value = window.wm_class.as_ref().and_then(|class| {
        (!class.class.trim().is_empty())
            .then_some(class.class.as_str())
            .or_else(|| (!class.instance.trim().is_empty()).then_some(class.instance.as_str()))
    });
    let process_value = window.process.as_ref().map(normalized_process_identity);
    let normalized = class_value
        .map(slug)
        .or(process_value)
        .unwrap_or_else(|| "unknown".to_string());
    apply_alias(&normalized).to_string()
}

pub fn normalized_process_identity(process: &ProcessInfo) -> String {
    let value = process
        .executable
        .as_deref()
        .and_then(|value| Path::new(value).file_name())
        .and_then(|value| value.to_str())
        .or_else(|| (!process.name.is_empty()).then_some(process.name.as_str()))
        .unwrap_or("unknown");
    apply_alias(&slug(value)).to_string()
}

pub fn generic_lifecycle_eligibility(inspection: &Inspection) -> Result<(), String> {
    if inspection.app_identity == "unknown" || inspection.app_identity.is_empty() {
        return Err("application identity is missing or unstable".to_string());
    }
    if disposition(&inspection.identity_window) != Disposition::Meaningful {
        return Err("identity window is not a meaningful normal window".to_string());
    }
    if inspection.meaningful_windows.is_empty() {
        return Err("application has no meaningful normal windows".to_string());
    }

    let has_class_identity = inspection
        .identity_window
        .wm_class
        .as_ref()
        .is_some_and(|class| {
            !class.class.trim().is_empty() || !class.instance.trim().is_empty()
        });
    let has_validated_process_identity = inspection.identity_window.pid_validated
        && inspection.identity_window.process.is_some();
    if !has_class_identity && !has_validated_process_identity {
        return Err("identity lacks WM_CLASS and validated process evidence".to_string());
    }

    Ok(())
}

pub fn disposition(window: &WindowFacts) -> Disposition {
    if window.override_redirect {
        return Disposition::Excluded("override-redirect window".to_string());
    }

    let identity = normalized_app_identity(window);
    if EXCLUDED_IDENTITIES.contains(&identity.as_str()) || identity.starts_with("conky-") {
        return Disposition::Excluded(format!("excluded desktop component: {identity}"));
    }

    if let Some(kind) = window
        .window_types
        .iter()
        .find(|kind| EXCLUDED_TYPES.contains(&kind.as_str()))
    {
        return Disposition::Excluded(format!("non-user window type: {kind}"));
    }
    if let Some(owner) = window.transient_for {
        return Disposition::Attached(format!("transient for 0x{owner:08x}"));
    }
    if let Some(kind) = window
        .window_types
        .iter()
        .find(|kind| ATTACHED_TYPES.contains(&kind.as_str()))
    {
        return Disposition::Attached(format!("attached {kind}"));
    }

    let normal_or_unspecified = window.window_types.is_empty()
        || window
            .window_types
            .iter()
            .any(|kind| kind == "_NET_WM_WINDOW_TYPE_NORMAL");
    if !normal_or_unspecified {
        return Disposition::Excluded("window type is not normal".to_string());
    }

    Disposition::Meaningful
}

fn transient_root(xid: u32, by_xid: &HashMap<u32, &WindowFacts>) -> u32 {
    let mut current = xid;
    let mut seen = HashSet::new();
    while seen.insert(current) {
        let Some(owner) = by_xid.get(&current).and_then(|window| window.transient_for) else {
            break;
        };
        if !by_xid.contains_key(&owner) {
            break;
        }
        current = owner;
    }
    current
}

fn grouping_rule(anchor: &WindowFacts, candidate: &WindowFacts) -> Option<String> {
    if anchor.xid == candidate.xid {
        return Some("identity window".to_string());
    }
    if let (Some(left), Some(right)) = (anchor.client_leader, candidate.client_leader) {
        if left == right {
            return Some(format!("shared WM_CLIENT_LEADER 0x{left:08x}"));
        }
        return None;
    }

    let anchor_identity = normalized_app_identity(anchor);
    let candidate_identity = normalized_app_identity(candidate);
    if anchor_identity != "unknown" && anchor_identity == candidate_identity {
        let rule = match anchor_identity.as_str() {
            "chatgpt"
            | "brave-browser"
            | "brave-origin"
            | "strawberry"
            | "xfce4-terminal"
            | "thunar" => {
                "normalized WM_CLASS with reference-app alias"
            }
            _ => "normalized WM_CLASS/process identity",
        };
        return Some(rule.to_string());
    }

    if anchor.pid_validated
        && candidate.pid_validated
        && anchor.pid.is_some()
        && anchor.pid == candidate.pid
    {
        return Some(format!(
            "shared validated _NET_WM_PID {}",
            anchor.pid.unwrap_or_default()
        ));
    }
    None
}

pub fn inspect(active_xid: u32, windows: Vec<WindowFacts>) -> Result<Inspection, DynError> {
    if active_xid == 0 {
        return Err("the X11 root window reports no active window".into());
    }

    let by_xid: HashMap<_, _> = windows.iter().map(|window| (window.xid, window)).collect();
    let focused = by_xid
        .get(&active_xid)
        .ok_or_else(|| format!("active window 0x{active_xid:08x} was not readable"))?;
    let identity_xid = transient_root(active_xid, &by_xid);
    let anchor = by_xid.get(&identity_xid).copied().unwrap_or(focused);

    let mut meaningful_windows = Vec::new();
    let mut decisions = Vec::new();
    for window in &windows {
        let window_disposition = disposition(window);
        let rule = grouping_rule(anchor, window);
        let belongs = rule.is_some();
        if belongs && window_disposition == Disposition::Meaningful {
            meaningful_windows.push(window.clone());
        }
        decisions.push(WindowDecision {
            xid: window.xid,
            disposition: window_disposition,
            belongs_to_focused_app: belongs,
            grouping_rule: rule,
        });
    }
    meaningful_windows.sort_by_key(|window| window.xid);
    decisions.sort_by_key(|decision| decision.xid);

    Ok(Inspection {
        focused_xid: active_xid,
        focused_window: (*focused).clone(),
        identity_window: anchor.clone(),
        app_identity: normalized_app_identity(anchor),
        meaningful_windows,
        decisions,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        disposition, generic_lifecycle_eligibility, inspect, normalized_app_identity,
        normalized_process_identity,
    };
    use crate::model::{Disposition, ProcessInfo, WindowFacts, WmClass};

    #[test]
    fn normalizes_reference_application_classes() {
        let cases = [
            ("Chatgpt", "chatgpt"),
            ("Brave-browser", "brave-browser"),
            ("Brave-origin", "brave-origin"),
            ("Xfce4-terminal", "xfce4-terminal"),
            ("Thunar", "thunar"),
            ("Strawberry", "strawberry"),
        ];
        for (class, expected) in cases {
            let window = WindowFacts::test_window(1, class);
            assert_eq!(normalized_app_identity(&window), expected);
        }
    }

    #[test]
    fn strips_profile_suffix_from_instance_fallback() {
        let mut window = WindowFacts::test_window(1, "");
        window.wm_class = Some(WmClass {
            instance: "brave-browser (/tmp/profile)".to_string(),
            class: String::new(),
        });
        assert_eq!(normalized_app_identity(&window), "brave-browser");
    }

    #[test]
    fn filters_override_redirect_and_desktop_components() {
        let mut popup = WindowFacts::test_window(1, "Brave-browser");
        popup.override_redirect = true;
        assert!(matches!(disposition(&popup), Disposition::Excluded(_)));

        let panel = WindowFacts::test_window(2, "xfce4-panel");
        assert!(matches!(disposition(&panel), Disposition::Excluded(_)));

        let conky = WindowFacts::test_window(3, "conky-semi");
        assert!(matches!(disposition(&conky), Disposition::Excluded(_)));
    }

    #[test]
    fn excludes_transient_menus_instead_of_treating_them_as_dialogs() {
        let mut menu = WindowFacts::test_window(4, "Brave-browser");
        menu.transient_for = Some(1);
        menu.window_types = vec!["_NET_WM_WINDOW_TYPE_POPUP_MENU".to_string()];
        assert!(matches!(disposition(&menu), Disposition::Excluded(_)));
    }

    #[test]
    fn groups_multiple_brave_windows_by_normalized_class() {
        let mut first = WindowFacts::test_window(10, "Brave-browser");
        first.pid = Some(100);
        first.pid_validated = true;
        let mut second = WindowFacts::test_window(20, "Brave-browser");
        second.pid = Some(200);
        second.pid_validated = true;

        let result = inspect(10, vec![first, second]).expect("inspection");
        assert_eq!(result.app_identity, "brave-browser");
        assert_eq!(result.meaningful_windows.len(), 2);
    }

    #[test]
    fn keeps_brave_origin_separate_from_brave_browser() {
        let origin = WindowFacts::test_window(10, "Brave-origin");
        let browser = WindowFacts::test_window(20, "Brave-browser");

        let result = inspect(10, vec![origin, browser]).expect("inspection");
        assert_eq!(result.app_identity, "brave-origin");
        assert_eq!(result.meaningful_windows.len(), 1);
        assert_eq!(result.meaningful_windows[0].xid, 10);
    }

    #[test]
    fn distinct_client_leaders_are_a_hard_group_boundary() {
        let mut first = WindowFacts::test_window(10, "Xfce4-terminal");
        first.client_leader = Some(100);
        let mut second = WindowFacts::test_window(20, "Xfce4-terminal");
        second.client_leader = Some(200);

        let result = inspect(10, vec![first, second]).expect("inspection");
        assert_eq!(result.meaningful_windows.len(), 1);
        assert_eq!(result.meaningful_windows[0].xid, 10);
    }

    #[test]
    fn associates_focused_transient_with_owner_without_counting_it() {
        let owner = WindowFacts::test_window(10, "Xfce4-terminal");
        let mut dialog = WindowFacts::test_window(11, "Xfce4-terminal");
        dialog.transient_for = Some(10);
        dialog.window_types = vec!["_NET_WM_WINDOW_TYPE_DIALOG".to_string()];

        let result = inspect(11, vec![owner, dialog]).expect("inspection");
        assert_eq!(result.identity_window.xid, 10);
        assert_eq!(result.meaningful_windows.len(), 1);
        assert_eq!(result.meaningful_windows[0].xid, 10);
    }

    #[test]
    fn falls_back_to_validated_pid_when_class_is_missing() {
        let mut first = WindowFacts::test_window(10, "");
        first.pid = Some(300);
        first.pid_validated = true;
        let mut second = WindowFacts::test_window(20, "");
        second.pid = Some(300);
        second.pid_validated = true;

        let result = inspect(10, vec![first, second]).expect("inspection");
        assert_eq!(result.meaningful_windows.len(), 2);
        assert!(result.decisions[1]
            .grouping_rule
            .as_deref()
            .unwrap_or_default()
            .contains("validated _NET_WM_PID"));
    }

    #[test]
    fn ordinary_unknown_by_name_window_is_generic_lifecycle_eligible() {
        let window = WindowFacts::test_window(10, "FeatherPad");
        let inspection = inspect(10, vec![window]).expect("inspection");
        assert_eq!(inspection.app_identity, "featherpad");
        assert!(generic_lifecycle_eligibility(&inspection).is_ok());
    }

    #[test]
    fn unstable_identity_is_not_generic_lifecycle_eligible() {
        let window = WindowFacts::test_window(10, "");
        let inspection = inspect(10, vec![window]).expect("inspection");
        assert!(generic_lifecycle_eligibility(&inspection).is_err());
    }

    #[test]
    fn process_identity_uses_the_same_normalization_as_windows() {
        let process = ProcessInfo {
            pid: 100,
            uid: 1000,
            parent_pid: Some(1),
            name: "featherpad".to_string(),
            executable: Some("/usr/bin/featherpad".to_string()),
            command_line: Some("featherpad".to_string()),
        };
        assert_eq!(normalized_process_identity(&process), "featherpad");
    }
}
