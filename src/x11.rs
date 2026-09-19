use crate::model::{Snapshot, WindowFacts, WmClass};
use crate::process;
use crate::DynError;
use std::collections::HashSet;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ConnectionExt, GetPropertyReply, MapState, Window,
};
use x11rb::rust_connection::RustConnection;

#[allow(non_snake_case)]
struct Atoms {
    NET_ACTIVE_WINDOW: Atom,
    NET_CLIENT_LIST: Atom,
    NET_CLIENT_LIST_STACKING: Atom,
    NET_WM_NAME: Atom,
    UTF8_STRING: Atom,
    NET_WM_PID: Atom,
    WM_CLIENT_LEADER: Atom,
    WM_TRANSIENT_FOR: Atom,
    WM_CLIENT_MACHINE: Atom,
    NET_WM_WINDOW_TYPE: Atom,
    NET_WM_STATE: Atom,
    type_atoms: Vec<(Atom, &'static str)>,
    state_atoms: Vec<(Atom, &'static str)>,
}

fn existing_atom(conn: &RustConnection, name: &'static str) -> Result<Atom, DynError> {
    Ok(conn.intern_atom(true, name.as_bytes())?.reply()?.atom)
}

impl Atoms {
    fn load(conn: &RustConnection) -> Result<Self, DynError> {
        let type_names = [
            "_NET_WM_WINDOW_TYPE_DESKTOP",
            "_NET_WM_WINDOW_TYPE_DOCK",
            "_NET_WM_WINDOW_TYPE_TOOLBAR",
            "_NET_WM_WINDOW_TYPE_MENU",
            "_NET_WM_WINDOW_TYPE_UTILITY",
            "_NET_WM_WINDOW_TYPE_SPLASH",
            "_NET_WM_WINDOW_TYPE_DIALOG",
            "_NET_WM_WINDOW_TYPE_NORMAL",
            "_NET_WM_WINDOW_TYPE_DROPDOWN_MENU",
            "_NET_WM_WINDOW_TYPE_POPUP_MENU",
            "_NET_WM_WINDOW_TYPE_TOOLTIP",
            "_NET_WM_WINDOW_TYPE_NOTIFICATION",
            "_NET_WM_WINDOW_TYPE_COMBO",
            "_NET_WM_WINDOW_TYPE_DND",
        ];
        let state_names = [
            "_NET_WM_STATE_MODAL",
            "_NET_WM_STATE_STICKY",
            "_NET_WM_STATE_MAXIMIZED_VERT",
            "_NET_WM_STATE_MAXIMIZED_HORZ",
            "_NET_WM_STATE_SHADED",
            "_NET_WM_STATE_SKIP_TASKBAR",
            "_NET_WM_STATE_SKIP_PAGER",
            "_NET_WM_STATE_HIDDEN",
            "_NET_WM_STATE_FULLSCREEN",
            "_NET_WM_STATE_ABOVE",
            "_NET_WM_STATE_BELOW",
            "_NET_WM_STATE_DEMANDS_ATTENTION",
            "_NET_WM_STATE_FOCUSED",
        ];
        let mut type_atoms = Vec::new();
        for name in type_names {
            let atom = existing_atom(conn, name)?;
            if atom != 0 {
                type_atoms.push((atom, name));
            }
        }
        let mut state_atoms = Vec::new();
        for name in state_names {
            let atom = existing_atom(conn, name)?;
            if atom != 0 {
                state_atoms.push((atom, name));
            }
        }

        Ok(Self {
            NET_ACTIVE_WINDOW: existing_atom(conn, "_NET_ACTIVE_WINDOW")?,
            NET_CLIENT_LIST: existing_atom(conn, "_NET_CLIENT_LIST")?,
            NET_CLIENT_LIST_STACKING: existing_atom(conn, "_NET_CLIENT_LIST_STACKING")?,
            NET_WM_NAME: existing_atom(conn, "_NET_WM_NAME")?,
            UTF8_STRING: existing_atom(conn, "UTF8_STRING")?,
            NET_WM_PID: existing_atom(conn, "_NET_WM_PID")?,
            WM_CLIENT_LEADER: existing_atom(conn, "WM_CLIENT_LEADER")?,
            WM_TRANSIENT_FOR: existing_atom(conn, "WM_TRANSIENT_FOR")?,
            WM_CLIENT_MACHINE: existing_atom(conn, "WM_CLIENT_MACHINE")?,
            NET_WM_WINDOW_TYPE: existing_atom(conn, "_NET_WM_WINDOW_TYPE")?,
            NET_WM_STATE: existing_atom(conn, "_NET_WM_STATE")?,
            type_atoms,
            state_atoms,
        })
    }
}

struct Collector {
    conn: RustConnection,
    root: Window,
    atoms: Atoms,
    self_uid: Option<u32>,
    hostname: Option<String>,
}

impl Collector {
    fn property(&self, window: Window, property: Atom, kind: Atom) -> Option<GetPropertyReply> {
        if property == 0 {
            return None;
        }
        self.conn
            .get_property(false, window, property, kind, 0, 4096)
            .ok()?
            .reply()
            .ok()
    }

    fn property32(&self, window: Window, property: Atom) -> Vec<u32> {
        self.property(window, property, AtomEnum::ANY.into())
            .and_then(|reply| reply.value32().map(Iterator::collect))
            .unwrap_or_default()
    }

    fn text_property(&self, window: Window, property: Atom, kind: Atom) -> Option<String> {
        let reply = self.property(window, property, kind)?;
        let bytes: Vec<_> = reply.value8()?.collect();
        let text = String::from_utf8_lossy(&bytes)
            .trim_end_matches('\0')
            .trim()
            .to_string();
        (!text.is_empty()).then_some(text)
    }

    fn wm_class(&self, window: Window) -> Option<WmClass> {
        let reply = self.property(window, AtomEnum::WM_CLASS.into(), AtomEnum::STRING.into())?;
        let mut fields = reply.value.split(|byte| *byte == 0);
        let instance = String::from_utf8_lossy(fields.next().unwrap_or_default()).into_owned();
        let class = String::from_utf8_lossy(fields.next().unwrap_or_default()).into_owned();
        if instance.is_empty() && class.is_empty() {
            None
        } else {
            Some(WmClass { instance, class })
        }
    }

    fn atom_names(&self, values: Vec<u32>, known: &[(Atom, &'static str)]) -> Vec<String> {
        values
            .into_iter()
            .map(|value| {
                known
                    .iter()
                    .find_map(|(atom, name)| (*atom == value).then_some((*name).to_string()))
                    .unwrap_or_else(|| format!("ATOM({value})"))
            })
            .collect()
    }

    fn validate_pid(
        &self,
        pid: Option<u32>,
        machine: Option<&str>,
    ) -> (bool, String, Option<crate::model::ProcessInfo>) {
        let Some(pid) = pid.filter(|pid| *pid > 0) else {
            return (false, "_NET_WM_PID missing".to_string(), None);
        };
        if let (Some(machine), Some(hostname)) = (machine, self.hostname.as_deref()) {
            if !machine.eq_ignore_ascii_case(hostname)
                && !machine.eq_ignore_ascii_case("localhost")
                && !machine.eq_ignore_ascii_case("localhost.localdomain")
            {
                return (
                    false,
                    format!("WM_CLIENT_MACHINE {machine:?} is not local"),
                    None,
                );
            }
        }
        let Some(info) = process::read_process(pid) else {
            return (false, format!("/proc/{pid} is unavailable"), None);
        };
        let Some(self_uid) = self.self_uid else {
            return (false, "could not determine current uid".to_string(), Some(info));
        };
        if info.uid != self_uid {
            let uid = info.uid;
            return (
                false,
                format!("process uid {uid} differs from session uid {self_uid}"),
                Some(info),
            );
        }
        (true, format!("same-user local /proc/{pid}"), Some(info))
    }

    fn collect_window(&self, xid: Window) -> Option<WindowFacts> {
        let attributes = self.conn.get_window_attributes(xid).ok()?.reply().ok()?;
        let title = self
            .text_property(xid, self.atoms.NET_WM_NAME, self.atoms.UTF8_STRING)
            .or_else(|| {
                self.text_property(xid, AtomEnum::WM_NAME.into(), AtomEnum::STRING.into())
            });
        let pid = self
            .property32(xid, self.atoms.NET_WM_PID)
            .into_iter()
            .next();
        let client_machine = self.text_property(
            xid,
            self.atoms.WM_CLIENT_MACHINE,
            AtomEnum::STRING.into(),
        );
        let (pid_validated, pid_validation, process) =
            self.validate_pid(pid, client_machine.as_deref());
        let client_leader = self
            .property32(xid, self.atoms.WM_CLIENT_LEADER)
            .into_iter()
            .next()
            .filter(|value| *value != 0);
        let transient_for = self
            .property32(xid, self.atoms.WM_TRANSIENT_FOR)
            .into_iter()
            .next()
            .filter(|value| *value != 0);
        let window_types = self.atom_names(
            self.property32(xid, self.atoms.NET_WM_WINDOW_TYPE),
            &self.atoms.type_atoms,
        );
        let states = self.atom_names(
            self.property32(xid, self.atoms.NET_WM_STATE),
            &self.atoms.state_atoms,
        );

        Some(WindowFacts {
            xid,
            title,
            wm_class: self.wm_class(xid),
            pid,
            pid_validated,
            pid_validation,
            process,
            client_machine,
            client_leader,
            transient_for,
            window_types,
            states,
            override_redirect: attributes.override_redirect,
            mapped: attributes.map_state != MapState::UNMAPPED,
        })
    }

    fn snapshot(&self) -> Result<Snapshot, DynError> {
        let active_window = self
            .property32(self.root, self.atoms.NET_ACTIVE_WINDOW)
            .into_iter()
            .next()
            .unwrap_or_default();
        let mut xids = self.property32(self.root, self.atoms.NET_CLIENT_LIST_STACKING);
        if xids.is_empty() {
            xids = self.property32(self.root, self.atoms.NET_CLIENT_LIST);
        }
        if active_window != 0 && !xids.contains(&active_window) {
            xids.push(active_window);
        }
        let mut seen = HashSet::new();
        let windows = xids
            .into_iter()
            .filter(|xid| seen.insert(*xid))
            .filter_map(|xid| self.collect_window(xid))
            .collect();
        Ok(Snapshot {
            active_window,
            windows,
        })
    }
}

pub fn collect_snapshot() -> Result<Snapshot, DynError> {
    let (conn, screen_number) = x11rb::connect(None)?;
    let root = conn
        .setup()
        .roots
        .get(screen_number)
        .ok_or("X11 screen number is out of range")?
        .root;
    let atoms = Atoms::load(&conn)?;
    let collector = Collector {
        conn,
        root,
        atoms,
        self_uid: process::current_uid(),
        hostname: process::hostname(),
    };
    collector.snapshot()
}
