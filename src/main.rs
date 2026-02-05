use serde::Serialize;
use std::env;
use std::process::ExitCode;

#[derive(Serialize)]
struct DownloadJson {
    repo: String,
    tag: String,
    url: String,
    asset: String,
    paths: Vec<String>,
}

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
            let json_output = env::var("JSON").ok().as_deref() == Some("1");
            match yoink::download_to_dir(&first, &cwd) {
                Ok(summary) => {
                    for path in &summary.paths {
                        eprintln!("{}", path.display());
                    }
                    if json_output {
                        let payload = DownloadJson {
                            repo: summary.repo,
                            tag: summary.tag,
                            url: summary.url,
                            asset: summary.asset_name,
                            paths: summary
                                .paths
                                .iter()
                                .map(|path| path.display().to_string())
                                .collect(),
                        };
                        match serde_json::to_string(&payload) {
                            Ok(json) => println!("{json}"),
                            Err(err) => {
                                eprintln!("yoink: {err:?}");
                                return ExitCode::from(1);
                            }
                        }
                    } else {
                        if let Ok(rel) = summary.primary_path.strip_prefix(&cwd) {
                            let display = std::path::PathBuf::from(".").join(rel);
                            println!("{}", display.display());
                        } else {
                            println!("{}", summary.primary_path.display());
                        }
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
