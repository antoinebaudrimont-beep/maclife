use crate::compatibility::{self, CompatibilityAdapter, CompatibilityQuit};
use crate::control;
use crate::documents::{
    self, AtspiDocumentProvider, CloseDocumentMethod, CloseTopLevelMethod,
    DocumentCloseDecision, InternalDocumentProvider,
};
use crate::identity::{self, normalized_app_identity};
use crate::input::{IntentDevices, KeyPhase, LifecycleChordTracker};
use crate::ipc::{LifecycleManager, WindowCloseRequest, LIFECYCLE_PROTOCOL_VERSION};
use crate::lifecycle::{
    self, ActiveTarget, ApplicationPolicy, CloseDecision, FocusKind, HiddenWindows,
    LifecycleIntent, PolicyKind, QuitMethod,
};
use crate::model::{Disposition, Inspection, Snapshot, WindowFacts};
use crate::signals::ShutdownSignals;
use crate::singleton::{AcquireResult, SingletonGuard};
use crate::{report, x11, DynError};
use std::io::ErrorKind;
use std::os::fd::AsRawFd;
use std::process::Command;
use std::thread;
use std::time::Duration;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    ChangeWindowAttributesAux, ConnectionExt, EventMask, GrabMode, ModMask,
};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;

pub const DEFAULT_CLOSE_KEYCODE: u8 = 191;
pub const DEFAULT_QUIT_KEYCODE: u8 = 192;
pub const DEFAULT_CLOSE_WINDOW_KEYCODE: u8 = 195;

#[derive(Clone, Copy, Debug)]
pub struct RunOptions {
    pub close_keycode: u8,
    pub quit_keycode: u8,
    pub close_window_keycode: u8,
    pub dry_run: bool,
    pub verbose: bool,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            close_keycode: DEFAULT_CLOSE_KEYCODE,
            quit_keycode: DEFAULT_QUIT_KEYCODE,
            close_window_keycode: DEFAULT_CLOSE_WINDOW_KEYCODE,
            dry_run: false,
            verbose: false,
        }
    }
}

fn focused_inspection() -> Result<(Snapshot, Inspection), DynError> {
    let snapshot = x11::collect_snapshot()?;
    let inspection = identity::inspect(snapshot.active_window, snapshot.windows.clone())?;
    Ok((snapshot, inspection))
}

fn focus_kind(inspection: &Inspection) -> FocusKind {
    match identity::disposition(&inspection.focused_window) {
        Disposition::Meaningful => FocusKind::Meaningful,
        Disposition::Attached(_) => FocusKind::Attached,
        Disposition::Excluded(_) => FocusKind::Excluded,
    }
}

fn policy_for(inspection: &Inspection) -> (ApplicationPolicy, Option<String>) {
    let eligibility = identity::generic_lifecycle_eligibility(inspection);
    let policy = lifecycle::application_policy(&inspection.app_identity, eligibility.is_ok());
    (policy, eligibility.err())
}

fn action_name(decision: &CloseDecision) -> &'static str {
    match decision {
        CloseDecision::CloseFocused => "wm-delete-focused",
        CloseDecision::HideLast => "iconify-last-window",
        CloseDecision::NativeCloseLast => "native-close-last-window",
        CloseDecision::Refuse(_) => "refuse",
    }
}

