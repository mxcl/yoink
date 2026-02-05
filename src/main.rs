use serde::Serialize;
use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Serialize)]
struct DownloadJson {
    repo: String,
    tag: String,
    url: String,
    executables: Vec<String>,
}

#[derive(Serialize)]
struct InfoJson {
    repo: String,
    tag: String,
    url: String,
}

fn main() -> ExitCode {
    run_with_args(env::args().skip(1))
}

fn run_with_args<I>(args: I) -> ExitCode
where
    I: IntoIterator<Item = String>,
{
    let args: Vec<String> = args.into_iter().collect();
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
        .map(|dir| {
            if dir.is_absolute() {
                dir
            } else {
                cwd.join(dir)
            }
        })
        .unwrap_or_else(|| cwd.clone());
    let use_relative = download_dir == cwd;

    if info_only {
        match yoink::release_info(repo) {
            Ok(info) => {
                let payload = InfoJson {
                    repo: format!("{}/{}", info.owner, info.name),
                    tag: info.tag,
                    url: info.asset_url,
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
                    let mut executables = Vec::new();
                    for path in &summary.paths {
                        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                            continue;
                        };
                        if executables.iter().any(|existing| existing == name) {
                            continue;
                        }
                        executables.push(name.to_string());
                    }
                    let payload = DownloadJson {
                        repo: summary.repo,
                        tag: summary.tag,
                        url: summary.url,
                        executables,
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

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::collections::BTreeMap;
    use std::ffi::OsString;
    use std::io::{BufRead, BufReader, Cursor, Write};
    use std::net::TcpListener;
    use std::path::Path;
    use std::thread;

    #[test]
    fn no_args_returns_usage() {
        let code = run_with_args(Vec::<String>::new());
        assert_eq!(code, ExitCode::from(2));
    }

    #[test]
    fn help_flag_returns_success() {
        let code = run_with_args(vec!["-h".to_string()]);
        assert_eq!(code, ExitCode::SUCCESS);
    }

    #[test]
    fn version_flag_returns_success() {
        let code = run_with_args(vec!["--version".to_string()]);
        assert_eq!(code, ExitCode::SUCCESS);
    }

    #[test]
    fn missing_directory_after_c_errors() {
        let code = run_with_args(vec!["-C".to_string()]);
        assert_eq!(code, ExitCode::from(2));
    }

    #[test]
    fn inline_c_sets_download_dir() {
        let code = run_with_args(vec!["-Ctmp".to_string(), "bad".to_string()]);
        assert_eq!(code, ExitCode::from(2));
    }

    #[test]
    fn invalid_repo_shape_errors() {
        let code = run_with_args(vec!["not-a-repo".to_string()]);
        assert_eq!(code, ExitCode::from(2));
    }

    #[test]
    fn missing_repo_after_flags_errors() {
        let code = run_with_args(vec!["-j".to_string()]);
        assert_eq!(code, ExitCode::from(2));
    }

    #[test]
    fn unknown_option_errors() {
        let code = run_with_args(vec!["-Z".to_string()]);
        assert_eq!(code, ExitCode::from(2));
    }

    #[test]
    fn combined_flags_unknown_option_errors() {
        let code = run_with_args(vec!["-jX".to_string(), "bad".to_string()]);
        assert_eq!(code, ExitCode::from(2));
    }

    #[test]
    fn combined_flags_reject_extra_args() {
        let code = run_with_args(vec![
            "-jI".to_string(),
            "mxcl/tool".to_string(),
            "extra".to_string(),
        ]);
        assert_eq!(code, ExitCode::from(2));
    }

    #[test]
    fn combined_flags_missing_dir_errors() {
        let code = run_with_args(vec!["-jC".to_string(), "mxcl/tool".to_string()]);
        assert_eq!(code, ExitCode::from(2));
    }

    #[test]
    fn combined_flags_inline_dir_sets_download_dir() {
        let code = run_with_args(vec!["-jCtmp".to_string(), "bad".to_string()]);
        assert_eq!(code, ExitCode::from(2));
    }

    #[test]
    fn dash_dash_stops_option_parsing() {
        let code = run_with_args(vec!["--".to_string(), "bad".to_string()]);
        assert_eq!(code, ExitCode::from(2));
    }

    #[test]
    #[serial]
    fn info_only_uses_release_info() {
        let server = TestServer::new(|base| {
            let mut responses = BTreeMap::new();
            let url = format!("{base}/download/tool");
            let body = format!(
                "{{\"tag_name\":\"v1.0.0\",\"assets\":[{{\"name\":\"tool\",\"browser_download_url\":\"{url}\"}}]}}"
            );
            responses.insert(
                "/repos/mxcl/tool/releases/latest".to_string(),
                body.into_bytes(),
            );
            responses
        });

        let _guard = EnvGuard::set("YOINK_GITHUB_API_BASE", &server.base);
        let code = run_with_args(vec!["-I".to_string(), "mxcl/tool".to_string()]);
        assert_eq!(code, ExitCode::SUCCESS);

        server.finish();
    }

    #[test]
    #[serial]
    fn info_only_reports_errors() {
        let server = TestServer::new(|_base| {
            let mut responses = BTreeMap::new();
            responses.insert(
                "/repos/mxcl/tool/releases/latest".to_string(),
                b"not-json".to_vec(),
            );
            responses
        });

        let _guard = EnvGuard::set("YOINK_GITHUB_API_BASE", &server.base);
        let code = run_with_args(vec!["-I".to_string(), "mxcl/tool".to_string()]);
        assert_eq!(code, ExitCode::from(1));

        server.finish();
    }

    #[test]
    #[serial]
    fn json_download_writes_to_directory() {
        let server = TestServer::new(|base| {
            let mut responses = BTreeMap::new();
            let url = format!("{base}/download/tool");
            let body = format!(
                "{{\"tag_name\":\"v1.0.0\",\"assets\":[{{\"name\":\"tool\",\"browser_download_url\":\"{url}\"}}]}}"
            );
            responses.insert(
                "/repos/mxcl/tool/releases/latest".to_string(),
                body.into_bytes(),
            );
            responses.insert("/download/tool".to_string(), b"bin".to_vec());
            responses
        });

        let dest = tempfile::tempdir().expect("temp dir");
        let _guard = EnvGuard::set("YOINK_GITHUB_API_BASE", &server.base);
        let code = run_with_args(vec![
            "-j".to_string(),
            "-C".to_string(),
            dest.path().display().to_string(),
            "mxcl/tool".to_string(),
        ]);
        assert_eq!(code, ExitCode::SUCCESS);
        assert!(dest.path().join("tool").exists());

        server.finish();
    }

    #[test]
    #[serial]
    fn json_output_skips_duplicate_executables() {
        let server = TestServer::new(|base| {
            let mut responses = BTreeMap::new();
            let url = format!("{base}/download/tool.zip");
            let body = format!(
                "{{\"tag_name\":\"v1.0.0\",\"assets\":[{{\"name\":\"tool.zip\",\"browser_download_url\":\"{url}\"}}]}}"
            );
            let zip = make_zip_bytes(&[("bin/tool", b"bin"), ("alt/tool", b"bin")]);
            responses.insert(
                "/repos/mxcl/tool/releases/latest".to_string(),
                body.into_bytes(),
            );
            responses.insert("/download/tool.zip".to_string(), zip);
            responses
        });

        let dest = tempfile::tempdir().expect("temp dir");
        let _guard = EnvGuard::set("YOINK_GITHUB_API_BASE", &server.base);
        let code = run_with_args(vec![
            "-j".to_string(),
            "-C".to_string(),
            dest.path().display().to_string(),
            "mxcl/tool".to_string(),
        ]);
        assert_eq!(code, ExitCode::SUCCESS);

        server.finish();
    }

    #[test]
    #[serial]
    fn download_relative_paths_use_cwd() {
        let server = TestServer::new(|base| {
            let mut responses = BTreeMap::new();
            let url = format!("{base}/download/tool");
            let body = format!(
                "{{\"tag_name\":\"v1.0.0\",\"assets\":[{{\"name\":\"tool\",\"browser_download_url\":\"{url}\"}}]}}"
            );
            responses.insert(
                "/repos/mxcl/tool/releases/latest".to_string(),
                body.into_bytes(),
            );
            responses.insert("/download/tool".to_string(), b"bin".to_vec());
            responses
        });

        let cwd = tempfile::tempdir().expect("temp dir");
        let _cwd_guard = DirGuard::set(cwd.path());
        let _guard = EnvGuard::set("YOINK_GITHUB_API_BASE", &server.base);
        let code = run_with_args(vec!["mxcl/tool".to_string()]);
        assert_eq!(code, ExitCode::SUCCESS);
        assert!(cwd.path().join("tool").exists());

        server.finish();
    }

    #[test]
    #[serial]
    fn relative_c_dir_joins_cwd() {
        let server = TestServer::new(|base| {
            let mut responses = BTreeMap::new();
            let url = format!("{base}/download/tool");
            let body = format!(
                "{{\"tag_name\":\"v1.0.0\",\"assets\":[{{\"name\":\"tool\",\"browser_download_url\":\"{url}\"}}]}}"
            );
            responses.insert(
                "/repos/mxcl/tool/releases/latest".to_string(),
                body.into_bytes(),
            );
            responses.insert("/download/tool".to_string(), b"bin".to_vec());
            responses
        });

        let cwd = tempfile::tempdir().expect("temp dir");
        let _cwd_guard = DirGuard::set(cwd.path());
        let _guard = EnvGuard::set("YOINK_GITHUB_API_BASE", &server.base);
        let code = run_with_args(vec![
            "-C".to_string(),
            "bin".to_string(),
            "mxcl/tool".to_string(),
        ]);
        assert_eq!(code, ExitCode::SUCCESS);
        assert!(cwd.path().join("bin").join("tool").exists());

        server.finish();
    }

    #[test]
    #[serial]
    #[cfg(unix)]
    fn run_branch_executes_binary() {
        let server = TestServer::new(|base| {
            let mut responses = BTreeMap::new();
            let url = format!("{base}/download/tool");
            let body = format!(
                "{{\"tag_name\":\"v1.0.0\",\"assets\":[{{\"name\":\"tool\",\"browser_download_url\":\"{url}\"}}]}}"
            );
            let script = b"#!/bin/sh\nexit 3\n";
            responses.insert(
                "/repos/mxcl/tool/releases/latest".to_string(),
                body.into_bytes(),
            );
            responses.insert("/download/tool".to_string(), script.to_vec());
            responses
        });

        let _guard = EnvGuard::set("YOINK_GITHUB_API_BASE", &server.base);
        let code = run_with_args(vec!["mxcl/tool".to_string(), "arg".to_string()]);
        assert_eq!(code, ExitCode::from(3));

        server.finish();
    }

    #[test]
    #[serial]
    fn run_branch_reports_errors() {
        let server = TestServer::new(|_base| {
            let mut responses = BTreeMap::new();
            responses.insert(
                "/repos/mxcl/tool/releases/latest".to_string(),
                b"not-json".to_vec(),
            );
            responses
        });

        let _guard = EnvGuard::set("YOINK_GITHUB_API_BASE", &server.base);
        let code = run_with_args(vec!["mxcl/tool".to_string(), "arg".to_string()]);
        assert_eq!(code, ExitCode::from(1));

        server.finish();
    }

    #[test]
    #[serial]
    fn download_to_dir_reports_errors() {
        let server = TestServer::new(|_base| {
            let mut responses = BTreeMap::new();
            responses.insert(
                "/repos/mxcl/tool/releases/latest".to_string(),
                b"not-json".to_vec(),
            );
            responses
        });

        let _guard = EnvGuard::set("YOINK_GITHUB_API_BASE", &server.base);
        let code = run_with_args(vec!["mxcl/tool".to_string()]);
        assert_eq!(code, ExitCode::from(1));

        server.finish();
    }

    struct EnvGuard {
        key: &'static str,
        previous: Option<OsString>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: impl AsRef<std::ffi::OsStr>) -> Self {
            let previous = env::var_os(key);
            env::set_var(key, value.as_ref());
            Self { key, previous }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            if let Some(value) = &self.previous {
                env::set_var(self.key, value);
            } else {
                env::remove_var(self.key);
            }
        }
    }

    struct DirGuard {
        previous: PathBuf,
    }

    impl DirGuard {
        fn set(path: &Path) -> Self {
            let previous = env::current_dir().expect("current dir");
            env::set_current_dir(path).expect("set current dir");
            Self { previous }
        }
    }

    impl Drop for DirGuard {
        fn drop(&mut self) {
            let _ = env::set_current_dir(&self.previous);
        }
    }

    struct TestServer {
        base: String,
        handle: Option<thread::JoinHandle<()>>,
    }

    impl TestServer {
        fn new<F>(make_responses: F) -> Self
        where
            F: FnOnce(&str) -> BTreeMap<String, Vec<u8>>,
        {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
            let addr = listener.local_addr().expect("addr");
            let base = format!("http://{addr}");
            let responses = make_responses(&base);
            let expected = responses.len();
            let handle = thread::spawn(move || {
                for _ in 0..expected {
                    let (mut stream, _) = listener.accept().expect("accept");
                    respond(&mut stream, &responses);
                }
            });
            Self {
                base,
                handle: Some(handle),
            }
        }

        fn finish(mut self) {
            if let Some(handle) = self.handle.take() {
                handle.join().expect("server thread");
            }
        }
    }

    fn respond(stream: &mut std::net::TcpStream, responses: &BTreeMap<String, Vec<u8>>) {
        let mut reader = BufReader::new(stream);
        let mut request_line = String::new();
        reader
            .read_line(&mut request_line)
            .expect("read request line");
        let path = request_line.split_whitespace().nth(1).unwrap_or("/");
        loop {
            let mut line = String::new();
            let bytes = reader.read_line(&mut line).expect("read header");
            if bytes == 0 || line == "\r\n" {
                break;
            }
        }
        let body = responses
            .get(path)
            .unwrap_or_else(|| panic!("unexpected path {path}"));
        let header = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        reader
            .get_mut()
            .write_all(header.as_bytes())
            .expect("write header");
        reader.get_mut().write_all(body).expect("write body");
    }

    fn make_zip_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buffer = Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut buffer);
            let options = zip::write::FileOptions::default();
            for &(name, contents) in entries {
                zip.start_file(name, options).expect("start file");
                zip.write_all(contents).expect("write file");
            }
            zip.finish().expect("finish zip");
        }
        buffer.into_inner()
    }
}
