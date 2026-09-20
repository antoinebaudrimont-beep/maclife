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
            last_window: LastWindowAction::Hide,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActiveTarget {
    Focused,
    LogicalHidden { xid: u32, identity: String },
    Refuse,
}

#[derive(Default, Debug)]
pub struct HiddenWindows {
    entries: HashMap<u32, String>,
    logical_active: Option<u32>,
}

impl HiddenWindows {
    pub fn adopt(&mut self, xid: u32, identity: impl Into<String>) {
        self.entries.insert(xid, identity.into());
    }

    pub fn remember_as_logical(&mut self, xid: u32, identity: impl Into<String>) {
        self.entries.insert(xid, identity.into());
        self.logical_active = Some(xid);
    }

    pub fn forget(&mut self, xid: u32) {
        self.entries.remove(&xid);
        if self.logical_active == Some(xid) {
            self.logical_active = None;
        }
    }

    pub fn forget_identity(&mut self, identity: &str) {
        let removed_logical = self
            .logical_active
            .and_then(|xid| self.entries.get(&xid))
            .is_some_and(|stored| stored == identity);
        self.entries.retain(|_, stored| stored != identity);
        if removed_logical {
            self.logical_active = None;
        }
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
        if self
            .logical_active
            .is_some_and(|xid| !self.entries.contains_key(&xid))
        {
            self.logical_active = None;
        }
    }

    pub fn logical_active(&self) -> Option<(u32, &str)> {
        let xid = self.logical_active?;
        self.entries
            .get(&xid)
            .map(|identity| (xid, identity.as_str()))
    }
}

pub fn select_active_target(
    hidden: &mut HiddenWindows,
    focused: Option<(u32, FocusKind)>,
) -> ActiveTarget {
    match focused {
        Some((xid, FocusKind::Meaningful | FocusKind::Attached)) => {
            if hidden.logical_active.is_some_and(|logical| logical != xid) {
                hidden.logical_active = None;
            }
            ActiveTarget::Focused
        }
        Some((_, FocusKind::Excluded)) | None => hidden
            .logical_active()
            .map(|(xid, identity)| ActiveTarget::LogicalHidden {
                xid,
                identity: identity.to_string(),
            })
            .unwrap_or(ActiveTarget::Refuse),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        application_rule, close_decision, select_active_target, ActiveTarget, CloseDecision,
        FocusKind, HiddenWindows, LastWindowAction, QuitMethod,
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
        assert_eq!(
            close_decision("thunar", 1, FocusKind::Meaningful),
            CloseDecision::HideLast
        );
    }

    #[test]
    fn preserves_native_close_for_cooperative_apps() {
        assert_eq!(
            close_decision("chatgpt", 1, FocusKind::Meaningful),
            CloseDecision::NativeCloseLast
        );
    }

    #[test]
    fn thunar_closes_one_of_two_windows_but_hides_the_last() {
        assert_eq!(
            close_decision("thunar", 2, FocusKind::Meaningful),
            CloseDecision::CloseFocused
        );
        assert_eq!(
            close_decision("thunar", 1, FocusKind::Meaningful),
            CloseDecision::HideLast
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
        hidden.adopt(10, "strawberry");
        assert!(hidden.contains(10));
        assert!(!hidden.contains(20));
        assert_eq!(hidden.logical_active(), None);
        hidden.retain_marked(&[10]);
        assert_eq!(hidden.len(), 1);
        hidden.forget(10);
        assert!(hidden.is_empty());
    }

    #[test]
    fn desktop_uses_the_most_recent_maclife_hidden_app_for_quit() {
        let mut hidden = HiddenWindows::default();
        hidden.remember_as_logical(10, "strawberry");
        assert_eq!(
            select_active_target(&mut hidden, Some((99, FocusKind::Excluded))),
            ActiveTarget::LogicalHidden {
                xid: 10,
                identity: "strawberry".to_string(),
            }
        );
    }

    #[test]
    fn meaningful_focus_supersedes_a_hidden_logical_app() {
        let mut hidden = HiddenWindows::default();
        hidden.remember_as_logical(10, "strawberry");
        assert_eq!(
            select_active_target(&mut hidden, Some((20, FocusKind::Meaningful))),
            ActiveTarget::Focused
        );
        assert_eq!(hidden.logical_active(), None);
    }

    #[test]
    fn invalid_or_absent_hidden_context_refuses_desktop_quit() {
        let mut invalid = HiddenWindows::default();
        invalid.remember_as_logical(10, "strawberry");
        invalid.retain_marked(&[]);
        assert_eq!(
            select_active_target(&mut invalid, Some((99, FocusKind::Excluded))),
            ActiveTarget::Refuse
        );

        let mut absent = HiddenWindows::default();
        assert_eq!(
            select_active_target(&mut absent, Some((99, FocusKind::Excluded))),
            ActiveTarget::Refuse
        );
    }

    #[test]
    fn restore_and_quit_clear_stale_logical_context() {
        let mut restored = HiddenWindows::default();
        restored.remember_as_logical(10, "strawberry");
        restored.forget(10);
        assert_eq!(restored.logical_active(), None);

        let mut quit = HiddenWindows::default();
        quit.remember_as_logical(10, "strawberry");
        quit.adopt(20, "strawberry");
        quit.forget_identity("strawberry");
        assert!(quit.is_empty());
        assert_eq!(quit.logical_active(), None);
    }
}