fn application_window_close_decision(
    inspection: &Inspection,
) -> (ApplicationPolicy, CloseDecision, Option<String>) {
    let focus = focus_kind(inspection);
    let (policy, ineligible_reason) = policy_for(inspection);
    if documents::is_thunderbird_compose_window(inspection) {
        // Compose is a document-owning top-level window. Its native close path
        // must retain Thunderbird's Save / Discard / Cancel authority even if
        // it happens to be the last meaningful Thunderbird window.
        return (policy, CloseDecision::CloseFocused, ineligible_reason);
    }
    let decision = lifecycle::close_decision(
        policy,
        inspection.meaningful_windows.len(),
        focus,
    );
    (policy, decision, ineligible_reason)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CloseRoute {
    ThunderbirdCompose,
    InternalDocuments,
    ApplicationWindows,
}

fn close_route(inspection: &Inspection, focus: FocusKind) -> CloseRoute {
    if focus == FocusKind::Meaningful && documents::is_thunderbird_compose_window(inspection) {
        CloseRoute::ThunderbirdCompose
    } else if focus == FocusKind::Meaningful
        && documents::adapter_for(&inspection.app_identity).is_some()
    {
        CloseRoute::InternalDocuments
    } else {
        CloseRoute::ApplicationWindows
    }
}

fn is_actually_hidden(window: &WindowFacts) -> bool {
    !window.mapped
        || window
            .states
            .iter()
            .any(|state| state == "_NET_WM_STATE_HIDDEN")
}

fn is_adoptable_hidden(window: &WindowFacts) -> bool {
    window.maclife_hidden
        && is_actually_hidden(window)
        && identity::disposition(window) == Disposition::Meaningful
        && normalized_app_identity(window) != "unknown"
        && (window.wm_class.is_some()
            || (window.pid_validated && window.process.is_some()))
}

fn reconcile_hidden(hidden: &mut HiddenWindows, snapshot: &Snapshot) {
    for window in snapshot
        .windows
        .iter()
        .filter(|window| window.maclife_hidden && !is_actually_hidden(window))
    {
        if let Err(error) = control::clear_hidden_marker(window.xid) {
            eprintln!(
                "MacLife: could not clear stale hidden marker on 0x{:08x}: {error}",
                window.xid
            );
        }
    }
    for window in snapshot
        .windows
        .iter()
        .filter(|window| {
            window.maclife_hidden
                && is_actually_hidden(window)
                && !is_adoptable_hidden(window)
        })
    {
        if let Err(error) = control::clear_hidden_marker(window.xid) {
            eprintln!(
                "MacLife: could not clear invalid hidden marker on 0x{:08x}: {error}",
                window.xid
            );
        }
    }
    let marked: Vec<_> = snapshot
        .windows
        .iter()
        .filter(|window| is_adoptable_hidden(window))
        .map(|window| window.xid)
        .collect();
    hidden.retain_marked(&marked);
    for window in snapshot
        .windows
        .iter()
        .filter(|window| is_adoptable_hidden(window))
    {
        hidden.adopt(window.xid, normalized_app_identity(window));
    }
}

fn apply_confirmed_interaction(
    hidden: &mut HiddenWindows,
    inspection: &Inspection,
    source: &str,
    verbose: bool,
) {
    let previous = hidden.confirm_user_focus(inspection.identity_window.xid);
    if verbose {
        if let Some(previous) = previous {
            println!(
                "user-intent {} source={} logical_active {} -> {}",
                inspection.app_identity, source, previous, inspection.app_identity
            );
        }
    }
}

fn observe_focus(hidden: &mut HiddenWindows, snapshot: &Snapshot, verbose: bool) {
    reconcile_hidden(hidden, snapshot);
    if let Ok(inspection) = identity::inspect(snapshot.active_window, snapshot.windows.clone()) {
        let focus = focus_kind(&inspection);
        if matches!(focus, FocusKind::Meaningful | FocusKind::Attached) {
            if hidden.pointer_intent_pending() {
                apply_confirmed_interaction(hidden, &inspection, "button", verbose);
            } else if let Some((logical_xid, logical_identity)) = hidden.logical_active() {
                if logical_xid != inspection.identity_window.xid && verbose {
                    println!(
                        "focus-change {} source=wm-automatic logical_active={} preserved",
                        inspection.app_identity, logical_identity
                    );
                }
            }
        }
    }
}

fn confirm_current_interaction(
    hidden: &mut HiddenWindows,
    source: &str,
    finalize_pointer: bool,
    verbose: bool,
) -> Result<(), DynError> {
    if hidden.logical_active().is_none() {
        hidden.clear_pointer_intent();
        return Ok(());
    }
    let snapshot = x11::collect_snapshot()?;
    reconcile_hidden(hidden, &snapshot);
    let inspection = identity::inspect(snapshot.active_window, snapshot.windows.clone()).ok();
    if let Some(inspection) = inspection.filter(|value| {
        matches!(focus_kind(value), FocusKind::Meaningful | FocusKind::Attached)
    }) {
        apply_confirmed_interaction(hidden, &inspection, source, verbose);
    } else {
        if finalize_pointer {
            hidden.clear_pointer_intent();
        }
        if verbose {
            if let Some((_, logical_identity)) = hidden.logical_active() {
                println!(
                    "user-intent source={} focus=excluded logical_active={} preserved",
                    source, logical_identity
                );
            }
        }
    }
    Ok(())
}

fn observe_keyboard_intent(
    hidden: &mut HiddenWindows,
    tracker: &mut LifecycleChordTracker,
    keycode: u32,
    phase: KeyPhase,
    sourceid: u16,
    options: RunOptions,
) {
    let intent = tracker.observe(
        keycode,
        phase,
        options.close_keycode,
        options.quit_keycode,
        options.close_window_keycode,
    );
    let log_input = !matches!(
        intent,
        crate::input::KeyIntent::User | crate::input::KeyIntent::Release
    ) || hidden.logical_active().is_some();
    if options.verbose && log_input {
        println!(
            "input device=XWayKeyz sourceid={} keycode={} event={} classification={}",
            sourceid,
            keycode,
            phase.label(),
            intent.label()
        );
    }
    if intent.confirms_user_intent() {
        if let Err(error) =
            confirm_current_interaction(hidden, "keyboard", false, options.verbose)
        {
            eprintln!("MacLife user-intent observation failed: {error}");
        }
    } else if options.verbose {
        if let Some((_, logical_identity)) = hidden.logical_active() {
            println!(
                "user-intent source={} keycode={} logical_active={} preserved",
                intent.label(),
                keycode,
                logical_identity
            );
        }
    }
}

fn inspect_close_target(snapshot: &Snapshot, target_xid: u32) -> Result<Inspection, DynError> {
    if !snapshot.windows.iter().any(|window| window.xid == target_xid) {
        return Err(format!(
            "requested close target 0x{target_xid:08x} is stale, destroyed, or unmanaged"
        )
        .into());
    }
    identity::inspect(target_xid, snapshot.windows.clone())
}

fn dispatch_application_window_close(
    options: RunOptions,
    hidden: &mut HiddenWindows,
    inspection: &Inspection,
    source: &str,
) -> Result<(), DynError> {
    let (policy, decision, ineligible_reason) = application_window_close_decision(inspection);
    let adapter = compatibility::adapter_for(&inspection.app_identity)
        .map(|adapter| adapter.name)
        .unwrap_or("none");
    println!(
        "{} target={} meaningful_windows={} policy={} adapter={} action={} xid=0x{:08x}",
        source,
        inspection.app_identity,
        inspection.meaningful_windows.len(),
        policy.kind.name(),
        adapter,
        action_name(&decision),
        inspection.focused_xid
    );
    if options.verbose {
        print!("{}", report::render(inspection, true));
    }
    if options.dry_run {
        return Ok(());
    }

    match decision {
        CloseDecision::CloseFocused | CloseDecision::NativeCloseLast => {
            control::request_close(inspection.focused_xid)
        }
        CloseDecision::HideLast => hide_and_remember(hidden, inspection),
        CloseDecision::Refuse(reason) => Err(ineligible_reason.unwrap_or(reason).into()),
    }
}

fn handle_document_close_inspection(
    options: RunOptions,
    hidden: &mut HiddenWindows,
    document_provider: &mut AtspiDocumentProvider,
    snapshot: Snapshot,
    inspection: Inspection,
    source: &str,
) -> Result<(), DynError> {
    reconcile_hidden(hidden, &snapshot);
    document_provider.retain_windows(&snapshot.windows);
    let focus = focus_kind(&inspection);
    let route = close_route(&inspection, focus);
    if route == CloseRoute::ThunderbirdCompose {
        println!(
            "{} target={} policy=thunderbird-compose action=native-wm-delete xid=0x{:08x}",
            source, inspection.app_identity, inspection.focused_xid
        );
        if options.verbose {
            print!("{}", report::render(&inspection, true));
        }
        if options.dry_run {
            return Ok(());
        }
        control::request_close(inspection.focused_xid)?;
        document_provider.invalidate(inspection.focused_xid);
        return Ok(());
    }
    if route == CloseRoute::InternalDocuments {
        let state = document_provider.inspect(&inspection);
        let decision = documents::close_decision(&state);
        let action = match &decision {
            DocumentCloseDecision::CloseActiveDocument => "native-close-active-document",
            DocumentCloseDecision::PreserveTopLevelWindow => "iconify-final-document-window",
            DocumentCloseDecision::Refuse(_) => "refuse",
        };
        println!(
            "{} target={} policy=internal-documents provider=atspi action={} xid=0x{:08x} {}",
            source,
            inspection.app_identity,
            action,
            inspection.focused_xid,
            state.concise_summary()
        );
        if options.verbose {
            println!(
                "internal-documents accessibility={} provider={} launch-opt-in-detected={}",
                document_provider.availability(),
                inspection.app_identity,
                documents::launch_opt_in_status(&inspection)
            );
            println!("internal-documents {}", state.summary());
            print!("{}", report::render(&inspection, true));
        }
        if options.dry_run {
            return Ok(());
        }
        return match decision {
            DocumentCloseDecision::CloseActiveDocument => {
                let adapter = documents::adapter_for(&inspection.app_identity)
                    .ok_or("internal-document adapter disappeared")?;
                match adapter.close_document {
                    CloseDocumentMethod::ControlW => {
                        control::close_active_document(inspection.focused_xid)
                    }
                    CloseDocumentMethod::FeatherPadFileClose => document_provider
                        .close_featherpad_document(&inspection.identity_window)
                        .map_err(Into::into),
                }
            }
            DocumentCloseDecision::PreserveTopLevelWindow => {
                hide_and_remember(hidden, &inspection)
            }
            DocumentCloseDecision::Refuse(reason) => Err(reason.into()),
        };
    }
    dispatch_application_window_close(options, hidden, &inspection, source)
}

fn handle_close(
    options: RunOptions,
    hidden: &mut HiddenWindows,
    document_provider: &mut AtspiDocumentProvider,
) -> Result<(), DynError> {
    if hidden.pointer_intent_pending() {
        confirm_current_interaction(hidden, "button", true, options.verbose)?;
    }
    let (snapshot, inspection) = focused_inspection()?;
    handle_document_close_inspection(
        options,
        hidden,
        document_provider,
        snapshot,
        inspection,
        &format!("{} source=Cmd+W", LifecycleIntent::DocumentClose.name()),
    )
}

fn handle_window_close_request(
    options: RunOptions,
    hidden: &mut HiddenWindows,
    request: WindowCloseRequest,
) -> Result<(), DynError> {
    let snapshot = x11::collect_snapshot()?;
    reconcile_hidden(hidden, &snapshot);
    let inspection = inspect_close_target(&snapshot, request.target_xid)?;
    if hidden.pointer_intent_pending() {
        apply_confirmed_interaction(hidden, &inspection, "ssd-close-button", options.verbose);
    }
    let source = format!(
        "{} source=xfwm4 request_id={} timestamp={}",
        LifecycleIntent::WindowClose.name(), request.request_id, request.timestamp
    );
    dispatch_application_window_close(options, hidden, &inspection, &source)
}

fn hide_and_remember(
    hidden: &mut HiddenWindows,
    inspection: &Inspection,
) -> Result<(), DynError> {
    control::iconify_and_mark(inspection.focused_xid)?;
    thread::sleep(Duration::from_millis(300));
    let after = x11::collect_snapshot()?;
    let hidden_confirmed = after.windows.iter().any(|window| {
        window.xid == inspection.focused_xid
            && window.maclife_hidden
            && (!window.mapped
                || window
                    .states
                    .iter()
                    .any(|state| state == "_NET_WM_STATE_HIDDEN"))
    });
    if !hidden_confirmed {
        let _ = control::clear_hidden_marker(inspection.focused_xid);
        return Err(format!(
            "xfwm4 did not confirm iconification of 0x{:08x}",
            inspection.focused_xid
        )
        .into());
    }
    hidden.remember_as_logical(inspection.focused_xid, inspection.app_identity.clone());
    Ok(())
}

fn top_level_close_target(inspection: &Inspection) -> Result<u32, String> {
    match focus_kind(inspection) {
        FocusKind::Excluded => Err("focused window is excluded from lifecycle control".into()),
        FocusKind::Attached | FocusKind::Meaningful => Ok(inspection.focused_xid),
    }
}

fn handle_close_top_level(
    options: RunOptions,
    hidden: &mut HiddenWindows,
    document_provider: &mut AtspiDocumentProvider,
) -> Result<(), DynError> {
    if hidden.pointer_intent_pending() {
        confirm_current_interaction(hidden, "button", true, options.verbose)?;
    }
    let (snapshot, inspection) = focused_inspection()?;
    reconcile_hidden(hidden, &snapshot);
    document_provider.retain_windows(&snapshot.windows);
    let target = top_level_close_target(&inspection)?;
    let method = documents::adapter_for(&inspection.app_identity)
        .map(|adapter| adapter.close_top_level)
        .unwrap_or(CloseTopLevelMethod::WmDelete);
    let method_name = match method {
        CloseTopLevelMethod::ControlShiftW => "application-native-ctrl-shift-w",
        CloseTopLevelMethod::WmDelete => "native-wm-delete",
    };
    println!(
        "{} source=Shift+Cmd+W focused={} policy=top-level-window action=request-native-window-close method={} xid=0x{:08x}",
        LifecycleIntent::NativeTopLevelClose.name(), inspection.app_identity, method_name, target
    );
    if options.verbose {
        print!("{}", report::render(&inspection, true));
    }
    if options.dry_run {
        return Ok(());
    }
    match method {
        CloseTopLevelMethod::ControlShiftW => control::close_brave_top_level(target),
        CloseTopLevelMethod::WmDelete => control::request_close(target),
    }?;
    document_provider.invalidate(target);
    println!(
        "Shift+Cmd+W focused={} result=dispatched method={} xid=0x{:08x}",
        inspection.app_identity, method_name, target
    );
    Ok(())
}

fn command_status(program: &str, arguments: &[&str]) -> Result<(), DynError> {
    let status = Command::new(program).args(arguments).status()?;
    if !status.success() {
        return Err(format!("{program} exited with status {status}").into());
    }
    Ok(())
}

fn revalidate_process_metadata(
    original: &WindowFacts,
    current: &WindowFacts,
    original_pid: u32,
    current_pid: u32,
) -> Result<u32, DynError> {
    if current.xid != original.xid {
        return Err("refusing action: X11 window identity changed".into());
    }
    if current_pid != original_pid {
        return Err(format!(
            "refusing action: PID changed from {original_pid} to {current_pid}"
        )
        .into());
    }

    let original_process = original
        .process
        .as_ref()
        .ok_or("original process metadata disappeared")?;
    let current_process = current
        .process
        .as_ref()
        .ok_or("current process metadata disappeared")?;
    if current_process.uid != original_process.uid
        || current_process.parent_pid != original_process.parent_pid
        || current_process.name != original_process.name
        || current_process.executable != original_process.executable
        || current_process.command_line != original_process.command_line
    {
        return Err("refusing action: process identity changed".into());
    }
    Ok(original_pid)
}

fn single_native_quit_target(windows: &[WindowFacts]) -> Result<u32, String> {
    match windows {
        [window] => Ok(window.xid),
        [] => Err(
            "refused-no-safe-quit: application has no meaningful top-level window".to_string(),
        ),
        _ => Err(format!(
            "refused-no-safe-quit: application has {} meaningful top-level windows and no audited veto-capable application quit adapter",
            windows.len()
        )),
    }
}

fn compatibility_quit_target(
    adapter: &CompatibilityAdapter,
    original: &Inspection,
) -> Result<u32, DynError> {
    let original_pid = compatibility::validate_inspection(adapter, original)?;
    let snapshot = x11::collect_snapshot()?;
    let current = identity::inspect(original.identity_window.xid, snapshot.windows.clone())?;
    if current.app_identity != original.app_identity {
        return Err(format!(
            "adapter {} refuses changed application identity {} -> {}",
            adapter.name, original.app_identity, current.app_identity
        )
        .into());
    }
    let current_pid = compatibility::validate_inspection(adapter, &current)?;
    revalidate_process_metadata(
        &original.identity_window,
        &current.identity_window,
        original_pid,
        current_pid,
    )?;

    compatibility_quit_target_from_inspection(adapter, &current, &snapshot.windows)
        .map_err(Into::into)
}

fn compatibility_quit_target_from_inspection(
    adapter: &CompatibilityAdapter,
    inspection: &Inspection,
    snapshot_windows: &[WindowFacts],
) -> Result<u32, String> {
    let windows = match adapter.quit {
        CompatibilityQuit::ThunderbirdMenuQuit => {
            let main_windows: Vec<_> = inspection
                .meaningful_windows
                .iter()
                .filter(|window| {
                    window.wm_class.as_ref().is_some_and(|class| {
                        class.instance.eq_ignore_ascii_case("Mail")
                            && class.class.eq_ignore_ascii_case("thunderbird-default")
                    })
                })
                .collect();
            return match main_windows.as_slice() {
                [main] => Ok(main.xid),
                [] => Err("Thunderbird application Quit requires one main Mail window".into()),
                _ => Err(format!(
                    "Thunderbird application Quit found {} main Mail windows",
                    main_windows.len()
                )),
            };
        }
        CompatibilityQuit::CloseSingleLogicalWindow => inspection.meaningful_windows.clone(),
        CompatibilityQuit::CloseSingleFamilyWindow => {
            compatibility::family_windows(adapter, snapshot_windows)?
        }
    };
    single_native_quit_target(&windows)
}

fn quit_method_name(method: QuitMethod) -> &'static str {
    match method {
        QuitMethod::StrawberryMpris => "application-adapter/mpris-quit",
        QuitMethod::ThunarCli => "application-adapter/thunar--quit",
        QuitMethod::NativeCloseSingleWindow => "native-wm-delete",
        QuitMethod::CloseEachWindow => "native-wm-delete-each-window",
        QuitMethod::Unsupported => "unsupported",
    }
}

