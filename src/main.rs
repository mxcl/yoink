use serde::Serialize;
use std::env;
use std::path::PathBuf;
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
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        print_usage();
        return ExitCode::from(2);
    }
    if args.len() == 1 {
        if args[0] == "-h" || args[0] == "--help" {
            print_usage();
            return ExitCode::SUCCESS;
        }
        if args[0] == "-V" || args[0] == "--version" {
            print_version();
            return ExitCode::SUCCESS;
        }
    }

    let mut json_output = false;
    let mut info_only = false;
    let mut download_dir: Option<PathBuf> = None;
    let mut index = 0;

    while index < args.len() {
        let arg = &args[index];
        if !arg.starts_with('-') || arg == "-" {
            break;
        }
        if arg == "--" {
            index += 1;
            break;
        }
        if arg == "-j" {
            json_output = true;
            index += 1;
            continue;
        }
        if arg == "-I" {
            info_only = true;
            index += 1;
            continue;
        }
        if arg == "-C" {
            let Some(dir) = args.get(index + 1) else {
                eprintln!("yoink: expected directory after -C");
                return ExitCode::from(2);
            };
            download_dir = Some(PathBuf::from(dir));
            index += 2;
            continue;
        }
        if arg.starts_with("-C") && arg.len() > 2 {
            download_dir = Some(PathBuf::from(&arg[2..]));
            index += 1;
            continue;
        }
        if arg.len() > 2 {
            let mut chars = arg.chars();
            chars.next();
            let mut handled = true;
            while let Some(ch) = chars.next() {
                match ch {
                    'j' => json_output = true,
                    'I' => info_only = true,
                    'C' => {
                        let rest: String = chars.collect();
                        if rest.is_empty() {
                            let Some(dir) = args.get(index + 1) else {
                                eprintln!("yoink: expected directory after -C");
                                return ExitCode::from(2);
                            };
                            download_dir = Some(PathBuf::from(dir));
                            index += 1;
                        } else {
                            download_dir = Some(PathBuf::from(rest));
                        }
                        break;
                    }
                    _ => {
                        handled = false;
                        break;
                    }
                }
            }
            if handled {
                index += 1;
                continue;
            }
        }

        eprintln!("yoink: unrecognized option {arg}");
        print_usage();
        return ExitCode::from(2);
    }

    let Some(repo) = args.get(index) else {
        eprintln!("yoink: expected owner/repo as the first argument");
        print_usage();
        return ExitCode::from(2);
    };
    let rest: Vec<String> = args.iter().skip(index + 1).cloned().collect();

    if !yoink::is_repo_shape(repo) {
        eprintln!("yoink: expected owner/repo as the first argument");
        print_usage();
        return ExitCode::from(2);
    }

    if !rest.is_empty() {
        if info_only || json_output || download_dir.is_some() {
            eprintln!("yoink: -C, -j, and -I require no additional args");
            return ExitCode::from(2);
        }
        return match yoink::run(repo, &rest) {
            Ok(code) => {
                let code = u8::try_from(code).unwrap_or(1);
                ExitCode::from(code)
            }
            Err(err) => {
                eprintln!("yoink: {err:?}");
                ExitCode::from(1)
            }
        };
    }

    let cwd = match env::current_dir() {
        Ok(dir) => dir,
        Err(err) => {
            eprintln!("yoink: {err:?}");
            return ExitCode::from(1);
        }
    };
    let download_dir = download_dir
        .map(|dir| if dir.is_absolute() { dir } else { cwd.join(dir) })
        .unwrap_or_else(|| cwd.clone());
    let use_relative = download_dir == cwd;

    if info_only {
        match yoink::release_info(repo) {
            Ok(info) => {
                let payload = DownloadJson {
                    repo: format!("{}/{}", info.owner, info.name),
                    tag: info.tag,
                    url: info.asset_url,
                    asset: info.asset_name,
                    paths: Vec::new(),
                };
                match serde_json::to_string_pretty(&payload) {
                    Ok(json) => println!("{json}"),
                    Err(err) => {
                        eprintln!("yoink: {err:?}");
                        return ExitCode::from(1);
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
        match yoink::download_to_dir(repo, &download_dir) {
            Ok(summary) => {
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
                    match serde_json::to_string_pretty(&payload) {
                        Ok(json) => println!("{json}"),
                        Err(err) => {
                            eprintln!("yoink: {err:?}");
                            return ExitCode::from(1);
                        }
                    }
                } else if use_relative {
                    for path in &summary.paths {
                        if let Ok(rel) = path.strip_prefix(&cwd) {
                            let display = std::path::PathBuf::from(".").join(rel);
                            println!("{}", display.display());
                        } else {
                            println!("{}", path.display());
                        }
                    }
                } else {
                    for path in &summary.paths {
                        println!("{}", path.display());
                    }
                }
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("yoink: {err:?}");
                ExitCode::from(1)
            }
        }
    }
}

fn print_usage() {
    eprintln!("usage:");
    eprintln!("  yoink [-jI] [-C dir] <owner/repo> [args...]");
    eprintln!("  yoink --version");
}

fn print_version() {
    println!("yoink {}", env!("CARGO_PKG_VERSION"));
}
