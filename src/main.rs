use maclife::documents::{self, AtspiDocumentProvider, InternalDocumentProvider};
use maclife::{identity, report, runtime, x11, xfwm4, DynError};

fn usage() -> &'static str {
    "Usage:\n  maclife --version\n  maclife inspect [--verbose]\n  maclife xfwm4-status [--machine]\n  maclife run [--dry-run] [--verbose] [--close-keycode N] [--quit-keycode N] [--close-window-keycode N]\n  maclife restore APPLICATION\n\nInspect and xfwm4-status are read-only. Run grabs the dedicated Toshy keys. Restore only activates a window explicitly marked as MacLife-hidden."
}

fn run() -> Result<(), DynError> {
    let mut args = std::env::args().skip(1);
    let command = args.next();

    if matches!(command.as_deref(), Some("-h" | "--help")) {
        println!("{}", usage());
        return Ok(());
    }
    if matches!(command.as_deref(), Some("-V" | "--version")) {
        if args.next().is_some() {
            return Err(usage().into());
        }
        println!("maclife {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    match command.as_deref() {
        Some("xfwm4-status") => {
            let machine = match args.next().as_deref() {
                None => false,
                Some("--machine") => true,
                _ => return Err(usage().into()),
            };
            if args.next().is_some() {
                return Err(usage().into());
            }
            let result = xfwm4::probe();
            if machine {
                println!("state={}", result.state.name());
                println!("installed={}", result.installed_version);
                println!("reason={}", result.reason);
            } else {
                println!(
                    "xfwm4 integration: {} (installed {}; {})",
                    result.state.name(), result.installed_version, result.reason
                );
            }
            Ok(())
        }
        Some("inspect") => {
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
            if documents::adapter_for(&inspection.app_identity).is_some() {
                let mut provider = AtspiDocumentProvider::connect();
                if verbose {
                    println!(
                        "Internal document diagnostics: accessibility={} provider={} launch-opt-in-detected={}",
                        provider.availability(),
                        inspection.app_identity,
                        documents::launch_opt_in_status(&inspection)
                    );
                }
                let state = provider.inspect(&inspection);
                println!("Internal documents: {}", state.summary());
            }
            Ok(())
        }
        Some("run") => {
            let mut options = runtime::RunOptions::default();
            let arguments: Vec<_> = args.collect();
            let mut index = 0;
            while index < arguments.len() {
                match arguments[index].as_str() {
                    "--dry-run" => options.dry_run = true,
                    "-v" | "--verbose" => options.verbose = true,
                    "--close-keycode" | "--quit-keycode" | "--close-window-keycode" => {
                        let option = arguments[index].clone();
                        index += 1;
                        let value = arguments
                            .get(index)
                            .ok_or_else(|| format!("{option} requires a value"))?
                            .parse::<u8>()?;
                        if option == "--close-keycode" {
                            options.close_keycode = value;
                        } else if option == "--quit-keycode" {
                            options.quit_keycode = value;
                        } else {
                            options.close_window_keycode = value;
                        }
                    }
                    "-h" | "--help" => {
                        println!("{}", usage());
                        return Ok(());
                    }
                    argument => {
                        return Err(format!("unknown argument: {argument}\n\n{}", usage()).into())
                    }
                }
                index += 1;
            }
            runtime::run(options)
        }
        Some("restore") => {
            let identity = args.next().ok_or("restore requires an application identity")?;
            if let Some(argument) = args.next() {
                return Err(format!("unexpected restore argument: {argument}").into());
            }
            runtime::restore(&identity)
        }
        _ => Err(usage().into()),
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("maclife: {error}");
        std::process::exit(1);
    }
}