fn quit_strawberry() -> Result<(), DynError> {
    command_status(
        "gdbus",
        &[
            "call",
            "--session",
            "--dest",
            "org.mpris.MediaPlayer2.strawberry",
            "--object-path",
            "/org/mpris/MediaPlayer2",
            "--method",
            "org.mpris.MediaPlayer2.Quit",
        ],
    )
}

#[derive(Clone, Copy, Debug)]
enum QuitTargetSource {
    Focused,
    LogicalHidden,
}

impl QuitTargetSource {
    fn name(self) -> &'static str {
        match self {
            Self::Focused => "focused",
            Self::LogicalHidden => "logical-hidden",
        }
    }
}

fn validate_logical_inspection(
    hidden: &mut HiddenWindows,
    snapshot: &Snapshot,
    xid: u32,
    expected_identity: &str,
) -> Result<Inspection, DynError> {
    let candidate = snapshot.windows.iter().find(|window| window.xid == xid);
    let valid_window = candidate.is_some_and(|window| {
        window.maclife_hidden
            && is_actually_hidden(window)
            && identity::disposition(window) == Disposition::Meaningful
            && normalized_app_identity(window) == expected_identity
    });
    if !valid_window {
        hidden.forget(xid);
        return Err(format!(
            "logical active window 0x{xid:08x} is no longer a valid MacLife-hidden {expected_identity} window"
        )
        .into());
    }

    let inspection = match identity::inspect(xid, snapshot.windows.clone()) {
        Ok(inspection) => inspection,
        Err(error) => {
            hidden.forget(xid);
            return Err(error);
        }
    };
    if inspection.app_identity != expected_identity {
        hidden.forget(xid);
        return Err(format!(
            "logical active identity changed from {expected_identity} to {}",
            inspection.app_identity
        )
        .into());
    }
    Ok(inspection)
}

