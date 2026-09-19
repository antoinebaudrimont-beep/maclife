use crate::DynError;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ClientMessageEvent, ConnectionExt, EventMask, PropMode, Window,
};
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as WrapperConnectionExt;
use x11rb::CURRENT_TIME;

const ICONIC_STATE: u32 = 3;
const SOURCE_APPLICATION: u32 = 1;

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

pub fn clear_hidden_marker(window: Window) -> Result<(), DynError> {
    let (conn, _) = connect_root()?;
    let marker = atom(&conn, "_MACLIFE_HIDDEN", true)?;
    conn.delete_property(window, marker)?.check()?;
    conn.flush()?;
    Ok(())
}
