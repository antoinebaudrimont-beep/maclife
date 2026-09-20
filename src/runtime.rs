use crate::control;
use crate::identity::{self, normalized_app_identity};
use crate::lifecycle::{
    self, CloseDecision, FocusKind, HiddenWindows, QuitMethod,
};
use crate::model::{Disposition, Inspection, Snapshot, WindowFacts};
use crate::{process, report, x11, DynError};
use std::process::Command;
use std::thread;
use std::time::Duration;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt, GrabMode, ModMask};
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

fn action_name(decision: &CloseDecision) -> &'static str {
    match decision {
        CloseDecision::CloseFocused => "wm-delete-focused",
        CloseDecision::HideLast => "iconify-last-window",
        CloseDecision::NativeCloseLast => "native-close-last-window",
        CloseDecision::Refuse(_) => "refuse",
    }
}

fn reconcile_hidden(hidden: &mut HiddenWindows, snapshot: &Snapshot) {
    let marked: Vec<_> = snapshot
        .windows
        .iter()
        .filter(|window| window.maclife_hidden)
        .map(|window| window.xid)
        .collect();
    hidden.retain_marked(&marked);
    for window in snapshot.windows.iter().filter(|window| window.maclife_hidden) {
        hidden.remember(window.xid, normalized_app_identity(window));
    }
}

fn handle_close(
    options: RunOptions,
    hidden: &mut HiddenWindows,
) -> Result<(), DynError> {
    let (snapshot, inspection) = focused_inspection()?;
    reconcile_hidden(hidden, &snapshot);
    let decision = lifecycle::close_decision(
        &inspection.app_identity,
        inspection.meaningful_windows.len(),
        focus_kind(&inspection),
    );
    println!(
        "Cmd+W focused={} meaningful_windows={} action={} xid=0x{:08x}",
        inspection.app_identity,
        inspection.meaningful_windows.len(),
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
            hidden.remember(inspection.focused_xid, inspection.app_identity);
            Ok(())
        }
        CloseDecision::Refuse(reason) => Err(reason.into()),
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

fn terminate_validated_pid(inspection: &Inspection) -> Result<(), DynError> {
    let pid = validated_pid(&inspection.identity_window, &inspection.app_identity)?;
    command_status("/bin/kill", &["-TERM", &pid.to_string()])
}

fn revalidate_same_process(
    original: &WindowFacts,
    current: &WindowFacts,
    expected_identity: &str,
) -> Result<u32, DynError> {
    if current.xid != original.xid {
        return Err("refusing SIGTERM fallback: X11 window identity changed".into());
    }
    let original_pid = validated_pid(original, expected_identity)?;
    let current_pid = validated_pid(current, expected_identity)?;
    if current_pid != original_pid {
        return Err(format!(
            "refusing SIGTERM fallback: PID changed from {original_pid} to {current_pid}"
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
        || current_process.name != original_process.name
        || current_process.executable != original_process.executable
    {
        return Err("refusing SIGTERM fallback: process identity changed".into());
    }
    Ok(original_pid)
}

fn quit_method_name(method: QuitMethod) -> &'static str {
    match method {
        QuitMethod::StrawberryMpris => "mpris-then-validated-pid-sigterm",
        QuitMethod::ThunarCli => "thunar--quit",
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

fn handle_quit(options: RunOptions) -> Result<(), DynError> {
    let (_, inspection) = focused_inspection()?;
    let method = lifecycle::application_rule(&inspection.app_identity).quit;
    println!(
        "Cmd+Q focused={} action=application-quit method={}",
        inspection.app_identity,
        quit_method_name(method)
    );
    if options.verbose {
        print!("{}", report::render(&inspection, true));
    }
    if options.dry_run {
        return Ok(());
    }

    match method {
        QuitMethod::StrawberryMpris => quit_strawberry(&inspection),
        QuitMethod::ThunarCli => command_status("thunar", &["--quit"]),
        QuitMethod::ValidatedPidTerm => terminate_validated_pid(&inspection),
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
    }
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

    let mut hidden = HiddenWindows::default();
    if let Ok(snapshot) = x11::collect_snapshot() {
        reconcile_hidden(&mut hidden, &snapshot);
    }
    println!(
        "MacLife listening: close_keycode={} quit_keycode={} dry_run={} adopted_hidden={}",
        options.close_keycode,
        options.quit_keycode,
        options.dry_run,
        hidden.len()
    );

    loop {
        if let Event::KeyPress(event) = conn.wait_for_event()? {
            let result = if event.detail == options.close_keycode {
                handle_close(options, &mut hidden)
            } else if event.detail == options.quit_keycode {
                handle_quit(options)
            } else {
                continue;
            };
            if let Err(error) = result {
                eprintln!("MacLife action refused/failed: {error}");
            }
        }
    }
}

pub fn restore(identity: &str) -> Result<(), DynError> {
    let wanted = identity.trim().to_ascii_lowercase();
    let snapshot = x11::collect_snapshot()?;
    let matches: Vec<_> = snapshot
        .windows
        .iter()
        .filter(|window| {
            window.maclife_hidden && normalized_app_identity(window) == wanted
        })
        .collect();
    let window = match matches.as_slice() {
        [] => return Err(format!("no MacLife-hidden window for {wanted}").into()),
        [window] => *window,
        _ => {
            return Err(format!(
                "multiple MacLife-hidden windows for {wanted}; refusing ambiguous restore"
            )
            .into())
        }
    };

    control::activate_window(window.xid)?;
    thread::sleep(Duration::from_millis(500));
    let after = x11::collect_snapshot()?;
    let restored = after.windows.iter().find(|candidate| candidate.xid == window.xid);
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
            window.xid
        )
        .into());
    }
    control::clear_hidden_marker(window.xid)?;
    println!("restored={} xid=0x{:08x}", wanted, window.xid);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::revalidate_same_process;
    use crate::model::{ProcessInfo, WindowFacts};

    fn strawberry_window(xid: u32, pid: u32) -> WindowFacts {
        let mut window = WindowFacts::test_window(xid, "Strawberry");
        window.pid = Some(pid);
        window.pid_validated = true;
        window.pid_validation = format!("same-user local /proc/{pid}");
        window.process = Some(ProcessInfo {
            pid,
            uid: 1000,
            parent_pid: Some(1),
            name: "strawberry".to_string(),
            executable: Some("/usr/bin/strawberry".to_string()),
            command_line: Some("strawberry".to_string()),
        });
        window
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

        let changed_identity = WindowFacts::test_window(10, "Other");
        assert!(revalidate_same_process(&original, &changed_identity, "strawberry").is_err());
    }
}