fn resolve_quit_target(
    hidden: &mut HiddenWindows,
) -> Result<(Inspection, QuitTargetSource), DynError> {
    let snapshot = x11::collect_snapshot()?;
    reconcile_hidden(hidden, &snapshot);
    let focused = identity::inspect(snapshot.active_window, snapshot.windows.clone()).ok();
    let focused_state = focused.as_ref().map(|inspection| {
        (inspection.identity_window.xid, focus_kind(inspection))
    });

    match lifecycle::select_active_target(hidden, focused_state) {
        ActiveTarget::Focused => focused
            .map(|inspection| (inspection, QuitTargetSource::Focused))
            .ok_or_else(|| "focused application disappeared during inspection".into()),
        ActiveTarget::LogicalHidden { xid, identity } => {
            let inspection = validate_logical_inspection(hidden, &snapshot, xid, &identity)?;
            Ok((inspection, QuitTargetSource::LogicalHidden))
        }
        ActiveTarget::Refuse => Err(
            "no meaningful focused application or valid MacLife-hidden logical app".into(),
        ),
    }
}

fn handle_quit(
    options: RunOptions,
    hidden: &mut HiddenWindows,
    document_provider: &mut AtspiDocumentProvider,
) -> Result<(), DynError> {
    if hidden.pointer_intent_pending() {
        confirm_current_interaction(hidden, "button", true, options.verbose)?;
    }
    let (inspection, source) = resolve_quit_target(hidden)?;
    let (policy, ineligible_reason) = policy_for(&inspection);
    let method = policy.quit;
    if policy.kind == PolicyKind::Refuse {
        let reason = ineligible_reason
            .unwrap_or_else(|| "application is not eligible for generic lifecycle control".into());
        println!(
            "Cmd+Q target={} source={} policy={} result=refused reason={}",
            inspection.app_identity,
            source.name(),
            policy.kind.name(),
            reason
        );
        return Err(reason.into());
    }
    if policy.kind == PolicyKind::Generic {
        if let Some(adapter) = compatibility::adapter_for(&inspection.app_identity) {
            let target = match compatibility_quit_target(adapter, &inspection) {
                Ok(target) => target,
                Err(reason) => {
                    println!(
                        "Cmd+Q target={} source={} policy={} adapter={} result=refused reason={}",
                        inspection.app_identity,
                        source.name(),
                        policy.kind.name(),
                        adapter.name,
                        reason
                    );
                    return Err(reason);
                }
            };
            println!(
                "Cmd+Q target={} source={} policy={} adapter={} action=request-graceful-quit method=application-adapter/{} xid=0x{:08x}",
                inspection.app_identity,
                source.name(),
                policy.kind.name(),
                adapter.name,
                adapter.quit.name(),
                target
            );
            if options.verbose {
                print!("{}", report::render(&inspection, true));
            }
            if options.dry_run {
                return Ok(());
            }
            let dispatched_method = match adapter.quit {
                CompatibilityQuit::ThunderbirdMenuQuit => {
                    control::activate_window_and_wait(target)?;
                    let main_window = inspection
                        .meaningful_windows
                        .iter()
                        .find(|window| window.xid == target)
                        .ok_or("validated Thunderbird Mail window disappeared")?;
                    if main_window.maclife_hidden {
                        control::clear_hidden_marker(target)?;
                        hidden.forget(target);
                    }
                    document_provider
                        .quit_thunderbird(main_window)
                        .map_err(|error| format!("Thunderbird native Quit failed: {error}"))?;
                    "atspi-file-quit"
                }
                CompatibilityQuit::CloseSingleLogicalWindow
                | CompatibilityQuit::CloseSingleFamilyWindow => {
                    control::request_close(target)?;
                    "native-wm-delete"
                }
            };
            println!(
                "Cmd+Q target={} source={} policy={} adapter={} result=dispatched method={} xid=0x{:08x}",
                inspection.app_identity,
                source.name(),
                policy.kind.name(),
                adapter.name,
                dispatched_method,
                target
            );
            return Ok(());
        }
    }
    if options.verbose {
        print!("{}", report::render(&inspection, true));
    }
    match method {
        QuitMethod::StrawberryMpris => {
            println!(
                "Cmd+Q target={} source={} policy={} adapter=none action=request-graceful-quit method={}",
                inspection.app_identity,
                source.name(),
                policy.kind.name(),
                quit_method_name(method)
            );
            if options.dry_run {
                return Ok(());
            }
            quit_strawberry()?;
            println!(
                "Cmd+Q target={} result=dispatched method={}",
                inspection.app_identity,
                quit_method_name(method)
            );
            Ok(())
        }
        QuitMethod::ThunarCli => {
            println!(
                "Cmd+Q target={} source={} policy={} adapter=none action=request-graceful-quit method={}",
                inspection.app_identity,
                source.name(),
                policy.kind.name(),
                quit_method_name(method)
            );
            if options.dry_run {
                return Ok(());
            }
            command_status("thunar", &["--quit"])?;
            println!(
                "Cmd+Q target={} result=dispatched method={}",
                inspection.app_identity,
                quit_method_name(method)
            );
            Ok(())
        }
        QuitMethod::NativeCloseSingleWindow => {
            let target = match single_native_quit_target(&inspection.meaningful_windows) {
                Ok(target) => target,
                Err(reason) => {
                    println!(
                        "Cmd+Q target={} source={} policy={} adapter=none result=refused reason={}",
                        inspection.app_identity,
                        source.name(),
                        policy.kind.name(),
                        reason
                    );
                    return Err(reason.into());
                }
            };
            println!(
                "Cmd+Q target={} source={} policy={} adapter=none action=request-graceful-quit method={} xid=0x{:08x}",
                inspection.app_identity,
                source.name(),
                policy.kind.name(),
                quit_method_name(method),
                target
            );
            if options.dry_run {
                return Ok(());
            }
            control::request_close(target)?;
            println!(
                "Cmd+Q target={} result=dispatched method={} xid=0x{:08x}",
                inspection.app_identity,
                quit_method_name(method),
                target
            );
            Ok(())
        }
        QuitMethod::CloseEachWindow => {
            if inspection.meaningful_windows.is_empty() {
                let reason = "refused-no-safe-quit: application has no meaningful top-level window";
                println!(
                    "Cmd+Q target={} source={} policy={} adapter=none result=refused reason={}",
                    inspection.app_identity,
                    source.name(),
                    policy.kind.name(),
                    reason
                );
                return Err(reason.into());
            }
            println!(
                "Cmd+Q target={} source={} policy={} adapter=none action=request-graceful-quit method={} windows={}",
                inspection.app_identity,
                source.name(),
                policy.kind.name(),
                quit_method_name(method),
                inspection.meaningful_windows.len()
            );
            if options.dry_run {
                return Ok(());
            }
            for window in &inspection.meaningful_windows {
                control::request_close(window.xid)?;
            }
            println!(
                "Cmd+Q target={} result=dispatched method={} windows={}",
                inspection.app_identity,
                quit_method_name(method),
                inspection.meaningful_windows.len()
            );
            Ok(())
        }
        QuitMethod::Unsupported => {
            let reason = format!(
                "refused-no-safe-quit: no veto-capable quit method for {}",
                inspection.app_identity
            );
            println!(
                "Cmd+Q target={} source={} policy={} adapter=none result=refused reason={}",
                inspection.app_identity,
                source.name(),
                policy.kind.name(),
                reason
            );
            Err(reason.into())
        }
    }
}

