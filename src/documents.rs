use crate::model::{Inspection, WindowFacts, WindowGeometry};
use crate::process;
use std::collections::{HashMap, VecDeque};
use zbus::blocking::{
    connection::Builder as ConnectionBuilder, proxy::Builder as ProxyBuilder, Connection, Proxy,
};
use zbus::proxy::CacheProperties;
use zbus::zvariant::OwnedObjectPath;

const REGISTRY_DESTINATION: &str = "org.a11y.atspi.Registry";
const REGISTRY_ROOT: &str = "/org/a11y/atspi/accessible/root";
const ACCESSIBLE_INTERFACE: &str = "org.a11y.atspi.Accessible";
const COMPONENT_INTERFACE: &str = "org.a11y.atspi.Component";
const ACTION_INTERFACE: &str = "org.a11y.atspi.Action";
const SCREEN_COORDINATES: u32 = 0;
const SELECTED_STATE_BIT: u32 = 1 << 23;
const DISCOVERY_NODE_LIMIT: usize = 192;
const DISCOVERY_DEPTH_LIMIT: usize = 7;
const UNKNOWN_METHOD_ERROR: &str = "org.freedesktop.DBus.Error.UnknownMethod";

type AccessibleRef = (String, OwnedObjectPath);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InternalDocumentState {
    Known {
        count: usize,
        selected: String,
        minimum_persistent: usize,
        association_proof: String,
    },
    BaseOnly {
        minimum_persistent: usize,
        association_proof: String,
    },
    Unknown {
        reason: String,
    },
}

impl InternalDocumentState {
    pub fn concise_summary(&self) -> String {
        match self {
            Self::Known {
                count,
                minimum_persistent,
                ..
            } => format!(
                "state=known count={count} minimum_persistent={minimum_persistent} selected=true"
            ),
            Self::BaseOnly {
                minimum_persistent,
                ..
            } => format!("state=base-only minimum_persistent={minimum_persistent}"),
            Self::Unknown { reason } => format!("state=unknown reason={reason}"),
        }
    }

