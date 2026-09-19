use crate::model::ProcessInfo;
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Debug)]
struct Status {
    name: String,
    uid: u32,
    parent_pid: Option<u32>,
}

fn parse_status(text: &str) -> Option<Status> {
    let mut name = None;
    let mut uid = None;
    let mut parent_pid = None;

    for line in text.lines() {
        if let Some(value) = line.strip_prefix("Name:") {
            name = Some(value.trim().to_string());
        } else if let Some(value) = line.strip_prefix("Uid:") {
            uid = value.split_whitespace().next()?.parse().ok();
        } else if let Some(value) = line.strip_prefix("PPid:") {
            parent_pid = value.trim().parse().ok();
        }
    }

    Some(Status {
        name: name?,
        uid: uid?,
        parent_pid,
    })
}

pub fn current_uid() -> Option<u32> {
    parse_status(&fs::read_to_string("/proc/self/status").ok()?).map(|status| status.uid)
}

pub fn hostname() -> Option<String> {
    fs::read_to_string("/proc/sys/kernel/hostname")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub fn read_process(pid: u32) -> Option<ProcessInfo> {
    let root = PathBuf::from(format!("/proc/{pid}"));
    let status = parse_status(&fs::read_to_string(root.join("status")).ok()?)?;
    let executable = fs::read_link(root.join("exe"))
        .ok()
        .map(|path| path.to_string_lossy().into_owned());
    let command_line = fs::read(root.join("cmdline")).ok().and_then(|bytes| {
        let fields: Vec<_> = bytes
            .split(|byte| *byte == 0)
            .filter(|field| !field.is_empty())
            .map(|field| String::from_utf8_lossy(field).into_owned())
            .collect();
        (!fields.is_empty()).then(|| fields.join(" "))
    });

    Some(ProcessInfo {
        pid,
        uid: status.uid,
        parent_pid: status.parent_pid,
        name: status.name,
        executable,
        command_line,
    })
}

#[cfg(test)]
mod tests {
    use super::parse_status;

    #[test]
    fn parses_linux_status_fields() {
        let status = parse_status("Name:\tbrave\nUid:\t1000\t1000\t1000\t1000\nPPid:\t42\n")
            .expect("valid status");
        assert_eq!(status.name, "brave");
        assert_eq!(status.uid, 1000);
        assert_eq!(status.parent_pid, Some(42));
    }
}