fn handle_event(
    conn: &RustConnection,
    event: Event,
    root: u32,
    active_window_atom: u32,
    lifecycle_manager: &LifecycleManager,
    intent_devices: &mut IntentDevices,
    chord_tracker: &mut LifecycleChordTracker,
    hidden: &mut HiddenWindows,
    document_provider: &mut AtspiDocumentProvider,
    options: RunOptions,
) {
    match event {
        // Execute on release so a native shortcut is never injected while the
        // dedicated Toshy function key is still part of the X11 key state.
        Event::KeyRelease(event) => {
            let result = if event.detail == options.close_keycode {
                handle_close(options, hidden, document_provider)
            } else if event.detail == options.quit_keycode {
                handle_quit(options, hidden, document_provider)
            } else if event.detail == options.close_window_keycode {
                handle_close_top_level(options, hidden, document_provider)
            } else {
                return;
            };
            if let Err(error) = result {
                eprintln!("MacLife action refused/failed: {error}");
            }
        }
        Event::PropertyNotify(event)
            if event.window == root && event.atom == active_window_atom =>
        {
            if let Ok(snapshot) = x11::collect_snapshot() {
                observe_focus(hidden, &snapshot, options.verbose);
            }
        }
        Event::ClientMessage(event) => match lifecycle_manager.decode(&event) {
            Ok(Some(request)) => {
                if let Err(error) =
                    handle_window_close_request(options, hidden, request)
                {
                    eprintln!(
                        "MacLife WindowClose(XID) request_id={} target=0x{:08x} refused/failed: {error}",
                        request.request_id, request.target_xid
                    );
                }
            }
            Ok(None) => {}
            Err(error) => eprintln!("MacLife private window-close request refused: {error}"),
        },
        Event::SelectionClear(event) if lifecycle_manager.lost_selection(&event) => {
            eprintln!(
                "MacLife private lifecycle manager selection was lost; SSD close requests will fail closed"
            );
        }
        Event::XinputRawKeyPress(event)
            if intent_devices.is_toshy_keyboard(event.sourceid) =>
        {
            observe_keyboard_intent(
                hidden,
                chord_tracker,
                event.detail,
                KeyPhase::Press,
                event.sourceid,
                options,
            );
        }
        Event::XinputRawKeyRelease(event)
            if intent_devices.is_toshy_keyboard(event.sourceid) =>
        {
            observe_keyboard_intent(
                hidden,
                chord_tracker,
                event.detail,
                KeyPhase::Release,
                event.sourceid,
                options,
            );
        }
        Event::XinputRawButtonPress(event)
            if intent_devices.is_user_pointer(event.sourceid) =>
        {
            if hidden.logical_active().is_some() {
                hidden.note_pointer_intent();
                if options.verbose {
                    println!(
                        "user-intent source=button pending-focus-confirmation sourceid={}",
                        event.sourceid
                    );
                }
            }
        }
        Event::XinputHierarchy(_) => {
            if let Err(error) = intent_devices.refresh(conn) {
                eprintln!("MacLife XInput device refresh failed: {error}");
            } else if options.verbose {
                println!("XInput devices refreshed: {}", intent_devices.summary());
            }
        }
        _ => {}
    }
}

