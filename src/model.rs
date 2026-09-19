#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WmClass {
    pub instance: String,
    pub class: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessInfo {
    pub pid: u32,
    pub uid: u32,
    pub parent_pid: Option<u32>,
    pub name: String,
    pub executable: Option<String>,
    pub command_line: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowFacts {
    pub xid: u32,
    pub title: Option<String>,
    pub wm_class: Option<WmClass>,
    pub pid: Option<u32>,
    pub pid_validated: bool,
    pub pid_validation: String,
    pub process: Option<ProcessInfo>,
    pub client_machine: Option<String>,
    pub client_leader: Option<u32>,
    pub transient_for: Option<u32>,
    pub window_types: Vec<String>,
    pub states: Vec<String>,
    pub override_redirect: bool,
    pub mapped: bool,
    pub maclife_hidden: bool,
}

impl WindowFacts {
    #[cfg(test)]
    pub fn test_window(xid: u32, class: &str) -> Self {
        Self {
            xid,
            title: Some(format!("window {xid}")),
            wm_class: (!class.is_empty()).then(|| WmClass {
                instance: class.to_string(),
                class: class.to_string(),
            }),
            pid: None,
            pid_validated: false,
            pid_validation: "not supplied".to_string(),
            process: None,
            client_machine: None,
            client_leader: None,
            transient_for: None,
            window_types: vec!["_NET_WM_WINDOW_TYPE_NORMAL".to_string()],
            states: Vec::new(),
            override_redirect: false,
            mapped: true,
            maclife_hidden: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Disposition {
    Meaningful,
    Attached(String),
    Excluded(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowDecision {
    pub xid: u32,
    pub disposition: Disposition,
    pub belongs_to_focused_app: bool,
    pub grouping_rule: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Inspection {
    pub focused_xid: u32,
    pub focused_window: WindowFacts,
    pub identity_window: WindowFacts,
    pub app_identity: String,
    pub meaningful_windows: Vec<WindowFacts>,
    pub decisions: Vec<WindowDecision>,
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub active_window: u32,
    pub windows: Vec<WindowFacts>,
}
