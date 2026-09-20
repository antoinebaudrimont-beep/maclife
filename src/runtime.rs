use crate::control;
use crate::identity::{self, normalized_app_identity};
use crate::input::{IntentDevices, KeyPhase, LifecycleChordTracker};
use crate::lifecycle::{
    self, ActiveTarget, ApplicationPolicy, CloseDecision, FocusKind, HiddenWindows, PolicyKind,
    QuitMethod,
};
use crate::model::{Disposition, Inspection, Snapshot, WindowFacts};
use crate::{process, report, x11, DynError};
use std::process::Command;
use std::thread;
use std::time::Duration;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    ChangeWindowAttributesAux, ConnectionExt, EventMask, GrabMode, ModMask,
};
use x11rb::protocol::Event;

pub const DEFAULT_CLOSE_KEYCODE: u8 = 191;
pub const DEFAULT_QUIT_KEYCODE: u8 = 192;

#[derive(Clone, Copy, Debug)]
pub struct RunOptions {
    pub close_keycode: u8,
    pub quit_keycode: u8,
    pub dry_run: bool,
    pub verbose: bool,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            close_keycode: DEFAULT_CLOSE_KEYCODE,
            quit_keycode: DEFAULT_QUIT_KEYCODE,
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

fn is_actually_hidden(window: &WindowFacts) -> bool {
    !window.mapped
        || window
            .states
            .iter()
            .any(|state| state == "_NET_WM_STATE_HIDDEN")
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
    let marked: Vec<_> = snapshot
        .windows
        .iter()
        .filter(|window| window.maclife_hidden && is_actually_hidden(window))
        .map(|window| window.xid)
        .collect();
    hidden.retain_marked(&marked);
    for window in snapshot
        .windows
        .iter()
        .filter(|window| window.maclife_hidden && is_actually_hidden(window))
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

fn handle_close(
    options: RunOptions,
    hidden: &mut HiddenWindows,
) -> Result<(), DynError> {
    if hidden.pointer_intent_pending() {
        confirm_current_interaction(hidden, "button", true, options.verbose)?;
    }
    let (snapshot, inspection) = focused_inspection()?;
    reconcile_hidden(hidden, &snapshot);
    let (policy, ineligible_reason) = policy_for(&inspection);
    let decision = lifecycle::close_decision(
        policy,
        inspection.meaningful_windows.len(),
        focus_kind(&inspection),
    );
    println!(
        "Cmd+W focused={} meaningful_windows={} policy={} action={} xid=0x{:08x}",
        inspection.app_identity,
        inspection.meaningful_windows.len(),
        policy.kind.name(),
        action_name(&decision),
        inspection.focused_xid
    );
    if options.verbose {
        print!("{}", report::render(&inspection, true));
    }
    if options.dry_run {
        return Ok(());
    }

    match decision {
        CloseDecision::CloseFocused | CloseDecision::NativeCloseLast => {
            control::request_close(inspection.focused_xid)
        }
        CloseDecision::HideLast => {
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
            hidden.remember_as_logical(inspection.focused_xid, inspection.app_identity);
            Ok(())
        }
        CloseDecision::Refuse(reason) => Err(ineligible_reason.unwrap_or(reason).into()),
    }
}

fn command_status(program: &str, arguments: &[&str]) -> Result<(), DynError> {
    let status = Command::new(program).args(arguments).status()?;
    if !status.success() {
        return Err(format!("{program} exited with status {status}").into());
    }
    Ok(())
}

fn validated_pid(window: &WindowFacts, expected_identity: &str) -> Result<u32, DynError> {
    let identity = normalized_app_identity(window);
    if identity != expected_identity {
        return Err(format!(
            "refusing SIGTERM: expected {expected_identity} identity, found {identity}"
        )
        .into());
    }
    if !window.pid_validated {
        return Err(format!(
            "refusing SIGTERM: _NET_WM_PID is not validated ({})",
            window.pid_validation
        )
        .into());
    }
    let pid = window.pid.ok_or("validated window has no PID")?;
    let process = window
        .process
        .as_ref()
        .ok_or("validated window has no process metadata")?;
    if process.pid != pid {
        return Err("refusing SIGTERM: window and process PIDs differ".into());
    }
    if pid == std::process::id() {
        return Err("refusing to terminate MacLife itself".into());
    }
    Ok(pid)
}

fn generic_process_pid(inspection: &Inspection) -> Result<u32, DynError> {
    identity::generic_lifecycle_eligibility(inspection)
        .map_err(|reason| format!("refusing generic quit: {reason}"))?;
    let expected_identity = &inspection.app_identity;
    let pid = validated_pid(&inspection.identity_window, expected_identity)?;
    let process = inspection
        .identity_window
        .process
        .as_ref()
        .ok_or("refusing generic quit: process metadata is missing")?;
    let process_identity = identity::normalized_process_identity(process);
    if process_identity != *expected_identity {
        return Err(format!(
            "refusing generic quit: window identity {expected_identity} does not match process identity {process_identity}"
        )
        .into());
    }
    if process
        .command_line
        .as_deref()
        .is_some_and(|command| command.split_whitespace().any(|part| part.starts_with("--type=")))
        || process.name.to_ascii_lowercase().contains("helper")
    {
        return Err(
            "refusing generic quit: window PID appears to be a helper process".into(),
        );
    }

    for window in &inspection.meaningful_windows {
        let window_pid = validated_pid(window, expected_identity)?;
        if window_pid != pid {
            return Err(format!(
                "refusing generic quit: meaningful windows expose multiple validated PIDs ({pid} and {window_pid})"
            )
            .into());
        }
        let window_process = window
            .process
            .as_ref()
            .ok_or("refusing generic quit: meaningful window process metadata is missing")?;
        if identity::normalized_process_identity(window_process) != *expected_identity {
            return Err(
                "refusing generic quit: a meaningful window has ambiguous process identity".into(),
            );
        }
    }
    Ok(pid)
}

fn revalidate_same_process(
    original: &WindowFacts,
    current: &WindowFacts,
    expected_identity: &str,
) -> Result<u32, DynError> {
    if current.xid != original.xid {
        return Err("refusing SIGTERM: X11 window identity changed".into());
    }
    let original_pid = validated_pid(original, expected_identity)?;
    let current_pid = validated_pid(current, expected_identity)?;
    if current_pid != original_pid {
        return Err(format!(
            "refusing SIGTERM: PID changed from {original_pid} to {current_pid}"
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
        return Err("refusing SIGTERM: process identity changed".into());
    }
    Ok(original_pid)
}

fn terminate_revalidated_pid(
    inspection: &Inspection,
    require_generic_association: bool,
) -> Result<(), DynError> {
    let initial_pid = if require_generic_association {
        generic_process_pid(inspection)?
    } else {
        validated_pid(&inspection.identity_window, &inspection.app_identity)?
    };

    let snapshot = x11::collect_snapshot()?;
    let current = identity::inspect(inspection.identity_window.xid, snapshot.windows)?;
    if current.app_identity != inspection.app_identity {
        return Err(format!(
            "refusing SIGTERM: application identity changed from {} to {}",
            inspection.app_identity, current.app_identity
        )
        .into());
    }
    let revalidated_pid = revalidate_same_process(
        &inspection.identity_window,
        &current.identity_window,
        &inspection.app_identity,
    )?;
    if revalidated_pid != initial_pid {
        return Err("refusing SIGTERM: process association changed during validation".into());
    }
    if require_generic_association && generic_process_pid(&current)? != initial_pid {
        return Err("refusing generic quit: application process association changed".into());
    }

    command_status("/bin/kill", &["-TERM", &initial_pid.to_string()])
}

fn quit_method_name(method: QuitMethod) -> &'static str {
    match method {
        QuitMethod::StrawberryMpris => "mpris-then-validated-pid-sigterm",
        QuitMethod::ThunarCli => "thunar--quit",
        QuitMethod::GenericValidatedPidTerm => "validated-generic-pid-sigterm",
        QuitMethod::ValidatedPidTerm => "validated-pid-sigterm",
        QuitMethod::CloseEachWindow => "wm-delete-each-window",
        QuitMethod::Unsupported => "unsupported",
    }
}

fn quit_strawberry(inspection: &Inspection) -> Result<(), DynError> {
    let original = &inspection.identity_window;
    let pid = validated_pid(original, "strawberry")?;
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
    )?;
    thread::sleep(Duration::from_millis(750));
    if process::read_process(pid).is_none() {
        return Ok(());
    }

    let snapshot = x11::collect_snapshot()?;
    let current = snapshot
        .windows
        .iter()
        .find(|window| window.xid == original.xid)
        .ok_or_else(|| {
            format!(
                "Strawberry PID {pid} remained after MPRIS Quit, but its original window disappeared; refusing SIGTERM fallback"
            )
        })?;
    let revalidated_pid = revalidate_same_process(original, current, "strawberry")?;
    eprintln!(
        "MacLife: Strawberry MPRIS Quit returned but PID {revalidated_pid} remained; using revalidated SIGTERM fallback"
    );
    command_status("/bin/kill", &["-TERM", &revalidated_pid.to_string()])
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

fn handle_quit(options: RunOptions, hidden: &mut HiddenWindows) -> Result<(), DynError> {
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
    if method == QuitMethod::GenericValidatedPidTerm {
        if let Err(error) = generic_process_pid(&inspection) {
            println!(
                "Cmd+Q target={} source={} policy={} result=refused reason={}",
                inspection.app_identity,
                source.name(),
                policy.kind.name(),
                error
            );
            return Err(error);
        }
    }
    println!(
        "Cmd+Q target={} source={} policy={} action=application-quit method={}",
        inspection.app_identity,
        source.name(),
        policy.kind.name(),
        quit_method_name(method)
    );
    if options.verbose {
        print!("{}", report::render(&inspection, true));
    }
    if options.dry_run {
        return Ok(());
    }

    let result = match method {
        QuitMethod::StrawberryMpris => quit_strawberry(&inspection),
        QuitMethod::ThunarCli => command_status("thunar", &["--quit"]),
        QuitMethod::GenericValidatedPidTerm => terminate_revalidated_pid(&inspection, true),
        QuitMethod::ValidatedPidTerm => terminate_revalidated_pid(&inspection, false),
        QuitMethod::CloseEachWindow => {
            for window in &inspection.meaningful_windows {
                control::request_close(window.xid)?;
            }
            Ok(())
        }
        QuitMethod::Unsupported => Err(format!(
            "no safe whole-application quit method for {}",
            inspection.app_identity
        )
        .into()),
    };
    if result.is_ok() {
        hidden.forget_identity(&inspection.app_identity);
    }
    result
}

pub fn run(options: RunOptions) -> Result<(), DynError> {
    if options.close_keycode == options.quit_keycode {
        return Err("close and quit keycodes must differ".into());
    }
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
    let mut intent_devices = IntentDevices::initialize(&conn, root)?;
    let mut chord_tracker = LifecycleChordTracker::default();
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
        "MacLife listening: close_keycode={} quit_keycode={} dry_run={} adopted_hidden={} {}",
        options.close_keycode,
        options.quit_keycode,
        options.dry_run,
        hidden.len(),
        intent_devices.summary()
    );

    loop {
        match conn.wait_for_event()? {
            Event::KeyPress(event) => {
                let result = if event.detail == options.close_keycode {
                    handle_close(options, &mut hidden)
                } else if event.detail == options.quit_keycode {
                    handle_quit(options, &mut hidden)
                } else {
                    continue;
                };
                if let Err(error) = result {
                    eprintln!("MacLife action refused/failed: {error}");
                }
            }
            Event::PropertyNotify(event)
                if event.window == root && event.atom == active_window_atom =>
            {
                if let Ok(snapshot) = x11::collect_snapshot() {
                    observe_focus(&mut hidden, &snapshot, options.verbose);
                }
            }
            Event::XinputRawKeyPress(event)
                if intent_devices.is_toshy_keyboard(event.sourceid) =>
            {
                observe_keyboard_intent(
                    &mut hidden,
                    &mut chord_tracker,
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
                    &mut hidden,
                    &mut chord_tracker,
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
                if let Err(error) = intent_devices.refresh(&conn) {
                    eprintln!("MacLife XInput device refresh failed: {error}");
                } else if options.verbose {
                    println!("XInput devices refreshed: {}", intent_devices.summary());
                }
            }
            _ => {}
        }
    }
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
    use super::{generic_process_pid, revalidate_same_process, select_restore_candidate};
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

    fn strawberry_window(xid: u32, pid: u32) -> WindowFacts {
        process_window(xid, "Strawberry", pid, "strawberry")
    }

    #[test]
    fn strawberry_fallback_requires_same_freshly_validated_process() {
        let original = strawberry_window(10, 1234);
        let current = original.clone();
        assert_eq!(
            revalidate_same_process(&original, &current, "strawberry").expect("same process"),
            1234
        );
    }

    #[test]
    fn strawberry_fallback_rejects_pid_or_validation_changes() {
        let original = strawberry_window(10, 1234);

        let mut changed_pid = strawberry_window(10, 5678);
        assert!(revalidate_same_process(&original, &changed_pid, "strawberry").is_err());

        changed_pid = original.clone();
        changed_pid.pid_validated = false;
        changed_pid.pid_validation = "process unavailable".to_string();
        assert!(revalidate_same_process(&original, &changed_pid, "strawberry").is_err());

        let mut changed_metadata = original.clone();
        changed_metadata.process.as_mut().expect("process").command_line =
            Some("strawberry --changed".to_string());
        assert!(revalidate_same_process(&original, &changed_metadata, "strawberry").is_err());

        let changed_identity = WindowFacts::test_window(10, "Other");
        assert!(revalidate_same_process(&original, &changed_identity, "strawberry").is_err());
    }

    #[test]
    fn generic_quit_accepts_one_stable_application_process() {
        let window = process_window(10, "FeatherPad", 1234, "featherpad");
        let inspection = identity::inspect(10, vec![window]).expect("inspection");
        assert_eq!(generic_process_pid(&inspection).expect("safe PID"), 1234);
    }

    #[test]
    fn generic_quit_rejects_invalidated_or_mismatched_process_identity() {
        let mut invalid = process_window(10, "FeatherPad", 1234, "featherpad");
        invalid.pid_validated = false;
        invalid.pid_validation = "process unavailable".to_string();
        let inspection = identity::inspect(10, vec![invalid]).expect("inspection");
        assert!(generic_process_pid(&inspection).is_err());

        let mismatched = process_window(10, "FeatherPad", 1234, "unrelated-helper");
        let inspection = identity::inspect(10, vec![mismatched]).expect("inspection");
        assert!(generic_process_pid(&inspection).is_err());

        let mut helper = process_window(10, "FeatherPad", 1234, "featherpad");
        helper.process.as_mut().expect("process").command_line =
            Some("featherpad --type=renderer".to_string());
        let inspection = identity::inspect(10, vec![helper]).expect("inspection");
        assert!(generic_process_pid(&inspection).is_err());
    }

    #[test]
    fn generic_quit_rejects_multiple_window_pids() {
        let first = process_window(10, "FeatherPad", 1234, "featherpad");
        let second = process_window(20, "FeatherPad", 5678, "featherpad");
        let inspection = identity::inspect(10, vec![first, second]).expect("inspection");
        assert!(generic_process_pid(&inspection).is_err());
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
}