fn wait_for_x_or_shutdown(
    conn: &RustConnection,
    signals: &ShutdownSignals,
) -> Result<bool, DynError> {
    let mut descriptors = [
        libc::pollfd {
            fd: conn.stream().as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        },
        libc::pollfd {
            fd: signals.read_fd(),
            events: libc::POLLIN,
            revents: 0,
        },
    ];
    loop {
        // SAFETY: descriptors points to two initialized pollfd values for the
        // duration of this blocking call.
        let result = unsafe { libc::poll(descriptors.as_mut_ptr(), descriptors.len() as _, -1) };
        if result < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == ErrorKind::Interrupted {
                continue;
            }
            return Err(error.into());
        }
        if descriptors[1].revents & libc::POLLIN != 0 && signals.consume()? {
            return Ok(false);
        }
        if descriptors[0].revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
            return Err("X11 connection was lost".into());
        }
        if descriptors[0].revents & libc::POLLIN != 0 {
            return Ok(true);
        }
    }
}

pub fn run(options: RunOptions) -> Result<(), DynError> {
    let keycodes = [
        options.close_keycode,
        options.quit_keycode,
        options.close_window_keycode,
    ];
    if keycodes[0] == keycodes[1]
        || keycodes[0] == keycodes[2]
        || keycodes[1] == keycodes[2]
    {
        return Err("close, quit, and close-window keycodes must differ".into());
    }
    let _singleton = match SingletonGuard::acquire()? {
        AcquireResult::Acquired(guard) => guard,
        AcquireResult::AlreadyRunning => {
            println!("MacLife already running; refusing second instance");
            return Ok(());
        }
    };
    let shutdown_signals = ShutdownSignals::install()?;
    let (conn, screen_number) = x11rb::connect(None)?;
    let root = conn
        .setup()
        .roots
        .get(screen_number)
        .ok_or("X11 screen number is out of range")?
        .root;
    let active_window_atom = conn
        .intern_atom(true, b"_NET_ACTIVE_WINDOW")?
        .reply()?
        .atom;
    if active_window_atom == 0 {
        return Err("X11 session does not expose _NET_ACTIVE_WINDOW".into());
    }
    conn.change_window_attributes(
        root,
        &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE),
    )?
    .check()?;
    let lifecycle_manager = LifecycleManager::claim(&conn, screen_number, root)?;
    let mut intent_devices = IntentDevices::initialize(&conn, root)?;
    let mut chord_tracker = LifecycleChordTracker::default();
    let mut document_provider = AtspiDocumentProvider::connect();
    conn.grab_key(
        false,
        root,
        ModMask::ANY,
        options.close_keycode,
        GrabMode::ASYNC,
        GrabMode::ASYNC,
    )?
    .check()?;
    conn.grab_key(
        false,
        root,
        ModMask::ANY,
        options.close_window_keycode,
        GrabMode::ASYNC,
        GrabMode::ASYNC,
    )?
    .check()?;
    conn.grab_key(
        false,
        root,
        ModMask::ANY,
        options.quit_keycode,
        GrabMode::ASYNC,
        GrabMode::ASYNC,
    )?
    .check()?;
    conn.flush()?;

    if !intent_devices.has_toshy_keyboard() {
        eprintln!(
            "MacLife: no enabled XWayKeyz virtual keyboard found; keyboard intent remains conservative"
        );
    }

    let mut hidden = HiddenWindows::default();
    if let Ok(snapshot) = x11::collect_snapshot() {
        reconcile_hidden(&mut hidden, &snapshot);
    }
    println!(
        "MacLife started: close_keycode={} quit_keycode={} close_window_keycode={} dry_run={} adopted_hidden={} atspi={} lifecycle_protocol={} lifecycle_manager=0x{:08x} {}",
        options.close_keycode,
        options.quit_keycode,
        options.close_window_keycode,
        options.dry_run,
        hidden.len(),
        document_provider.availability(),
        LIFECYCLE_PROTOCOL_VERSION,
        lifecycle_manager.window(),
        intent_devices.summary()
    );

    loop {
        while let Some(event) = conn.poll_for_event()? {
            handle_event(
                &conn,
                event,
                root,
                active_window_atom,
                &lifecycle_manager,
                &mut intent_devices,
                &mut chord_tracker,
                &mut hidden,
                &mut document_provider,
                options,
            );
        }
        if !wait_for_x_or_shutdown(&conn, &shutdown_signals)? {
            break;
        }
    }
    println!("MacLife stopping: received termination signal");
    Ok(())
}

