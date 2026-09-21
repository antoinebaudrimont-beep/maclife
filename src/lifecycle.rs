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
    GenericValidatedPidTerm,
    ValidatedPidTerm,
    CloseEachWindow,
    Unsupported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PolicyKind {
    Generic,
    DedicatedAdapter,
    NativeLifecycle,
    TerminalSafety,
    Refuse,
}

impl PolicyKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Generic => "generic",
            Self::DedicatedAdapter => "dedicated-adapter",
            Self::NativeLifecycle => "native-lifecycle",
            Self::TerminalSafety => "terminal-safety",
            Self::Refuse => "refuse",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApplicationPolicy {
    pub kind: PolicyKind,
    pub last_window: LastWindowAction,
    pub quit: QuitMethod,
}

pub fn application_policy(identity: &str, generic_eligible: bool) -> ApplicationPolicy {
    match identity {
        "strawberry" => ApplicationPolicy {
            kind: PolicyKind::DedicatedAdapter,
            last_window: LastWindowAction::Hide,
            quit: QuitMethod::StrawberryMpris,
        },
        "thunar" => ApplicationPolicy {
            kind: PolicyKind::DedicatedAdapter,
            last_window: LastWindowAction::Hide,
            quit: QuitMethod::ThunarCli,
        },
        "chatgpt" => ApplicationPolicy {
            kind: PolicyKind::NativeLifecycle,
            last_window: LastWindowAction::NativeClose,
            quit: QuitMethod::ValidatedPidTerm,
        },
        "xfce4-terminal" => ApplicationPolicy {
            kind: PolicyKind::TerminalSafety,
            last_window: LastWindowAction::NativeClose,
            quit: QuitMethod::CloseEachWindow,
        },
        // Chromium exposes its browser process through the top-level window's
        // validated PID. Keep this proven association as a narrow exception;
        // never infer it for arbitrary multi-process applications.
        "brave-origin" | "brave-browser" => ApplicationPolicy {
            kind: PolicyKind::Generic,
            last_window: LastWindowAction::Hide,
            quit: QuitMethod::ValidatedPidTerm,
        },
        _ if generic_eligible => ApplicationPolicy {
            kind: PolicyKind::Generic,
            last_window: LastWindowAction::Hide,
            quit: QuitMethod::GenericValidatedPidTerm,
        },
        _ => ApplicationPolicy {
            kind: PolicyKind::Refuse,
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

pub fn close_decision(
    policy: ApplicationPolicy,
    meaningful_count: usize,
    focus: FocusKind,
) -> CloseDecision {
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

    match policy.last_window {
        LastWindowAction::Hide => CloseDecision::HideLast,
        LastWindowAction::NativeClose => CloseDecision::NativeCloseLast,
        LastWindowAction::Refuse => CloseDecision::Refuse(
            "application is not eligible for generic lifecycle control".to_string(),
        ),
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
    pointer_intent_pending: bool,
}

impl HiddenWindows {
    pub fn adopt(&mut self, xid: u32, identity: impl Into<String>) {
        self.entries.insert(xid, identity.into());
    }

    pub fn remember_as_logical(&mut self, xid: u32, identity: impl Into<String>) {
        self.entries.insert(xid, identity.into());
        self.logical_active = Some(xid);
        self.pointer_intent_pending = false;
    }

    pub fn forget(&mut self, xid: u32) {
        self.entries.remove(&xid);
        if self.logical_active == Some(xid) {
            self.logical_active = None;
            self.pointer_intent_pending = false;
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
            self.pointer_intent_pending = false;
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
            self.pointer_intent_pending = false;
        }
    }

    pub fn logical_active(&self) -> Option<(u32, &str)> {
        let xid = self.logical_active?;
        self.entries
            .get(&xid)
            .map(|identity| (xid, identity.as_str()))
    }

    pub fn confirm_user_focus(&mut self, xid: u32) -> Option<String> {
        self.pointer_intent_pending = false;
        let logical_xid = self.logical_active?;
        if logical_xid == xid {
            return None;
        }
        let identity = self.entries.get(&logical_xid)?.clone();
        self.logical_active = None;
        Some(identity)
    }

    pub fn note_pointer_intent(&mut self) {
        if self.logical_active.is_some() {
            self.pointer_intent_pending = true;
        }
    }

    pub fn pointer_intent_pending(&self) -> bool {
        self.pointer_intent_pending
    }

    pub fn clear_pointer_intent(&mut self) {
        self.pointer_intent_pending = false;
    }
}

pub fn select_active_target(
    hidden: &HiddenWindows,
    focused: Option<(u32, FocusKind)>,
) -> ActiveTarget {
    if let Some((xid, identity)) = hidden.logical_active() {
        return ActiveTarget::LogicalHidden {
            xid,
            identity: identity.to_string(),
        };
    }
    match focused {
        Some((_, FocusKind::Meaningful | FocusKind::Attached)) => ActiveTarget::Focused,
        Some((_, FocusKind::Excluded)) | None => ActiveTarget::Refuse,
    }
}

#[cfg(test)]
mod tests {
    use crate::input::{KeyPhase, LifecycleChordTracker};

    use super::{
        application_policy, close_decision, select_active_target, ActiveTarget, ApplicationPolicy,
        CloseDecision, FocusKind, HiddenWindows, LastWindowAction, PolicyKind, QuitMethod,
    };

    fn eligible_policy(identity: &str) -> ApplicationPolicy {
        application_policy(identity, true)
    }

    fn observe_key(
        hidden: &mut HiddenWindows,
        tracker: &mut LifecycleChordTracker,
        focused_xid: u32,
        keycode: u32,
        phase: KeyPhase,
    ) {
        if tracker
            .observe(keycode, phase, 191, 192, 195)
            .confirms_user_intent()
        {
            hidden.confirm_user_focus(focused_xid);
        }
    }

    fn lifecycle_sequence(keycode: u32) -> [(u32, KeyPhase); 6] {
        [
            (105, KeyPhase::Press),
            (105, KeyPhase::Release),
            (keycode, KeyPhase::Press),
            (keycode, KeyPhase::Release),
            (105, KeyPhase::Press),
            (105, KeyPhase::Release),
        ]
    }

    #[test]
    fn closes_only_the_focused_window_when_multiple_exist() {
        assert_eq!(
            close_decision(eligible_policy("brave-origin"), 2, FocusKind::Meaningful),
            CloseDecision::CloseFocused
        );
    }

    #[test]
    fn hides_known_destructive_last_windows() {
        assert_eq!(
            close_decision(eligible_policy("strawberry"), 1, FocusKind::Meaningful),
            CloseDecision::HideLast
        );
        assert_eq!(
            close_decision(eligible_policy("brave-origin"), 1, FocusKind::Meaningful),
            CloseDecision::HideLast
        );
        assert_eq!(
            close_decision(eligible_policy("thunar"), 1, FocusKind::Meaningful),
            CloseDecision::HideLast
        );
    }

    #[test]
    fn preserves_native_close_for_cooperative_apps() {
        assert_eq!(
            close_decision(eligible_policy("chatgpt"), 1, FocusKind::Meaningful),
            CloseDecision::NativeCloseLast
        );
    }

    #[test]
    fn thunar_closes_one_of_two_windows_but_hides_the_last() {
        assert_eq!(
            close_decision(eligible_policy("thunar"), 2, FocusKind::Meaningful),
            CloseDecision::CloseFocused
        );
        assert_eq!(
            close_decision(eligible_policy("thunar"), 1, FocusKind::Meaningful),
            CloseDecision::HideLast
        );
    }

    #[test]
    fn closes_attached_dialog_without_hiding_owner() {
        assert_eq!(
            close_decision(eligible_policy("strawberry"), 1, FocusKind::Attached),
            CloseDecision::CloseFocused
        );
    }

    #[test]
    fn valid_unknown_by_name_application_uses_generic_policy() {
        let policy = application_policy("featherpad", true);
        assert_eq!(
            close_decision(policy, 1, FocusKind::Meaningful),
            CloseDecision::HideLast
        );
        assert_eq!(policy.kind, PolicyKind::Generic);
        assert_eq!(policy.quit, QuitMethod::GenericValidatedPidTerm);
    }

    #[test]
    fn ambiguous_application_refuses_generic_policy() {
        let policy = application_policy("unknown", false);
        assert!(matches!(
            close_decision(policy, 1, FocusKind::Meaningful),
            CloseDecision::Refuse(_)
        ));
        assert_eq!(policy.kind, PolicyKind::Refuse);
        assert_eq!(policy.quit, QuitMethod::Unsupported);
    }

    #[test]
    fn brave_variants_have_separate_rules() {
        let origin = application_policy("brave-origin", true);
        let browser = application_policy("brave-browser", true);
        assert_eq!(origin.last_window, LastWindowAction::Hide);
        assert_eq!(browser.last_window, LastWindowAction::Hide);
        assert_ne!("brave-origin", "brave-browser");
    }

    #[test]
    fn generic_two_window_and_attached_dialog_decisions_stay_window_scoped() {
        let policy = application_policy("featherpad", true);
        assert_eq!(
            close_decision(policy, 2, FocusKind::Meaningful),
            CloseDecision::CloseFocused
        );
        assert_eq!(
            close_decision(policy, 1, FocusKind::Attached),
            CloseDecision::CloseFocused
        );
    }

    #[test]
    fn terminal_and_native_lifecycle_exceptions_remain_narrow() {
        let terminal = application_policy("xfce4-terminal", true);
        assert_eq!(terminal.kind, PolicyKind::TerminalSafety);
        assert_eq!(terminal.last_window, LastWindowAction::NativeClose);
        assert_eq!(terminal.quit, QuitMethod::CloseEachWindow);

        let chatgpt = application_policy("chatgpt", true);
        assert_eq!(chatgpt.kind, PolicyKind::NativeLifecycle);
        assert_eq!(chatgpt.last_window, LastWindowAction::NativeClose);
        assert_eq!(chatgpt.quit, QuitMethod::ValidatedPidTerm);
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
            select_active_target(&hidden, Some((99, FocusKind::Excluded))),
            ActiveTarget::LogicalHidden {
                xid: 10,
                identity: "strawberry".to_string(),
            }
        );
    }

    #[test]
    fn automatic_meaningful_focus_preserves_a_hidden_logical_app() {
        let mut hidden = HiddenWindows::default();
        hidden.remember_as_logical(10, "strawberry");
        assert_eq!(
            select_active_target(&hidden, Some((20, FocusKind::Meaningful))),
            ActiveTarget::LogicalHidden {
                xid: 10,
                identity: "strawberry".to_string(),
            }
        );
        assert_eq!(hidden.logical_active(), Some((10, "strawberry")));
    }

    #[test]
    fn confirmed_brave_or_terminal_interaction_supersedes_hidden_strawberry() {
        for focused_xid in [20, 30] {
            let mut hidden = HiddenWindows::default();
            hidden.remember_as_logical(10, "strawberry");
            assert_eq!(
                hidden.confirm_user_focus(focused_xid),
                Some("strawberry".to_string())
            );
            assert_eq!(
                select_active_target(&hidden, Some((focused_xid, FocusKind::Meaningful))),
                ActiveTarget::Focused
            );
        }
    }

    #[test]
    fn pointer_intent_waits_for_focus_confirmation() {
        let mut hidden = HiddenWindows::default();
        hidden.remember_as_logical(10, "strawberry");
        hidden.note_pointer_intent();
        assert!(hidden.pointer_intent_pending());
        assert_eq!(
            select_active_target(&hidden, Some((20, FocusKind::Meaningful))),
            ActiveTarget::LogicalHidden {
                xid: 10,
                identity: "strawberry".to_string(),
            }
        );
        hidden.confirm_user_focus(20);
        assert!(!hidden.pointer_intent_pending());
        assert_eq!(hidden.logical_active(), None);
    }

    #[test]
    fn physical_quit_chord_does_not_promote_auto_focused_terminal() {
        let mut hidden = HiddenWindows::default();
        let mut tracker = LifecycleChordTracker::default();
        hidden.remember_as_logical(10, "strawberry");
        for (keycode, phase) in lifecycle_sequence(192) {
            observe_key(&mut hidden, &mut tracker, 20, keycode, phase);
        }
        assert_eq!(
            select_active_target(&hidden, Some((20, FocusKind::Meaningful))),
            ActiveTarget::LogicalHidden {
                xid: 10,
                identity: "strawberry".to_string(),
            }
        );
    }

    #[test]
    fn ordinary_key_promotes_auto_focused_terminal() {
        let mut hidden = HiddenWindows::default();
        let mut tracker = LifecycleChordTracker::default();
        hidden.remember_as_logical(10, "strawberry");
        observe_key(&mut hidden, &mut tracker, 20, 38, KeyPhase::Press);
        assert_eq!(
            select_active_target(&hidden, Some((20, FocusKind::Meaningful))),
            ActiveTarget::Focused
        );
    }

    #[test]
    fn physical_close_chord_does_not_promote_another_application() {
        let mut hidden = HiddenWindows::default();
        let mut tracker = LifecycleChordTracker::default();
        hidden.remember_as_logical(10, "strawberry");
        for (keycode, phase) in lifecycle_sequence(191) {
            observe_key(&mut hidden, &mut tracker, 20, keycode, phase);
        }
        assert_eq!(hidden.logical_active(), Some((10, "strawberry")));
    }

    #[test]
    fn ordinary_input_without_a_hidden_target_keeps_focused_policy() {
        let mut hidden = HiddenWindows::default();
        let mut tracker = LifecycleChordTracker::default();
        observe_key(&mut hidden, &mut tracker, 20, 38, KeyPhase::Press);
        assert_eq!(
            select_active_target(&hidden, Some((20, FocusKind::Meaningful))),
            ActiveTarget::Focused
        );
    }

    #[test]
    fn invalid_or_absent_hidden_context_refuses_desktop_quit() {
        let mut invalid = HiddenWindows::default();
        invalid.remember_as_logical(10, "strawberry");
        invalid.retain_marked(&[]);
        assert_eq!(
            select_active_target(&invalid, Some((99, FocusKind::Excluded))),
            ActiveTarget::Refuse
        );

        let absent = HiddenWindows::default();
        assert_eq!(
            select_active_target(&absent, Some((99, FocusKind::Excluded))),
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
