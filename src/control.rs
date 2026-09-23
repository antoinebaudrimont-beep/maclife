use crate::DynError;
use std::thread;
use std::time::Duration;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ClientMessageEvent, ConnectionExt, EventMask, PropMode, Window,
    KEY_PRESS_EVENT, KEY_RELEASE_EVENT,
};
use x11rb::protocol::xtest::ConnectionExt as XTestConnectionExt;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as WrapperConnectionExt;
use x11rb::CURRENT_TIME;

const ICONIC_STATE: u32 = 3;
const SOURCE_APPLICATION: u32 = 1;
const XK_CONTROL_L: u32 = 0xffe3;
const XK_SHIFT_L: u32 = 0xffe1;
const XK_W: u32 = 0x0077;

fn connect_root() -> Result<(RustConnection, Window), DynError> {
    let (conn, screen_number) = x11rb::connect(None)?;
    let root = conn
        .setup()
        .roots
        .get(screen_number)
        .ok_or("X11 screen number is out of range")?
        .root;
    Ok((conn, root))
}

fn atom(conn: &RustConnection, name: &str, only_if_exists: bool) -> Result<Atom, DynError> {
    let atom = conn
        .intern_atom(only_if_exists, name.as_bytes())?
        .reply()?
        .atom;
    if atom == 0 {
        return Err(format!("required X11 atom {name} does not exist").into());
    }
    Ok(atom)
}

pub fn request_close(window: Window) -> Result<(), DynError> {
    let (conn, _) = connect_root()?;
    let wm_protocols = atom(&conn, "WM_PROTOCOLS", true)?;
    let wm_delete = atom(&conn, "WM_DELETE_WINDOW", true)?;
    let protocols = conn
        .get_property(
            false,
            window,
            wm_protocols,
            AtomEnum::ATOM,
            0,
            1024,
        )?
        .reply()?;
    let supports_delete = protocols
        .value32()
        .is_some_and(|mut values| values.any(|value| value == wm_delete));
    if !supports_delete {
        return Err(format!("window 0x{window:08x} does not advertise WM_DELETE_WINDOW").into());
    }

    let event = ClientMessageEvent::new(
        32,
        window,
        wm_protocols,
        [wm_delete, CURRENT_TIME, 0, 0, 0],
    );
    conn.send_event(false, window, EventMask::NO_EVENT, event)?
        .check()?;
    conn.flush()?;
    Ok(())
}

pub fn iconify_and_mark(window: Window) -> Result<(), DynError> {
    let (conn, root) = connect_root()?;
    let wm_change_state = atom(&conn, "WM_CHANGE_STATE", false)?;
    let marker = atom(&conn, "_MACLIFE_HIDDEN", false)?;

    conn.change_property32(
        PropMode::REPLACE,
        window,
        marker,
        AtomEnum::CARDINAL,
        &[1],
    )?
    .check()?;

    let event = ClientMessageEvent::new(
        32,
        window,
        wm_change_state,
        [ICONIC_STATE, 0, 0, 0, 0],
    );
    let mask = EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY;
    if let Err(error) = conn.send_event(false, root, mask, event)?.check() {
        let _ = conn.delete_property(window, marker)?.check();
        return Err(error.into());
    }
    conn.flush()?;
    Ok(())
}

pub fn activate_window(window: Window) -> Result<(), DynError> {
    let (conn, root) = connect_root()?;
    let active = atom(&conn, "_NET_ACTIVE_WINDOW", true)?;
    let event = ClientMessageEvent::new(
        32,
        window,
        active,
        [SOURCE_APPLICATION, CURRENT_TIME, 0, 0, 0],
    );
    let mask = EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY;
    conn.send_event(false, root, mask, event)?.check()?;
    conn.flush()?;
    Ok(())
}

