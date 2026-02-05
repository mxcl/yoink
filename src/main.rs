use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let Some(first) = args.next() else {
        print_usage();
        return ExitCode::from(2);
    };

    if first == "-h" || first == "--help" {
        print_usage();
        return ExitCode::SUCCESS;
    }

    if first == "-V" || first == "--version" {
        if args.next().is_some() {
            eprintln!("yoink: extra arguments after {first} are not supported yet");
            return ExitCode::from(2);
        }
        print_version();
        return ExitCode::SUCCESS;
    }

    if yoink::is_repo_shape(&first) {
        let rest: Vec<String> = args.collect();
        if rest.is_empty() {
            let cwd = match env::current_dir() {
                Ok(dir) => dir,
                Err(err) => {
                    eprintln!("yoink: {err:?}");
                    return ExitCode::from(1);
                }
            };
            match yoink::download_to_dir(&first, &cwd) {
                Ok(paths) => {
                    for path in paths {
                        println!("{}", path.display());
                    }
                    ExitCode::SUCCESS
                }
                Err(err) => {
                    eprintln!("yoink: {err:?}");
                    ExitCode::from(1)
                }
            }
        } else {
            match yoink::run(&first, &rest) {
                Ok(code) => {
                    let code = u8::try_from(code).unwrap_or(1);
                    ExitCode::from(code)
                }
                Err(err) => {
                    eprintln!("yoink: {err:?}");
                    ExitCode::from(1)
                }
            }
        }
    } else {
        eprintln!("yoink: expected owner/repo as the first argument");
        print_usage();
        ExitCode::from(2)
    }
}

fn print_usage() {
    eprintln!("usage:");
    eprintln!("  yoink <owner/repo> [args...]");
    eprintln!("  yoink --version");
}

fn print_version() {
    println!("yoink {}", env!("CARGO_PKG_VERSION"));
}