fn select_restore_candidate(windows: &[WindowFacts], wanted: &str) -> Result<u32, String> {
    let matches: Vec<_> = windows
        .iter()
        .filter(|window| {
            window.maclife_hidden
                && identity::disposition(window) == Disposition::Meaningful
                && normalized_app_identity(window) == wanted
        })
        .map(|window| window.xid)
        .collect();
    match matches.as_slice() {
        [] => Err(format!("no MacLife-hidden window for {wanted}")),
        [xid] => Ok(*xid),
        _ => Err(format!(
            "multiple MacLife-hidden windows for {wanted}; refusing ambiguous restore"
        )),
    }
}

pub fn restore(identity: &str) -> Result<(), DynError> {
    let wanted = identity.trim().to_ascii_lowercase();
    let snapshot = x11::collect_snapshot()?;
    let xid = select_restore_candidate(&snapshot.windows, &wanted)?;

    control::activate_window(xid)?;
    thread::sleep(Duration::from_millis(500));
    let after = x11::collect_snapshot()?;
    let restored = after.windows.iter().find(|candidate| candidate.xid == xid);
    let is_visible = restored.is_some_and(|candidate| {
        candidate.mapped
            && !candidate
                .states
                .iter()
                .any(|state| state == "_NET_WM_STATE_HIDDEN")
    });
    if !is_visible {
        return Err(format!(
            "xfwm4 did not confirm restoration of 0x{:08x}; marker retained",
            xid
        )
        .into());
    }
    control::clear_hidden_marker(xid)?;
    println!("restored={} xid=0x{:08x}", wanted, xid);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        application_window_close_decision, close_route,
        compatibility_quit_target_from_inspection, inspect_close_target, is_adoptable_hidden,
        revalidate_process_metadata, select_restore_candidate, single_native_quit_target,
        top_level_close_target, CloseRoute,
    };
    use crate::compatibility;
    use crate::identity;
    use crate::lifecycle::{CloseDecision, FocusKind, LifecycleIntent};
    use crate::model::{ProcessInfo, Snapshot, WindowFacts, WmClass};

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
    fn single_window_generic_quit_targets_exact_native_window() {
        let window = WindowFacts::test_window(10, "FeatherPad");
        assert_eq!(single_native_quit_target(&[window]), Ok(10));
    }

    #[test]
    fn multi_window_generic_quit_refuses_without_a_kill_fallback() {
        let first = WindowFacts::test_window(10, "FeatherPad");
        let second = WindowFacts::test_window(20, "FeatherPad");
        let error = single_native_quit_target(&[first, second]).expect_err("must refuse");
        assert!(error.contains("refused-no-safe-quit"));
        assert!(error.contains("2 meaningful top-level windows"));
    }

    #[test]
    fn shift_close_targets_exact_focused_main_or_transient_xid() {
        let main = WindowFacts::test_window(10, "FeatherPad");
        let inspection = identity::inspect(10, vec![main.clone()]).expect("main inspection");
        assert_eq!(top_level_close_target(&inspection), Ok(10));

        let mut dialog = WindowFacts::test_window(20, "FeatherPad");
        dialog.transient_for = Some(10);
        dialog.window_types = vec!["_NET_WM_WINDOW_TYPE_DIALOG".to_string()];
        let inspection = identity::inspect(20, vec![main, dialog]).expect("dialog inspection");
        assert_eq!(top_level_close_target(&inspection), Ok(20));
    }

    #[test]
    fn lifecycle_intents_remain_explicit_and_distinct() {
        assert_eq!(LifecycleIntent::DocumentClose.name(), "DocumentClose");
        assert_eq!(LifecycleIntent::WindowClose.name(), "WindowClose");
        assert_eq!(
            LifecycleIntent::NativeTopLevelClose.name(),
            "NativeTopLevelClose"
        );
        assert_eq!(LifecycleIntent::ApplicationQuit.name(), "ApplicationQuit");
    }

    #[test]
    fn tab_aware_document_close_does_not_change_window_close_semantics() {
        let brave = WindowFacts::test_window(10, "Brave-origin");
        let inspection = identity::inspect(10, vec![brave]).expect("inspection");

        assert_eq!(
            close_route(&inspection, FocusKind::Meaningful),
            CloseRoute::InternalDocuments
        );
        assert_eq!(
            application_window_close_decision(&inspection).1,
            CloseDecision::HideLast
        );
    }

    #[test]
    fn window_close_uses_top_level_count_and_exact_dialog_target() {
        let first = WindowFacts::test_window(10, "Brave-origin");
        let second = WindowFacts::test_window(20, "Brave-origin");
        let inspection = identity::inspect(10, vec![first.clone(), second])
            .expect("multi-window inspection");
        assert_eq!(
            application_window_close_decision(&inspection).1,
            CloseDecision::CloseFocused
        );
        assert_eq!(inspection.focused_xid, 10);

        let mut dialog = WindowFacts::test_window(30, "Brave-origin");
        dialog.transient_for = Some(10);
        dialog.window_types = vec!["_NET_WM_WINDOW_TYPE_DIALOG".to_string()];
        let inspection = identity::inspect(30, vec![first, dialog])
            .expect("dialog inspection");
        assert_eq!(
            application_window_close_decision(&inspection).1,
            CloseDecision::CloseFocused
        );
        assert_eq!(inspection.focused_xid, 30);
    }

    #[test]
    fn standalone_settings_dialog_uses_final_window_preservation() {
        let mut settings = WindowFacts::test_window(40, "Xfce4-settings-manager");
        settings.window_types = vec!["_NET_WM_WINDOW_TYPE_DIALOG".to_string()];
        let inspection = identity::inspect(40, vec![settings]).expect("inspection");
        assert_eq!(
            application_window_close_decision(&inspection).1,
            CloseDecision::HideLast
        );
    }

    #[test]
    fn thunderbird_compose_closes_natively_without_changing_mail_tab_routing() {
        let mut mail = WindowFacts::test_window(10, "thunderbird-default");
        mail.wm_class = Some(WmClass {
            instance: "Mail".to_string(),
            class: "thunderbird-default".to_string(),
        });
        let mut compose = WindowFacts::test_window(20, "thunderbird-default");
        compose.wm_class = Some(WmClass {
            instance: "Msgcompose".to_string(),
            class: "thunderbird-default".to_string(),
        });

        let mail_inspection = identity::inspect(10, vec![mail.clone(), compose.clone()])
            .expect("mail inspection");
        assert_eq!(
            close_route(&mail_inspection, FocusKind::Meaningful),
            CloseRoute::InternalDocuments
        );

        let compose_inspection =
            identity::inspect(20, vec![mail, compose]).expect("compose inspection");
        assert_eq!(
            close_route(&compose_inspection, FocusKind::Meaningful),
            CloseRoute::ThunderbirdCompose
        );
        assert_eq!(compose_inspection.focused_xid, 20);
        assert_eq!(
            application_window_close_decision(&compose_inspection).1,
            CloseDecision::CloseFocused
        );

        let compose_only = identity::inspect(20, vec![compose_inspection.focused_window.clone()])
            .expect("compose-only inspection");
        assert_eq!(
            application_window_close_decision(&compose_only).1,
            CloseDecision::CloseFocused
        );
    }

    #[test]
    fn featherpad_document_close_uses_internal_tabs_but_window_close_does_not() {
        let window = WindowFacts::test_window(10, "FeatherPad");
        let inspection = identity::inspect(10, vec![window]).expect("inspection");
        assert_eq!(
            close_route(&inspection, FocusKind::Meaningful),
            CloseRoute::InternalDocuments
        );
        assert_eq!(
            application_window_close_decision(&inspection).1,
            CloseDecision::HideLast
        );
    }

    #[test]
    fn thunderbird_multi_window_quit_targets_its_single_main_mail_window() {
        let mut mail = process_window(10, "thunderbird-default", 1234, "thunderbird-bin");
        mail.wm_class = Some(WmClass {
            instance: "Mail".to_string(),
            class: "thunderbird-default".to_string(),
        });
        let mut compose =
            process_window(20, "thunderbird-default", 1234, "thunderbird-bin");
        compose.wm_class = Some(WmClass {
            instance: "Msgcompose".to_string(),
            class: "thunderbird-default".to_string(),
        });
        let windows = vec![mail, compose];
        let inspection = identity::inspect(20, windows.clone()).expect("inspection");
        let adapter = compatibility::adapter_for("thunderbird-default").expect("adapter");

        assert_eq!(
            compatibility_quit_target_from_inspection(adapter, &inspection, &windows),
            Ok(10)
        );

        let compose_only = identity::inspect(20, vec![windows[1].clone()]).expect("compose");
        assert!(compatibility_quit_target_from_inspection(
            adapter,
            &compose_only,
            &[windows[1].clone()]
        )
        .is_err());

        let mut second_mail = windows[0].clone();
        second_mail.xid = 30;
        let ambiguous_windows = vec![windows[0].clone(), windows[1].clone(), second_mail];
        let ambiguous =
            identity::inspect(20, ambiguous_windows.clone()).expect("ambiguous main windows");
        assert!(compatibility_quit_target_from_inspection(
            adapter,
            &ambiguous,
            &ambiguous_windows
        )
        .is_err());
    }

    #[test]
    fn close_target_uses_requested_xid_not_active_window() {
        let snapshot = Snapshot {
            active_window: 10,
            windows: vec![
                WindowFacts::test_window(10, "FeatherPad"),
                WindowFacts::test_window(20, "Thunar"),
            ],
        };
        let inspection = inspect_close_target(&snapshot, 20).expect("exact target");
        assert_eq!(inspection.focused_xid, 20);
        assert_eq!(inspection.app_identity, "thunar");
        assert!(inspect_close_target(&snapshot, 30).is_err());
    }

    #[test]
    fn compatibility_revalidation_rejects_changed_process_metadata() {
        let original = process_window(10, "thunderbird-default", 1234, "thunderbird-bin");
        let mut current = original.clone();
        current.process.as_mut().expect("process").command_line =
            Some("thunderbird-bin --changed".to_string());
        let adapter = compatibility::adapter_for("thunderbird-default").expect("adapter");
        let original_inspection =
            identity::inspect(10, vec![original.clone()]).expect("inspection");
        let current_inspection =
            identity::inspect(10, vec![current.clone()]).expect("inspection");
        let original_pid =
            compatibility::validate_inspection(adapter, &original_inspection).expect("PID");
        let current_pid =
            compatibility::validate_inspection(adapter, &current_inspection).expect("PID");
        assert!(revalidate_process_metadata(
            &original,
            &current,
            original_pid,
            current_pid
        )
        .is_err());
    }

    #[test]
    fn generic_restore_requires_marker_and_refuses_ambiguity() {
        let mut hidden = WindowFacts::test_window(10, "FeatherPad");
        hidden.maclife_hidden = true;
        assert_eq!(
            select_restore_candidate(&[hidden.clone()], "featherpad").expect("candidate"),
            10
        );

        let visible = WindowFacts::test_window(20, "FeatherPad");
        assert!(select_restore_candidate(&[visible], "featherpad").is_err());

        let mut second = hidden.clone();
        second.xid = 30;
        assert!(select_restore_candidate(&[hidden, second], "featherpad").is_err());
    }

    #[test]
    fn brave_restore_selection_never_crosses_variant_or_app_identity() {
        let mut origin = WindowFacts::test_window(10, "Brave-origin");
        origin.maclife_hidden = true;
        let mut browser = WindowFacts::test_window(20, "Brave-browser");
        browser.maclife_hidden = true;
        let mut origin_app = WindowFacts::test_window(30, "Brave-origin");
        origin_app.maclife_hidden = true;
        origin_app.wm_class = Some(crate::model::WmClass {
            instance: "crx_mjoklplbddabcmpepnokjaffbmgbkkgg".to_string(),
            class: "Brave-origin".to_string(),
        });

        let windows = [origin, browser, origin_app];
        assert_eq!(
            select_restore_candidate(&windows, "brave-origin").expect("origin candidate"),
            10
        );
        assert_eq!(
            select_restore_candidate(&windows, "brave-browser").expect("browser candidate"),
            20
        );
        assert_eq!(
            select_restore_candidate(
                &windows,
                "brave-origin-crx-mjoklplbddabcmpepnokjaffbmgbkkgg",
            )
            .expect("origin app candidate"),
            30
        );
    }

    #[test]
    fn restart_adopts_only_stable_meaningful_hidden_windows() {
        let mut valid = WindowFacts::test_window(10, "FeatherPad");
        valid.maclife_hidden = true;
        valid.mapped = false;
        assert!(is_adoptable_hidden(&valid));

        let mut visible = valid.clone();
        visible.mapped = true;
        assert!(!is_adoptable_hidden(&visible));

        let mut unknown = WindowFacts::test_window(20, "");
        unknown.maclife_hidden = true;
        unknown.mapped = false;
        assert!(!is_adoptable_hidden(&unknown));

        let mut desktop = valid;
        desktop.window_types = vec!["_NET_WM_WINDOW_TYPE_DESKTOP".to_string()];
        assert!(!is_adoptable_hidden(&desktop));
    }
}
