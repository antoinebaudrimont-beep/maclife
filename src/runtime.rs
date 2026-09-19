use crate::control;
use crate::identity::{self, normalized_app_identity};
use crate::lifecycle::{
    self, CloseDecision, FocusKind, HiddenWindows, QuitMethod,
};
use crate::model::{Disposition, Inspection, Snapshot};
use crate::{report, x11, DynError};
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

fn terminate_validated_pid(inspection: &Inspection) -> Result<(), DynError> {
    let window = &inspection.identity_window;
    if !window.pid_validated {
        return Err(format!(
            "refusing SIGTERM: _NET_WM_PID is not validated ({})",
            window.pid_validation
        )
        .into());
    }
    let pid = window.pid.ok_or("validated window has no PID")?;
    if pid == std::process::id() {
        return Err("refusing to terminate MacLife itself".into());
    }
    command_status("/bin/kill", &["-TERM", &pid.to_string()])
}

fn quit_method_name(method: QuitMethod) -> &'static str {
    match method {
        QuitMethod::StrawberryMpris => "mpris-quit",
        QuitMethod::ThunarCli => "thunar--quit",
        QuitMethod::ValidatedPidTerm => "validated-pid-sigterm",
        QuitMethod::CloseEachWindow => "wm-delete-each-window",
        QuitMethod::Unsupported => "unsupported",
    }
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
        QuitMethod::StrawberryMpris => command_status(
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
        ),
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
