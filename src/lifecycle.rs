use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LastWindowAction {
    Hide,
    NativeClose,
    Refuse,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuitMethod {
    StrawberryMpris,
    ThunarCli,
    ValidatedPidTerm,
    CloseEachWindow,
    Unsupported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApplicationRule {
    pub last_window: LastWindowAction,
    pub quit: QuitMethod,
}

pub fn application_rule(identity: &str) -> ApplicationRule {
    match identity {
        "strawberry" => ApplicationRule {
            last_window: LastWindowAction::Hide,
            quit: QuitMethod::StrawberryMpris,
        },
        "brave-origin" | "brave-browser" => ApplicationRule {
            last_window: LastWindowAction::Hide,
            quit: QuitMethod::ValidatedPidTerm,
        },
        "chatgpt" => ApplicationRule {
            last_window: LastWindowAction::NativeClose,
            quit: QuitMethod::ValidatedPidTerm,
        },
        "thunar" => ApplicationRule {
            last_window: LastWindowAction::NativeClose,
            quit: QuitMethod::ThunarCli,
        },
        "xfce4-terminal" => ApplicationRule {
            last_window: LastWindowAction::NativeClose,
            quit: QuitMethod::CloseEachWindow,
        },
        _ => ApplicationRule {
            last_window: LastWindowAction::Refuse,
            quit: QuitMethod::Unsupported,
        },
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusKind {
    Meaningful,
    Attached,
    Excluded,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CloseDecision {
    CloseFocused,
    HideLast,
    NativeCloseLast,
    Refuse(String),
}

pub fn close_decision(identity: &str, meaningful_count: usize, focus: FocusKind) -> CloseDecision {
    if focus == FocusKind::Excluded {
        return CloseDecision::Refuse("focused window is excluded from lifecycle control".into());
    }
    if focus == FocusKind::Attached {
        return CloseDecision::CloseFocused;
    }
    if meaningful_count == 0 {
        return CloseDecision::Refuse("focused application has no meaningful windows".into());
    }
    if meaningful_count > 1 {
        return CloseDecision::CloseFocused;
    }

    match application_rule(identity).last_window {
        LastWindowAction::Hide => CloseDecision::HideLast,
        LastWindowAction::NativeClose => CloseDecision::NativeCloseLast,
        LastWindowAction::Refuse => CloseDecision::Refuse(format!(
            "no proven last-window rule for {identity}"
        )),
    }
}

#[derive(Default, Debug)]
pub struct HiddenWindows {
    entries: HashMap<u32, String>,
}

impl HiddenWindows {
    pub fn remember(&mut self, xid: u32, identity: impl Into<String>) {
        self.entries.insert(xid, identity.into());
    }

    pub fn forget(&mut self, xid: u32) {
        self.entries.remove(&xid);
    }

    pub fn contains(&self, xid: u32) -> bool {
        self.entries.contains_key(&xid)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn retain_marked(&mut self, marked_xids: &[u32]) {
        self.entries.retain(|xid, _| marked_xids.contains(xid));
    }
}

#[cfg(test)]
mod tests {
    use super::{
        application_rule, close_decision, CloseDecision, FocusKind, HiddenWindows,
        LastWindowAction, QuitMethod,
    };

    #[test]
    fn closes_only_the_focused_window_when_multiple_exist() {
        assert_eq!(
            close_decision("brave-origin", 2, FocusKind::Meaningful),
            CloseDecision::CloseFocused
        );
    }

    #[test]
    fn hides_known_destructive_last_windows() {
        assert_eq!(
            close_decision("strawberry", 1, FocusKind::Meaningful),
            CloseDecision::HideLast
        );
        assert_eq!(
            close_decision("brave-origin", 1, FocusKind::Meaningful),
            CloseDecision::HideLast
        );
    }

    #[test]
    fn preserves_native_close_for_cooperative_apps() {
        assert_eq!(
            close_decision("chatgpt", 1, FocusKind::Meaningful),
            CloseDecision::NativeCloseLast
        );
        assert_eq!(
            close_decision("thunar", 1, FocusKind::Meaningful),
            CloseDecision::NativeCloseLast
        );
    }

    #[test]
    fn closes_attached_dialog_without_hiding_owner() {
        assert_eq!(
            close_decision("strawberry", 1, FocusKind::Attached),
            CloseDecision::CloseFocused
        );
    }

    #[test]
    fn unknown_last_window_fails_safely() {
        assert!(matches!(
            close_decision("unknown-editor", 1, FocusKind::Meaningful),
            CloseDecision::Refuse(_)
        ));
        assert_eq!(
            application_rule("unknown-editor").quit,
            QuitMethod::Unsupported
        );
    }

    #[test]
    fn brave_variants_have_separate_rules() {
        let origin = application_rule("brave-origin");
        let browser = application_rule("brave-browser");
        assert_eq!(origin.last_window, LastWindowAction::Hide);
        assert_eq!(browser.last_window, LastWindowAction::Hide);
        assert_ne!("brave-origin", "brave-browser");
    }

    #[test]
    fn hidden_bookkeeping_tracks_only_maclife_actions() {
        let mut hidden = HiddenWindows::default();
        hidden.remember(10, "strawberry");
        assert!(hidden.contains(10));
        assert!(!hidden.contains(20));
        hidden.retain_marked(&[10]);
        assert_eq!(hidden.len(), 1);
        hidden.forget(10);
        assert!(hidden.is_empty());
    }
}
