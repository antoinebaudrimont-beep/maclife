use crate::DynError;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ClientMessageEvent, ConnectionExt, CreateWindowAux, EventMask, PropMode,
    SelectionClearEvent, Window, WindowClass,
};
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as WrapperConnectionExt;
use x11rb::{COPY_DEPTH_FROM_PARENT, COPY_FROM_PARENT, CURRENT_TIME};

pub const LIFECYCLE_PROTOCOL_VERSION: u32 = 1;
pub const CLOSE_REQUEST_ATOM: &str = "_MACLIFE_CLOSE_REQUEST";
pub const PROTOCOL_VERSION_ATOM: &str = "_MACLIFE_LIFECYCLE_VERSION";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CloseRequest {
    pub target_xid: Window,
    pub timestamp: u32,
    pub request_id: u32,
}

pub struct LifecycleManager {
    window: Window,
    selection: Atom,
    close_request: Atom,
}

fn intern(conn: &RustConnection, name: &str) -> Result<Atom, DynError> {
    Ok(conn.intern_atom(false, name.as_bytes())?.reply()?.atom)
}

fn decode_close_request(
    event: &ClientMessageEvent,
    manager_window: Window,
    close_request_atom: Atom,
) -> Result<Option<CloseRequest>, String> {
    if event.type_ != close_request_atom {
        return Ok(None);
    }
    if event.window != manager_window {
        return Err(format!(
            "close request addressed to unexpected manager window 0x{:08x}",
            event.window
        ));
    }
    if event.format != 32 {
        return Err(format!(
            "close request uses format {}, expected 32",
            event.format
        ));
    }
    let data = event.data.as_data32();
    if data[0] != LIFECYCLE_PROTOCOL_VERSION {
        return Err(format!(
            "close request protocol version {} is incompatible with {}",
            data[0], LIFECYCLE_PROTOCOL_VERSION
        ));
    }
    if data[1] == 0 {
        return Err("close request target XID is zero".to_string());
    }
    Ok(Some(CloseRequest {
        target_xid: data[1],
        timestamp: data[2],
        request_id: data[3],
    }))
}

impl LifecycleManager {
    pub fn claim(
        conn: &RustConnection,
        screen_number: usize,
        root: Window,
    ) -> Result<Self, DynError> {
        let selection_name = format!("_MACLIFE_LIFECYCLE_MANAGER_S{screen_number}");
        let selection = intern(conn, &selection_name)?;
        let existing = conn.get_selection_owner(selection)?.reply()?.owner;
        if existing != 0 {
            return Err(format!(
                "private lifecycle selection {selection_name} is already owned by 0x{existing:08x}"
            )
            .into());
        }

        let window = conn.generate_id()?;
        conn.create_window(
            COPY_DEPTH_FROM_PARENT,
            window,
            root,
            -1,
            -1,
            1,
            1,
            0,
            WindowClass::INPUT_ONLY,
            COPY_FROM_PARENT,
            &CreateWindowAux::new().event_mask(EventMask::PROPERTY_CHANGE),
        )?
        .check()?;

        let version_atom = intern(conn, PROTOCOL_VERSION_ATOM)?;
        let close_request = intern(conn, CLOSE_REQUEST_ATOM)?;
        conn.change_property32(
            PropMode::REPLACE,
            window,
            version_atom,
            AtomEnum::CARDINAL,
            &[LIFECYCLE_PROTOCOL_VERSION],
        )?
        .check()?;
        conn.set_selection_owner(window, selection, CURRENT_TIME)?
            .check()?;
        conn.flush()?;

        let owner = conn.get_selection_owner(selection)?.reply()?.owner;
        if owner != window {
            return Err(format!(
                "could not acquire private lifecycle selection {selection_name}"
            )
            .into());
        }

        let manager_atom = intern(conn, "MANAGER")?;
        let announcement = ClientMessageEvent::new(
            32,
            root,
            manager_atom,
            [CURRENT_TIME, selection, window, LIFECYCLE_PROTOCOL_VERSION, 0],
        );
        conn.send_event(false, root, EventMask::STRUCTURE_NOTIFY, announcement)?
            .check()?;
        conn.flush()?;

        Ok(Self {
            window,
            selection,
            close_request,
        })
    }

    pub fn decode(&self, event: &ClientMessageEvent) -> Result<Option<CloseRequest>, String> {
        decode_close_request(event, self.window, self.close_request)
    }

    pub fn lost_selection(&self, event: &SelectionClearEvent) -> bool {
        event.owner == self.window && event.selection == self.selection
    }

    pub fn window(&self) -> Window {
        self.window
    }
}

#[cfg(test)]
mod tests {
    use super::{
        decode_close_request, CloseRequest, LIFECYCLE_PROTOCOL_VERSION,
    };
    use x11rb::protocol::xproto::ClientMessageEvent;

    const MANAGER: u32 = 0x100;
    const REQUEST_ATOM: u32 = 0x200;

    fn request(data: [u32; 5]) -> ClientMessageEvent {
        ClientMessageEvent::new(32, MANAGER, REQUEST_ATOM, data)
    }

    #[test]
    fn decodes_exact_target_xid_without_focus_substitution() {
        let event = request([LIFECYCLE_PROTOCOL_VERSION, 0xfeed, 1234, 77, 0]);
        assert_eq!(
            decode_close_request(&event, MANAGER, REQUEST_ATOM).expect("decode"),
            Some(CloseRequest {
                target_xid: 0xfeed,
                timestamp: 1234,
                request_id: 77,
            })
        );
    }

    #[test]
    fn ignores_unrelated_client_messages() {
        let event = ClientMessageEvent::new(
            32,
            MANAGER,
            REQUEST_ATOM + 1,
            [LIFECYCLE_PROTOCOL_VERSION, 0xfeed, 0, 1, 0],
        );
        assert_eq!(
            decode_close_request(&event, MANAGER, REQUEST_ATOM).expect("decode"),
            None
        );
    }

    #[test]
    fn rejects_version_mismatch_zero_target_and_wrong_manager() {
        let mismatch = request([LIFECYCLE_PROTOCOL_VERSION + 1, 0xfeed, 0, 1, 0]);
        assert!(decode_close_request(&mismatch, MANAGER, REQUEST_ATOM).is_err());

        let zero = request([LIFECYCLE_PROTOCOL_VERSION, 0, 0, 1, 0]);
        assert!(decode_close_request(&zero, MANAGER, REQUEST_ATOM).is_err());

        let wrong_manager = ClientMessageEvent::new(
            32,
            MANAGER + 1,
            REQUEST_ATOM,
            [LIFECYCLE_PROTOCOL_VERSION, 0xfeed, 0, 1, 0],
        );
        assert!(decode_close_request(&wrong_manager, MANAGER, REQUEST_ATOM).is_err());
    }
}
