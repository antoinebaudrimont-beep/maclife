use crate::identity::normalized_app_identity;
use crate::model::{Disposition, Inspection, WindowFacts};
use std::fmt::Write;

fn xid(value: Option<u32>) -> String {
    value
        .map(|value| format!("0x{value:08x}"))
        .unwrap_or_else(|| "none".to_string())
}

fn list(values: &[String]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values.join(", ")
    }
}

fn render_window(output: &mut String, window: &WindowFacts, prefix: &str) {
    let class = window
        .wm_class
        .as_ref()
        .map(|class| format!("instance={:?}, class={:?}", class.instance, class.class))
        .unwrap_or_else(|| "missing".to_string());
    let pid = window
        .pid
        .map(|pid| pid.to_string())
        .unwrap_or_else(|| "missing".to_string());
    let transient = window
        .transient_for
        .map(|owner| format!("yes, owner=0x{owner:08x}"))
        .unwrap_or_else(|| "no".to_string());

    let _ = writeln!(output, "{prefix}XID: 0x{:08x}", window.xid);
    let _ = writeln!(
        output,
        "{prefix}Title: {}",
        window.title.as_deref().unwrap_or("<untitled>")
    );
    let _ = writeln!(
        output,
        "{prefix}Normalized identity: {}",
        normalized_app_identity(window)
    );
    let _ = writeln!(output, "{prefix}WM_CLASS: {class}");
    let _ = writeln!(
        output,
        "{prefix}PID: {pid} [{}]",
        window.pid_validation
    );
    if let Some(process) = &window.process {
        let _ = writeln!(
            output,
            "{prefix}Process: name={:?}, exe={}, ppid={}",
            process.name,
            process.executable.as_deref().unwrap_or("unknown"),
            process
                .parent_pid
                .map(|pid| pid.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        );
    }
    let _ = writeln!(
        output,
        "{prefix}Client machine: {}",
        window.client_machine.as_deref().unwrap_or("missing")
    );
    let _ = writeln!(
        output,
        "{prefix}Client leader: {}",
        xid(window.client_leader)
    );
    let _ = writeln!(output, "{prefix}Window type: {}", list(&window.window_types));
    let _ = writeln!(output, "{prefix}Window state: {}", list(&window.states));
    let _ = writeln!(output, "{prefix}Transient: {transient}");
    let _ = writeln!(
        output,
        "{prefix}Managed flags: mapped={}, override_redirect={}",
        window.mapped, window.override_redirect
    );
}

pub fn render(inspection: &Inspection, verbose: bool) -> String {
    let mut output = String::new();
    let _ = writeln!(output, "Focused application inspection");
    let _ = writeln!(output, "==============================");
    let _ = writeln!(output, "Focused XID: 0x{:08x}", inspection.focused_xid);
    let _ = writeln!(output, "Application: {}", inspection.app_identity);
    if inspection.focused_window.xid != inspection.identity_window.xid {
        let _ = writeln!(
            output,
            "Identity owner: 0x{:08x} (focused window is transient)",
            inspection.identity_window.xid
        );
    }
    let _ = writeln!(output);
    render_window(&mut output, &inspection.focused_window, "  ");

    let _ = writeln!(
        output,
        "\nMeaningful windows for {} ({})",
        inspection.app_identity,
        inspection.meaningful_windows.len()
    );
    let _ = writeln!(output, "--------------------------------");
    for window in &inspection.meaningful_windows {
        let _ = writeln!(
            output,
            "  0x{:08x}  {:?}  class={}  pid={}  leader={}  type={}  state={}",
            window.xid,
            window.title.as_deref().unwrap_or("<untitled>"),
            window
                .wm_class
                .as_ref()
                .map(|class| class.class.as_str())
                .unwrap_or("missing"),
            window
                .pid
                .map(|pid| pid.to_string())
                .unwrap_or_else(|| "missing".to_string()),
            xid(window.client_leader),
            list(&window.window_types),
            list(&window.states),
        );
    }

    if verbose {
        let _ = writeln!(output, "\nVerbose grouping/filter decisions");
        let _ = writeln!(output, "---------------------------------");
        for decision in &inspection.decisions {
            let disposition = match &decision.disposition {
                Disposition::Meaningful => "meaningful".to_string(),
                Disposition::Attached(reason) => format!("attached: {reason}"),
                Disposition::Excluded(reason) => format!("excluded: {reason}"),
            };
            let relation = if decision.belongs_to_focused_app {
                format!(
                    "belongs ({})",
                    decision.grouping_rule.as_deref().unwrap_or("unspecified rule")
                )
            } else {
                "unrelated".to_string()
            };
            let _ = writeln!(
                output,
                "  0x{:08x}: {disposition}; {relation}",
                decision.xid
            );
        }
    }
    output
}
