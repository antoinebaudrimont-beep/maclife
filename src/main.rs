use maclife::{identity, report, x11, DynError};

fn usage() -> &'static str {
    "Usage: maclife inspect [--verbose]\n\nRead the current X11 session and report the focused application and its meaningful windows."
}

fn run() -> Result<(), DynError> {
    let mut args = std::env::args().skip(1);
    let command = args.next();

    if matches!(command.as_deref(), Some("-h" | "--help")) {
        println!("{}", usage());
        return Ok(());
    }
    if command.as_deref() != Some("inspect") {
        return Err(usage().into());
    }

    let mut verbose = false;
    for arg in args {
        match arg.as_str() {
            "-v" | "--verbose" => verbose = true,
            "-h" | "--help" => {
                println!("{}", usage());
                return Ok(());
            }
            _ => return Err(format!("unknown argument: {arg}\n\n{}", usage()).into()),
        }
    }

    let snapshot = x11::collect_snapshot()?;
    let inspection = identity::inspect(snapshot.active_window, snapshot.windows)?;
    print!("{}", report::render(&inspection, verbose));
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("maclife: {error}");
        std::process::exit(1);
    }
}