pub fn activate_window_and_wait(window: Window) -> Result<(), DynError> {
    activate_window(window)?;
    let (conn, root) = connect_root()?;
    let active_atom = atom(&conn, "_NET_ACTIVE_WINDOW", true)?;
    for _ in 0..20 {
        if active_window(&conn, root, active_atom)? == window {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(25));
    }
    Err(format!("window 0x{window:08x} did not become active").into())
}

pub fn clear_hidden_marker(window: Window) -> Result<(), DynError> {
    let (conn, _) = connect_root()?;
    let marker = atom(&conn, "_MACLIFE_HIDDEN", true)?;
    conn.delete_property(window, marker)?.check()?;
    conn.flush()?;
    Ok(())
}

fn active_window(
    conn: &RustConnection,
    root: Window,
    active_atom: Atom,
) -> Result<Window, DynError> {
    let reply = conn
        .get_property(false, root, active_atom, AtomEnum::WINDOW, 0, 1)?
        .reply()?;
    Ok(reply
        .value32()
        .and_then(|mut values| values.next())
        .unwrap_or_default())
}

fn keycode_for_keysym(conn: &RustConnection, keysym: u32) -> Result<u8, DynError> {
    let setup = conn.setup();
    let count = setup
        .max_keycode
        .checked_sub(setup.min_keycode)
        .and_then(|value| value.checked_add(1))
        .ok_or("invalid X11 keycode range")?;
    let mapping = conn
        .get_keyboard_mapping(setup.min_keycode, count)?
        .reply()?;
    let width = usize::from(mapping.keysyms_per_keycode);
    if width == 0 {
        return Err("X11 keyboard mapping has zero keysyms per keycode".into());
    }
    mapping
        .keysyms
        .chunks(width)
        .position(|symbols| symbols.contains(&keysym))
        .and_then(|index| setup.min_keycode.checked_add(index as u8))
        .ok_or_else(|| format!("X11 keyboard mapping has no keysym 0x{keysym:x}").into())
}

fn send_shortcut(window: Window, keysym: u32, shift: bool) -> Result<(), DynError> {
    let (conn, root) = connect_root()?;
    let active_atom = atom(&conn, "_NET_ACTIVE_WINDOW", true)?;
    let active = active_window(&conn, root, active_atom)?;
    if active != window {
        return Err(format!(
            "focused X11 window changed before native shortcut: expected 0x{window:08x}, found 0x{active:08x}"
        )
        .into());
    }

    let control = keycode_for_keysym(&conn, XK_CONTROL_L)?;
    let shift_key = shift.then(|| keycode_for_keysym(&conn, XK_SHIFT_L)).transpose()?;
    let key = keycode_for_keysym(&conn, keysym)?;
    conn.xtest_fake_input(KEY_PRESS_EVENT, control, CURRENT_TIME, root, 0, 0, 0)?
        .check()?;
    if let Some(shift_key) = shift_key {
        conn.xtest_fake_input(KEY_PRESS_EVENT, shift_key, CURRENT_TIME, root, 0, 0, 0)?
            .check()?;
    }
    conn.xtest_fake_input(KEY_PRESS_EVENT, key, CURRENT_TIME, root, 0, 0, 0)?
        .check()?;
    conn.xtest_fake_input(KEY_RELEASE_EVENT, key, CURRENT_TIME, root, 0, 0, 0)?
        .check()?;
    if let Some(shift_key) = shift_key {
        conn.xtest_fake_input(KEY_RELEASE_EVENT, shift_key, CURRENT_TIME, root, 0, 0, 0)?
            .check()?;
    }
    conn.xtest_fake_input(KEY_RELEASE_EVENT, control, CURRENT_TIME, root, 0, 0, 0)?
        .check()?;
    conn.flush()?;
    Ok(())
}

pub fn close_active_document(window: Window) -> Result<(), DynError> {
    send_shortcut(window, XK_W, false)
}

pub fn close_brave_top_level(window: Window) -> Result<(), DynError> {
    send_shortcut(window, XK_W, true)
}
