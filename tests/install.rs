use serial_test::serial;
use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

fn install_repo(repo: &str) -> tempfile::TempDir {
    let temp = tempfile::tempdir().expect("create temp dir");
    let _guard = EnvGuard::set("HOME", temp.path());
    let dest = yoink::install(repo).expect("install failed");
    assert!(dest.exists(), "{} should exist", dest.display());
    assert!(dest.starts_with(temp.path()));
    assert_executable(&dest);
    temp
}

#[test]
#[serial]
fn installs_brewx() {
    let _temp = install_repo("mxcl/brewx");
}

#[test]
#[serial]
fn installs_pkgx() {
    let _temp = install_repo("pkgxdev/pkgx");
}

#[test]
#[serial]
fn installs_deno() {
    let _temp = install_repo("denoland/deno");
}

#[test]
#[serial]
fn installs_gum() {
    let _temp = install_repo("charmbracelet/gum");
}

#[test]
#[serial]
fn installs_direnv() {
    let _temp = install_repo("direnv/direnv");
}

#[test]
#[serial]
fn installs_cli() {
    let temp = install_repo("cli/cli");
    let gh = installed_bin(&temp, "gh");
    assert!(gh.exists(), "{} should exist", gh.display());
    assert_executable(&gh);
}

#[test]
#[serial]
fn installs_uv() {
    let temp = install_repo("astral-sh/uv");
    let uv = installed_bin(&temp, "uv");
    let uvx = installed_bin(&temp, "uvx");
    assert!(uv.exists(), "{} should exist", uv.display());
    assert_executable(&uv);
    assert!(uvx.exists(), "{} should exist", uvx.display());
    assert_executable(&uvx);
}

fn assert_executable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = path
            .metadata()
            .expect("stat binary")
            .permissions()
            .mode();
        assert!(mode & 0o111 != 0, "{} not executable", path.display());
    }
}

fn installed_bin(temp: &tempfile::TempDir, name: &str) -> PathBuf {
    temp.path()
        .join(".local")
        .join("bin")
        .join(binary_name(name))
}

fn binary_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

struct EnvGuard {
    key: &'static str,
    previous: Option<OsString>,
}

impl EnvGuard {
    fn set(key: &'static str, value: impl AsRef<Path>) -> Self {
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