    pub fn summary(&self) -> String {
        match self {
            Self::Known {
                count,
                selected,
                minimum_persistent,
                association_proof,
            } => format!(
                "known count={count} selected={selected:?} minimum_persistent={minimum_persistent} association={association_proof}"
            ),
            Self::BaseOnly {
                minimum_persistent,
                association_proof,
            } => format!(
                "base-only minimum_persistent={minimum_persistent} association={association_proof}"
            ),
            Self::Unknown { reason } => format!("unknown reason={reason}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DocumentCloseDecision {
    CloseActiveDocument,
    PreserveTopLevelWindow,
    Refuse(String),
}

pub fn close_decision(state: &InternalDocumentState) -> DocumentCloseDecision {
    match state {
        InternalDocumentState::Known {
            count,
            minimum_persistent,
            ..
        } if count > minimum_persistent => DocumentCloseDecision::CloseActiveDocument,
        InternalDocumentState::Known { .. } | InternalDocumentState::BaseOnly { .. } => {
            DocumentCloseDecision::PreserveTopLevelWindow
        }
        InternalDocumentState::Unknown { reason } => {
            DocumentCloseDecision::Refuse(reason.clone())
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseDocumentMethod {
    ControlW,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseTopLevelMethod {
    ControlShiftW,
    WmDelete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProviderKind {
    Brave,
    Thunderbird,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DocumentLifecycleAdapter {
    pub identity: &'static str,
    application_name: &'static str,
    kind: ProviderKind,
    pub minimum_persistent: usize,
    pub close_document: CloseDocumentMethod,
    pub close_top_level: CloseTopLevelMethod,
}

const ADAPTERS: &[DocumentLifecycleAdapter] = &[
    DocumentLifecycleAdapter {
        identity: "brave-origin",
        application_name: "Brave Origin",
        kind: ProviderKind::Brave,
        minimum_persistent: 1,
        close_document: CloseDocumentMethod::ControlW,
        close_top_level: CloseTopLevelMethod::ControlShiftW,
    },
    DocumentLifecycleAdapter {
        identity: "brave-browser",
        application_name: "Brave Browser",
        kind: ProviderKind::Brave,
        minimum_persistent: 1,
        close_document: CloseDocumentMethod::ControlW,
        close_top_level: CloseTopLevelMethod::ControlShiftW,
    },
    DocumentLifecycleAdapter {
        identity: "thunderbird-default",
        application_name: "Thunderbird",
        kind: ProviderKind::Thunderbird,
        minimum_persistent: 1,
        close_document: CloseDocumentMethod::ControlW,
        close_top_level: CloseTopLevelMethod::WmDelete,
    },
];

pub fn adapter_for(identity: &str) -> Option<&'static DocumentLifecycleAdapter> {
    ADAPTERS.iter().find(|adapter| adapter.identity == identity)
}

pub fn is_thunderbird_compose_window(inspection: &Inspection) -> bool {
    inspection.app_identity == "thunderbird-default"
        && inspection.focused_window.wm_class.as_ref().is_some_and(|class| {
            class.instance.eq_ignore_ascii_case("Msgcompose")
                && class.class.eq_ignore_ascii_case("thunderbird-default")
        })
}

pub fn launch_opt_in_status(inspection: &Inspection) -> &'static str {
    let window = &inspection.identity_window;
    if !window.pid_validated {
        return "unavailable";
    }
    match inspection.app_identity.as_str() {
        "brave-origin" | "brave-browser" => window
            .process
            .as_ref()
            .and_then(|process| process.command_line.as_deref())
            .map_or("unavailable", |command_line| {
                if command_line
                    .split_whitespace()
                    .any(|argument| argument == "--force-renderer-accessibility=basic")
                {
                    "yes"
                } else {
                    "no"
                }
            }),
        "thunderbird-default" => {
            let Some(pid) = window.pid else {
                return "unavailable";
            };
            let Some(environment) = process::read_environment(pid) else {
                return "unavailable";
            };
            if environment
                .iter()
                .any(|(name, value)| name == "GNOME_ACCESSIBILITY" && value == "1")
            {
                "yes"
            } else {
                "no"
            }
        }
        _ => "not-applicable",
    }
}

fn is_null_accessible_path(path: &str) -> bool {
    matches!(
        path,
        "/org/a11y/atspi/null" | "/org/a11y/atspi/accessible/null"
    )
}

fn is_skippable_discovery_error(path: &str, error_name: Option<&str>) -> bool {
    is_null_accessible_path(path) || error_name == Some(UNKNOWN_METHOD_ERROR)
}

fn method_error_name(error: &zbus::Error) -> Option<&str> {
    match error {
        zbus::Error::MethodError(name, _, _) => Some(name.as_str()),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FrameCandidate {
    destination: String,
    path: String,
    title: String,
    geometry: WindowGeometry,
}

#[derive(Clone, Debug)]
struct CachedFrame {
    destination: String,
    frame_path: String,
    tab_list_path: Option<String>,
    last_count: Option<usize>,
    selected_path: Option<String>,
    association_proof: String,
}

#[derive(Default)]
struct ProviderCache {
    frames: HashMap<u32, CachedFrame>,
}

impl ProviderCache {
    fn retain_windows(&mut self, windows: &[WindowFacts]) {
        self.frames
            .retain(|xid, _| windows.iter().any(|window| window.xid == *xid));
    }

    fn invalidate(&mut self, xid: u32) {
        self.frames.remove(&xid);
    }
}

pub trait InternalDocumentProvider {
    fn inspect(&mut self, inspection: &Inspection) -> InternalDocumentState;
    fn retain_windows(&mut self, windows: &[WindowFacts]);
    fn invalidate(&mut self, xid: u32);
}

pub struct AtspiDocumentProvider {
    connection: Option<Connection>,
    connection_error: Option<String>,
    cache: ProviderCache,
}

impl AtspiDocumentProvider {
    pub fn connect() -> Self {
        match connect_atspi() {
            Ok(connection) => Self {
                connection: Some(connection),
                connection_error: None,
                cache: ProviderCache::default(),
            },
            Err(error) => Self {
                connection: None,
                connection_error: Some(error),
                cache: ProviderCache::default(),
            },
        }
    }

    pub fn availability(&self) -> String {
        self.connection_error
            .as_ref()
            .map_or_else(|| "available".to_string(), |error| format!("unavailable: {error}"))
    }

    fn connection(&self) -> Result<&Connection, String> {
        self.connection.as_ref().ok_or_else(|| {
            format!(
                "AT-SPI unavailable: {}; relaunch with the managed accessibility launcher",
                self.connection_error.as_deref().unwrap_or("connection failed")
            )
        })
    }

    fn proxy<'a>(
        &'a self,
        destination: &'a str,
        path: &'a str,
        interface: &'static str,
    ) -> Result<Proxy<'a>, String> {
        ProxyBuilder::<Proxy<'a>>::new(self.connection()?)
            .destination(destination)
            .map_err(|error| error.to_string())?
            .path(path)
            .map_err(|error| error.to_string())?
            .interface(interface)
            .map_err(|error| error.to_string())?
            .cache_properties(CacheProperties::No)
            .build()
            .map_err(|error| error.to_string())
    }

    fn children(&self, destination: &str, path: &str) -> Result<Vec<AccessibleRef>, String> {
        self.proxy(destination, path, ACCESSIBLE_INTERFACE)?
            .call("GetChildren", &())
            .map_err(|error| error.to_string())
    }

    fn role(&self, destination: &str, path: &str) -> Result<String, String> {
        self.proxy(destination, path, ACCESSIBLE_INTERFACE)?
            .call("GetRoleName", &())
            .map_err(|error| error.to_string())
    }

    fn discovery_role(&self, destination: &str, path: &str) -> Result<Option<String>, String> {
        if is_null_accessible_path(path) {
            return Ok(None);
        }
        match self
            .proxy(destination, path, ACCESSIBLE_INTERFACE)?
            .call("GetRoleName", &())
        {
            Ok(role) => Ok(Some(role)),
            Err(error)
                if is_skippable_discovery_error(path, method_error_name(&error)) =>
            {
                Ok(None)
            }
            Err(error) => Err(error.to_string()),
        }
    }

    fn name(&self, destination: &str, path: &str) -> Result<String, String> {
        self.proxy(destination, path, ACCESSIBLE_INTERFACE)?
            .get_property("Name")
            .map_err(|error| error.to_string())
    }

    fn states(&self, destination: &str, path: &str) -> Result<Vec<u32>, String> {
        self.proxy(destination, path, ACCESSIBLE_INTERFACE)?
            .call("GetState", &())
            .map_err(|error| error.to_string())
    }

    fn extents(&self, destination: &str, path: &str) -> Result<WindowGeometry, String> {
        let (x, y, width, height): (i32, i32, i32, i32) = self
            .proxy(destination, path, COMPONENT_INTERFACE)?
            .call("GetExtents", &(SCREEN_COORDINATES,))
            .map_err(|error| error.to_string())?;
        if width <= 0 || height <= 0 || width > i32::from(u16::MAX) || height > i32::from(u16::MAX)
        {
            return Err(format!("invalid AT-SPI frame geometry {x},{y} {width}x{height}"));
        }
        Ok(WindowGeometry {
            x,
            y,
            width: width as u16,
            height: height as u16,
        })
    }

    fn action_name(&self, destination: &str, path: &str, index: i32) -> Result<String, String> {
        self.proxy(destination, path, ACTION_INTERFACE)?
            .call("GetName", &(index,))
            .map_err(|error| error.to_string())
    }

    fn action_count(&self, destination: &str, path: &str) -> Result<i32, String> {
        self.proxy(destination, path, ACTION_INTERFACE)?
            .get_property("NActions")
            .map_err(|error| error.to_string())
    }

    fn action_binding(&self, destination: &str, path: &str, index: i32) -> Result<String, String> {
        self.proxy(destination, path, ACTION_INTERFACE)?
            .call("GetKeyBinding", &(index,))
            .map_err(|error| error.to_string())
    }

    fn do_action(&self, destination: &str, path: &str, index: i32) -> Result<(), String> {
        let performed: bool = self
            .proxy(destination, path, ACTION_INTERFACE)?
            .call("DoAction", &(index,))
            .map_err(|error| error.to_string())?;
        if performed {
            Ok(())
        } else {
            Err(format!(
                "AT-SPI action {index} was refused by {destination}{path}"
            ))
        }
    }

    fn named_descendants(
        &self,
        destination: &str,
        root_path: &str,
        wanted_role: &str,
        wanted_name: &str,
    ) -> Result<Vec<AccessibleRef>, String> {
        let mut pending = VecDeque::from([(destination.to_string(), root_path.to_string(), 0)]);
        let mut visited = 0_usize;
        let mut matches = Vec::new();
        while let Some((node_destination, node_path, depth)) = pending.pop_front() {
            visited += 1;
            if visited > DISCOVERY_NODE_LIMIT {
                return Err("bounded AT-SPI action discovery exceeded its node limit".into());
            }
            let Some(role) = self.discovery_role(&node_destination, &node_path)? else {
                continue;
            };
            let name = self.name(&node_destination, &node_path)?;
            if role == wanted_role && name == wanted_name {
                matches.push((node_destination.clone(), node_path.clone().try_into().map_err(
                    |error: zbus::zvariant::Error| error.to_string(),
                )?));
            }
            if depth >= DISCOVERY_DEPTH_LIMIT || !is_action_discovery_container(&role) {
                continue;
            }
            for (child_destination, child_path) in
                self.children(&node_destination, &node_path)?
            {
                pending.push_back((child_destination, child_path.to_string(), depth + 1));
            }
        }
        Ok(matches)
    }

    fn unique_named_descendant(
        &self,
        destination: &str,
        root_path: &str,
        role: &str,
        name: &str,
    ) -> Result<AccessibleRef, String> {
        let matches = self.named_descendants(destination, root_path, role, name)?;
        match matches.as_slice() {
            [single] => Ok(single.clone()),
            [] => Err(format!("AT-SPI {role} {name:?} is unavailable")),
            _ => Err(format!(
                "AT-SPI {role} {name:?} is ambiguous: {} matches",
                matches.len()
            )),
        }
    }

    fn application_roots(&self, application_name: &str) -> Result<Vec<AccessibleRef>, String> {
        let mut matches = Vec::new();
        for (destination, path) in self.children(REGISTRY_DESTINATION, REGISTRY_ROOT)? {
            let path_string = path.to_string();
            if self.name(&destination, &path_string).as_deref() == Ok(application_name) {
                matches.push((destination, path));
            }
        }
        Ok(matches)
    }

    fn frame_candidates(
        &self,
        application_name: &str,
    ) -> Result<Vec<FrameCandidate>, String> {
        let mut frames = Vec::new();
        for (destination, application_path) in self.application_roots(application_name)? {
            for (child_destination, child_path) in
                self.children(&destination, application_path.as_str())?
            {
                let child_path = child_path.to_string();
                if self
                    .discovery_role(&child_destination, &child_path)?
                    .as_deref()
                    != Some("frame")
                {
                    continue;
                }
                frames.push(FrameCandidate {
                    title: self.name(&child_destination, &child_path)?,
                    geometry: self.extents(&child_destination, &child_path)?,
                    destination: child_destination,
                    path: child_path,
                });
            }
        }
        Ok(frames)
    }

    fn verify_cached_frame(
        &self,
        window: &WindowFacts,
        cached: &CachedFrame,
    ) -> Result<(), String> {
        if self.role(&cached.destination, &cached.frame_path)? != "frame" {
            return Err("cached AT-SPI object is no longer a frame".to_string());
        }
        let candidate = FrameCandidate {
            title: self.name(&cached.destination, &cached.frame_path)?,
            geometry: self.extents(&cached.destination, &cached.frame_path)?,
            destination: cached.destination.clone(),
            path: cached.frame_path.clone(),
        };
        if frame_score(window, &candidate).is_none() {
            return Err("cached AT-SPI frame no longer matches the focused X11 window".to_string());
        }
        Ok(())
    }

    fn discover_tab_list(
        &self,
        destination: &str,
        frame_path: &str,
    ) -> Result<(Option<String>, bool), String> {
        let mut pending = VecDeque::new();
        for (child_destination, child_path) in self.children(destination, frame_path)? {
            pending.push_back((child_destination, child_path.to_string(), 1_usize));
        }
        let mut visited = 0_usize;
        let mut tab_lists = Vec::new();
        let mut mail_marker = false;

        while let Some((node_destination, node_path, depth)) = pending.pop_front() {
            visited += 1;
            if visited > DISCOVERY_NODE_LIMIT {
                return Err("bounded AT-SPI tab discovery exceeded its node limit".to_string());
            }
            let Some(role) = self.discovery_role(&node_destination, &node_path)? else {
                continue;
            };
            let name = self.name(&node_destination, &node_path)?;
            if is_thunderbird_mail_marker(&role, &name) {
                mail_marker = true;
            }
            if role == "page tab list" {
                tab_lists.push(node_path);
                continue;
            }
            if depth >= DISCOVERY_DEPTH_LIMIT || !is_discovery_container(&role) {
                continue;
            }
            for (child_destination, child_path) in
                self.children(&node_destination, &node_path)?
            {
                pending.push_back((child_destination, child_path.to_string(), depth + 1));
            }
        }

        match tab_lists.len() {
            0 => Ok((None, mail_marker)),
            1 => Ok((tab_lists.pop(), mail_marker)),
            count => Err(format!(
                "AT-SPI frame exposes {count} page-tab lists; association is ambiguous"
            )),
        }
    }

    fn inspect_tab_list(
        &self,
        adapter: &DocumentLifecycleAdapter,
        destination: &str,
        tab_list_path: &str,
        association_proof: String,
    ) -> Result<(InternalDocumentState, usize, String), String> {
        if self.role(destination, tab_list_path)? != "page tab list" {
            return Err("cached AT-SPI object is no longer a page-tab list".to_string());
        }
        let mut tabs = Vec::new();
        for (child_destination, child_path) in self.children(destination, tab_list_path)? {
            let child_path = child_path.to_string();
            if self.role(&child_destination, &child_path)? != "page tab" {
                continue;
            }
            let states = self.states(&child_destination, &child_path)?;
            let selected = states
                .first()
                .is_some_and(|word| word & SELECTED_STATE_BIT != 0);
            let name = self.name(&child_destination, &child_path)?;
            tabs.push((
                child_path,
                name,
                selected,
            ));
        }
        if tabs.is_empty() {
            return Err("AT-SPI page-tab list contains no page-tab children".to_string());
        }
        let selected: Vec<_> = tabs.iter().filter(|(_, _, selected)| *selected).collect();
        if selected.len() != 1 {
            return Err(format!(
                "AT-SPI page-tab list has {} selected tabs; expected exactly one",
                selected.len()
            ));
        }
        let selected_path = selected[0].0.clone();
        let selected_name = selected[0].1.clone();
        let count = tabs.len();
        Ok((
            InternalDocumentState::Known {
                count,
                selected: selected_name,
                minimum_persistent: adapter.minimum_persistent,
                association_proof,
            },
            count,
            selected_path,
        ))
    }

    fn inspect_cached(
        &self,
        adapter: &DocumentLifecycleAdapter,
        window: &WindowFacts,
        cached: &CachedFrame,
    ) -> Result<(InternalDocumentState, Option<String>, Option<usize>, Option<String>), String> {
        self.verify_cached_frame(window, cached)?;
        if let Some(path) = cached.tab_list_path.as_deref() {
            let (state, count, selected_path) = self.inspect_tab_list(
                adapter,
                &cached.destination,
                path,
                cached.association_proof.clone(),
            )?;
            return Ok((
                state,
                Some(path.to_string()),
                Some(count),
                Some(selected_path),
            ));
        }
        let (tab_list_path, mail_marker) =
            self.discover_tab_list(&cached.destination, &cached.frame_path)?;
        match tab_list_path {
            Some(path) => {
                let (state, count, selected_path) = self.inspect_tab_list(
                    adapter,
                    &cached.destination,
                    &path,
                    cached.association_proof.clone(),
                )?;
                Ok((state, Some(path), Some(count), Some(selected_path)))
            }
            None if adapter.kind == ProviderKind::Thunderbird && mail_marker => Ok((
                InternalDocumentState::BaseOnly {
                    minimum_persistent: adapter.minimum_persistent,
                    association_proof: cached.association_proof.clone(),
                },
                None,
                Some(adapter.minimum_persistent),
                None,
            )),
            None if adapter.kind == ProviderKind::Thunderbird => Err(
                "matched Thunderbird frame has no tab strip and no healthy Mail marker".to_string(),
            ),
            None => Err(
                "Brave accessibility tab strip is unavailable; relaunch from the managed launcher"
                    .to_string(),
            ),
        }
    }

    fn discover(
        &self,
        adapter: &DocumentLifecycleAdapter,
        window: &WindowFacts,
    ) -> Result<(CachedFrame, InternalDocumentState), String> {
        let candidates = self.frame_candidates(adapter.application_name)?;
        if candidates.is_empty() {
            return Err(format!(
                "{} is not exposing accessible frames; relaunch from the managed launcher",
                adapter.application_name
            ));
        }
        let frame = select_frame(window, &candidates)?;
        let association_proof = format!(
            "bus={} frame={} geometry={}x{}@{},{}",
            frame.destination,
            frame.path,
            frame.geometry.width,
            frame.geometry.height,
            frame.geometry.x,
            frame.geometry.y
        );
        let mut cached = CachedFrame {
            destination: frame.destination,
            frame_path: frame.path,
            tab_list_path: None,
            last_count: None,
            selected_path: None,
            association_proof,
        };
        let (state, tab_list_path, last_count, selected_path) =
            self.inspect_cached(adapter, window, &cached)?;
        cached.tab_list_path = tab_list_path;
        cached.last_count = last_count;
        cached.selected_path = selected_path;
        Ok((cached, state))
    }

    pub fn quit_thunderbird(&self, main_window: &WindowFacts) -> Result<(), String> {
        let candidates = self.frame_candidates("Thunderbird")?;
        let frame = select_frame(main_window, &candidates)?;
        let (file_destination, file_path) =
            self.unique_named_descendant(&frame.destination, &frame.path, "menu", "File")?;
        let file_path = file_path.to_string();
        if self.action_count(&file_destination, &file_path)? != 1
            || self.action_name(&file_destination, &file_path, 0)? != "click"
        {
            return Err("Thunderbird File menu does not expose one click action".into());
        }
        self.do_action(&file_destination, &file_path, 0)?;

        let mut last_error = "Thunderbird Quit action did not appear".to_string();
        for _ in 0..20 {
            match self.unique_named_descendant(&file_destination, &file_path, "menu item", "Quit") {
                Ok((quit_destination, quit_path)) => {
                    let quit_path = quit_path.to_string();
                    if self.action_count(&quit_destination, &quit_path)? != 1
                        || self.action_name(&quit_destination, &quit_path, 0)? != "click"
                        || !self
                            .action_binding(&quit_destination, &quit_path, 0)?
                            .contains("<Control>Q")
                    {
                        return Err(
                            "Thunderbird Quit menu item does not expose the audited native action"
                                .into(),
                        );
                    }
                    return self.do_action(&quit_destination, &quit_path, 0);
                }
                Err(error) => last_error = error,
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        Err(last_error)
    }
}

impl InternalDocumentProvider for AtspiDocumentProvider {
    fn inspect(&mut self, inspection: &Inspection) -> InternalDocumentState {
        let Some(adapter) = adapter_for(&inspection.app_identity) else {
            return InternalDocumentState::Unknown {
                reason: format!(
                    "{} has no internal-document provider",
                    inspection.app_identity
                ),
            };
        };
        if let Err(error) = self.connection() {
            return InternalDocumentState::Unknown { reason: error };
        }

        let xid = inspection.identity_window.xid;
        if let Some(mut cached) = self.cache.frames.get(&xid).cloned() {
            match self.inspect_cached(adapter, &inspection.identity_window, &cached) {
                Ok((state, tab_list_path, last_count, selected_path)) => {
                    cached.tab_list_path = tab_list_path;
                    cached.last_count = last_count;
                    cached.selected_path = selected_path;
                    self.cache.frames.insert(xid, cached);
                    return state;
                }
                Err(_) => self.cache.invalidate(xid),
            }
        }

        match self.discover(adapter, &inspection.identity_window) {
            Ok((cached, state)) => {
                self.cache.frames.insert(xid, cached);
                state
            }
            Err(reason) => InternalDocumentState::Unknown { reason },
        }
    }

    fn retain_windows(&mut self, windows: &[WindowFacts]) {
        self.cache.retain_windows(windows);
    }

    fn invalidate(&mut self, xid: u32) {
        self.cache.invalidate(xid);
    }
}

fn connect_atspi() -> Result<Connection, String> {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
        .map_err(|_| "XDG_RUNTIME_DIR is unavailable".to_string())?;
    let address = format!("unix:path={runtime_dir}/at-spi/bus_0");
    ConnectionBuilder::address(address.as_str())
        .map_err(|error| error.to_string())?
        .build()
        .map_err(|error| error.to_string())
}

fn is_discovery_container(role: &str) -> bool {
    matches!(
        role,
        "frame"
            | "panel"
            | "filler"
            | "tool bar"
            | "scroll pane"
            | "section"
            | "unknown"
            | "split pane"
            | "layered pane"
            | "root pane"
            | "menu bar"
    )
}

fn is_action_discovery_container(role: &str) -> bool {
    is_discovery_container(role) || role == "menu"
}

fn is_thunderbird_mail_marker(role: &str, name: &str) -> bool {
    (role == "menu" && name == "Message")
        || (role == "button" && matches!(name, "New Message" | "Get Messages"))
        || (role == "button" && name.starts_with("Mail ("))
}

fn frame_score(window: &WindowFacts, frame: &FrameCandidate) -> Option<u32> {
    let geometry = window.geometry?;
    let width_delta = i32::from(geometry.width).abs_diff(i32::from(frame.geometry.width));
    let height_delta = i32::from(geometry.height).abs_diff(i32::from(frame.geometry.height));
    if width_delta > 4 || height_delta > 4 {
        return None;
    }
    let x_delta = geometry.x.abs_diff(frame.geometry.x);
    let y_delta = geometry.y.abs_diff(frame.geometry.y);
    if x_delta > 96 || y_delta > 96 {
        return None;
    }
    let mut score = 4;
    if width_delta == 0 && height_delta == 0 {
        score += 4;
    }
    if x_delta <= 4 && y_delta <= 32 {
        score += 2;
    }
    if window.title.as_deref() == Some(frame.title.as_str()) {
        score += 3;
    }
    Some(score)
}

fn select_frame(
    window: &WindowFacts,
    candidates: &[FrameCandidate],
) -> Result<FrameCandidate, String> {
    let mut matches: Vec<_> = candidates
        .iter()
        .filter_map(|candidate| frame_score(window, candidate).map(|score| (score, candidate)))
        .collect();
    matches.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
    let Some((best_score, best)) = matches.first() else {
        return Err("no AT-SPI frame matches the focused X11 window geometry".to_string());
    };
    if matches
        .get(1)
        .is_some_and(|(score, _)| score == best_score)
    {
        return Err("multiple AT-SPI frames match the focused X11 window".to_string());
    }
    Ok((*best).clone())
}

#[cfg(test)]
mod tests {
    use super::{
        adapter_for, close_decision, is_action_discovery_container, is_discovery_container,
        is_skippable_discovery_error, launch_opt_in_status, select_frame, CachedFrame,
        DocumentCloseDecision, InternalDocumentState, ProviderCache, WindowGeometry,
        UNKNOWN_METHOD_ERROR,
    };
    use crate::model::{Inspection, ProcessInfo, WindowFacts};

    fn known(count: usize) -> InternalDocumentState {
        InternalDocumentState::Known {
            count,
            selected: "selected".to_string(),
            minimum_persistent: 1,
            association_proof: "test".to_string(),
        }
    }

    #[test]
    fn action_lookup_enters_open_menus_without_changing_tab_discovery() {
        assert!(!is_discovery_container("menu"));
        assert!(is_action_discovery_container("menu"));
        assert!(!is_action_discovery_container("menu item"));
    }

    #[test]
    fn known_multi_document_closes_only_the_active_document() {
        assert_eq!(
            close_decision(&known(3)),
            DocumentCloseDecision::CloseActiveDocument
        );
        assert_eq!(
            close_decision(&known(2)),
            DocumentCloseDecision::CloseActiveDocument
        );
    }

    #[test]
    fn known_final_and_base_only_preserve_the_top_level_window() {
        assert_eq!(
            close_decision(&known(1)),
            DocumentCloseDecision::PreserveTopLevelWindow
        );
        let base = InternalDocumentState::BaseOnly {
            minimum_persistent: 1,
            association_proof: "mail-frame".to_string(),
        };
        assert_eq!(
            close_decision(&base),
            DocumentCloseDecision::PreserveTopLevelWindow
        );
    }

    #[test]
    fn unknown_document_state_refuses() {
        let state = InternalDocumentState::Unknown {
            reason: "accessibility unavailable".to_string(),
        };
        assert_eq!(
            close_decision(&state),
            DocumentCloseDecision::Refuse("accessibility unavailable".to_string())
        );
    }

    #[test]
    fn focused_window_geometry_selects_its_own_brave_frame() {
        let mut window = WindowFacts::test_window(10, "Brave-browser");
        window.title = Some("Second - Brave".to_string());
        window.geometry = Some(WindowGeometry {
            x: 20,
            y: 20,
            width: 1050,
            height: 828,
        });
        let candidates = [
            super::FrameCandidate {
                destination: ":1.1".to_string(),
                path: "/frame/one".to_string(),
                title: "First - Brave".to_string(),
                geometry: WindowGeometry {
                    x: 10,
                    y: 10,
                    width: 900,
                    height: 700,
                },
            },
            super::FrameCandidate {
                destination: ":1.1".to_string(),
                path: "/frame/two".to_string(),
                title: "Second - Brave".to_string(),
                geometry: WindowGeometry {
                    x: 20,
                    y: 20,
                    width: 1050,
                    height: 828,
                },
            },
        ];
        assert_eq!(
            select_frame(&window, &candidates).expect("focused frame").path,
            "/frame/two"
        );
    }

    #[test]
    fn ambiguous_frame_match_refuses() {
        let window = WindowFacts::test_window(10, "Brave-browser");
        let candidate = super::FrameCandidate {
            destination: ":1.1".to_string(),
            path: "/frame/one".to_string(),
            title: "window 10".to_string(),
            geometry: window.geometry.expect("geometry"),
        };
        let mut duplicate = candidate.clone();
        duplicate.path = "/frame/two".to_string();
        assert!(select_frame(&window, &[candidate, duplicate]).is_err());
    }

    #[test]
    fn unsupported_discovery_object_is_skipped_without_weakening_other_errors() {
        assert!(is_skippable_discovery_error(
            "/org/a11y/atspi/null",
            None
        ));
        assert!(is_skippable_discovery_error(
            "/org/a11y/atspi/accessible/99",
            Some(UNKNOWN_METHOD_ERROR)
        ));
        assert!(!is_skippable_discovery_error(
            "/org/a11y/atspi/accessible/99",
            Some("org.freedesktop.DBus.Error.NoReply")
        ));
        assert!(!is_skippable_discovery_error(
            "/org/a11y/atspi/accessible/99",
            None
        ));
    }

    #[test]
    fn unsupported_object_does_not_prevent_valid_frame_selection() {
        assert!(is_skippable_discovery_error(
            "/org/a11y/atspi/null",
            Some(UNKNOWN_METHOD_ERROR)
        ));
        let window = WindowFacts::test_window(10, "Brave-browser");
        let valid = super::FrameCandidate {
            destination: ":1.1".to_string(),
            path: "/org/a11y/atspi/accessible/1".to_string(),
            title: "window 10".to_string(),
            geometry: window.geometry.expect("geometry"),
        };
        assert_eq!(
            select_frame(&window, &[valid])
                .expect("valid frame remains discoverable")
                .path,
            "/org/a11y/atspi/accessible/1"
        );
    }

    #[test]
    fn provider_unavailability_remains_unknown_and_refuses() {
        let state = InternalDocumentState::Unknown {
            reason: "AT-SPI unavailable".to_string(),
        };
        assert!(matches!(
            close_decision(&state),
            DocumentCloseDecision::Refuse(reason) if reason == "AT-SPI unavailable"
        ));
    }

    #[test]
    fn brave_launch_opt_in_diagnostic_uses_validated_main_process() {
        let mut window = WindowFacts::test_window(10, "Brave-browser");
        window.pid = Some(20);
        window.pid_validated = true;
        window.process = Some(ProcessInfo {
            pid: 20,
            uid: 1000,
            parent_pid: Some(1),
            name: "brave".to_string(),
            executable: Some("/opt/brave.com/brave/brave".to_string()),
            command_line: Some(
                "/opt/brave.com/brave/brave --force-renderer-accessibility=basic"
                    .to_string(),
            ),
        });
        let inspection = Inspection {
            focused_xid: window.xid,
            focused_window: window.clone(),
            identity_window: window.clone(),
            app_identity: "brave-browser".to_string(),
            meaningful_windows: vec![window],
            decisions: Vec::new(),
        };
        assert_eq!(launch_opt_in_status(&inspection), "yes");
    }

    #[test]
    fn cache_is_invalidated_when_x11_window_disappears() {
        let mut cache = ProviderCache::default();
        cache.frames.insert(
            10,
            CachedFrame {
                destination: ":1.1".to_string(),
                frame_path: "/frame".to_string(),
                tab_list_path: Some("/tabs".to_string()),
                last_count: Some(3),
                selected_path: Some("/tab/3".to_string()),
                association_proof: "test".to_string(),
            },
        );
        cache.retain_windows(&[WindowFacts::test_window(20, "Brave-browser")]);
        assert!(cache.frames.is_empty());
    }

    #[test]
    fn adapters_keep_brave_variants_separate_and_exclude_pwas() {
        assert_eq!(
            adapter_for("brave-origin").expect("origin").application_name,
            "Brave Origin"
        );
        assert_eq!(
            adapter_for("brave-browser").expect("browser").application_name,
            "Brave Browser"
        );
        assert!(adapter_for("spotifyweb").is_none());
        assert!(adapter_for("brave-origin-crx-app").is_none());
    }

    #[test]
    fn brave_three_one_and_unknown_states_choose_safe_actions() {
        for identity in ["brave-origin", "brave-browser"] {
            assert!(adapter_for(identity).is_some());
            assert_eq!(
                close_decision(&known(3)),
                DocumentCloseDecision::CloseActiveDocument
            );
            assert_eq!(
                close_decision(&known(1)),
                DocumentCloseDecision::PreserveTopLevelWindow
            );
            assert!(matches!(
                close_decision(&InternalDocumentState::Unknown {
                    reason: format!("{identity} accessibility unavailable"),
                }),
                DocumentCloseDecision::Refuse(_)
            ));
        }
    }

    #[test]
    fn brave_top_level_close_is_distinct_from_document_close() {
        for identity in ["brave-origin", "brave-browser"] {
            let adapter = adapter_for(identity).expect("Brave adapter");
            assert_eq!(
                adapter.close_document,
                super::CloseDocumentMethod::ControlW
            );
            assert_eq!(
                adapter.close_top_level,
                super::CloseTopLevelMethod::ControlShiftW
            );
        }
    }

    #[test]
    fn thunderbird_message_counts_and_base_state_choose_safe_actions() {
        let adapter = adapter_for("thunderbird-default").expect("Thunderbird adapter");
        assert_eq!(
            close_decision(&known(3)),
            DocumentCloseDecision::CloseActiveDocument
        );
        assert_eq!(
            close_decision(&known(2)),
            DocumentCloseDecision::CloseActiveDocument
        );
        assert_eq!(
            close_decision(&InternalDocumentState::BaseOnly {
                minimum_persistent: adapter.minimum_persistent,
                association_proof: "healthy-mail-frame".to_string(),
            }),
            DocumentCloseDecision::PreserveTopLevelWindow
        );
        assert!(matches!(
            close_decision(&InternalDocumentState::Unknown {
                reason: "failed AT-SPI query".to_string(),
            }),
            DocumentCloseDecision::Refuse(_)
        ));
    }

    #[test]
    fn thunderbird_uses_base_only_semantics_and_wm_delete_for_window_close() {
        let adapter = adapter_for("thunderbird-default").expect("Thunderbird adapter");
        assert_eq!(adapter.minimum_persistent, 1);
        assert_eq!(
            adapter.close_top_level,
            super::CloseTopLevelMethod::WmDelete
        );
    }
}
