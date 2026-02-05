use anyhow::{bail, Context, Result};
use fs2::FileExt;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use tempfile::TempDir;
use walkdir::WalkDir;

#[derive(Clone, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

#[derive(Deserialize)]
struct Release {
    assets: Vec<Asset>,
    tag_name: Option<String>,
}

#[derive(Debug)]
pub struct ReleaseInfo {
    pub owner: String,
    pub name: String,
    pub tag: String,
    pub asset_name: String,
    pub asset_url: String,
}

pub fn install(repo: &str) -> Result<PathBuf> {
    let (dest, _version) = install_with_version(repo)?;
    Ok(dest)
}

#[derive(Debug)]
pub struct DownloadSummary {
    pub repo: String,
    pub tag: String,
    pub url: String,
    pub asset_name: String,
    pub primary_path: PathBuf,
    pub paths: Vec<PathBuf>,
}

pub fn release_info(repo: &str) -> Result<ReleaseInfo> {
    let (owner, name) = parse_repo(repo)?;
    let client = github_client()?;
    resolve_release_info(&client, &owner, &name)
}

pub fn download_to_dir(repo: &str, dest_dir: &Path) -> Result<DownloadSummary> {
    let prepared = prepare_binary(repo)?;
    fs::create_dir_all(dest_dir).with_context(|| format!("create {}", dest_dir.display()))?;

    let Some(name) = prepared.path.file_name() else {
        bail!("downloaded binary has no filename");
    };
    let dest = dest_dir.join(name);
    install_binary(&prepared.path, &dest)?;
    let mut downloaded = vec![dest.clone()];

    for extra in &prepared.extra_paths {
        let Some(name) = extra.file_name() else {
            continue;
        };
        let extra_dest = dest_dir.join(name);
        if downloaded.iter().any(|path| path == &extra_dest) {
            continue;
        }
        install_binary(extra, &extra_dest)?;
        downloaded.push(extra_dest);
    }

    Ok(DownloadSummary {
        repo: format!("{}/{}", prepared.owner, prepared.name),
        tag: prepared.tag,
        url: prepared.asset_url,
        asset_name: prepared.asset_name,
        primary_path: dest,
        paths: downloaded,
    })
}

fn install_with_version(repo: &str) -> Result<(PathBuf, String)> {
    let prepared = prepare_binary(repo)?;
    let install_dir = default_install_dir()?;
    ensure_install_dir(&install_dir)?;

    let Some(name) = prepared.path.file_name() else {
        bail!("downloaded binary has no filename");
    };
    let dest = install_dir.join(name);
    install_payload(&prepared.path, &dest)?;
    let mut installed_bins = vec![dest.clone()];
    for extra in &prepared.extra_paths {
        let Some(name) = extra.file_name() else {
            continue;
        };
        let extra_dest = install_dir.join(name);
        if installed_bins.iter().any(|path| path == &extra_dest) {
            continue;
        }
        install_payload(extra, &extra_dest)?;
        installed_bins.push(extra_dest);
    }
    let version = prepared.tag.clone();
    record_install(
        &format!("{}/{}", prepared.owner, prepared.name),
        &version,
        &installed_bins,
    )?;

    Ok((dest, version))
}

pub fn is_repo_shape(input: &str) -> bool {
    parse_repo(input).is_ok()
}

pub fn run(repo: &str, args: &[String]) -> Result<i32> {
    let prepared = prepare_binary(repo)?;
    set_executable(&prepared.path)?;
    let status = Command::new(&prepared.path)
        .args(args)
        .status()
        .with_context(|| format!("run {}", prepared.path.display()))?;
    Ok(exit_status_code(status))
}

#[derive(Debug)]
pub struct InstallSummary {
    pub repo: String,
    pub version: String,
}

pub fn list_installs() -> Result<Vec<InstallSummary>> {
    let state = load_state()?;
    let mut installs = Vec::new();
    for (repo, entry) in state.installs {
        installs.push(InstallSummary {
            repo,
            version: display_version(&entry.version).to_string(),
        });
    }
    Ok(installs)
}

#[derive(Debug)]
pub struct UpgradeSummary {
    pub repo: String,
    pub version: String,
    pub path: PathBuf,
}

pub fn upgrade_all() -> Result<Vec<UpgradeSummary>> {
    let state = load_state()?;
    let repos: Vec<String> = state.installs.keys().cloned().collect();
    let mut upgrades = Vec::new();
    for repo in repos {
        let (path, version) = install_with_version(&repo)?;
        upgrades.push(UpgradeSummary {
            repo,
            version: display_version(&version).to_string(),
            path,
        });
    }
    Ok(upgrades)
}

pub fn uninstall(repo: &str) -> Result<()> {
    let (owner, name) = parse_repo(repo)?;
    let key = format!("{owner}/{name}");
    remove_install(&key)
}

fn parse_repo(repo: &str) -> Result<(String, String)> {
    let mut parts = repo.split('/');
    let owner = parts.next().unwrap_or("");
    let name = parts.next().unwrap_or("");
    if owner.is_empty() || name.is_empty() || parts.next().is_some() {
        bail!("expected repo in owner/name form")
    }
    Ok((owner.to_string(), name.to_string()))
}

struct PreparedBinary {
    owner: String,
    name: String,
    tag: String,
    asset_name: String,
    asset_url: String,
    path: PathBuf,
    extra_paths: Vec<PathBuf>,
    _download_dir: TempDir,
    _extracted: Option<ExtractedPaths>,
}

fn prepare_binary(repo: &str) -> Result<PreparedBinary> {
    let (owner, name) = parse_repo(repo)?;
    let client = github_client()?;
    let info = resolve_release_info(&client, &owner, &name)?;

    let temp_dir = tempfile::tempdir().context("create temp dir")?;
    let download_path = temp_dir.path().join(&info.asset_name);
    let asset_name = info.asset_name.clone();
    let asset_url = info.asset_url.clone();
    download_asset(&client, &asset_url, &download_path)?;

    let mut extracted = None;
    let (payload_path, extra_paths) = if is_archive_name(&asset_name) {
        let extracted_paths = extract_archive(&download_path, &name)?;
        let primary = extracted_paths.primary.clone();
        let extras = extracted_paths.extras.clone();
        extracted = Some(extracted_paths);
        (primary, extras)
    } else if is_gzip_name(&asset_name) {
        let extracted_paths = extract_gzip(&download_path, &name)?;
        let primary = extracted_paths.primary.clone();
        let extras = extracted_paths.extras.clone();
        extracted = Some(extracted_paths);
        (primary, extras)
    } else {
        (download_path, Vec::new())
    };

    Ok(PreparedBinary {
        owner,
        name,
        tag: info.tag,
        asset_name,
        asset_url,
        path: payload_path,
        extra_paths,
        _download_dir: temp_dir,
        _extracted: extracted,
    })
}

fn github_client() -> Result<Client> {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::USER_AGENT,
        reqwest::header::HeaderValue::from_static("yoink"),
    );
    headers.insert(
        reqwest::header::ACCEPT,
        reqwest::header::HeaderValue::from_static("application/vnd.github+json"),
    );
    headers.insert(
        "X-GitHub-Api-Version",
        reqwest::header::HeaderValue::from_static("2022-11-28"),
    );

    if let Some(token) = github_token() {
        let value = format!("token {}", token);
        let header =
            reqwest::header::HeaderValue::from_str(&value).context("parse GitHub token header")?;
        headers.insert(reqwest::header::AUTHORIZATION, header);
    }

    Client::builder()
        .default_headers(headers)
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .context("build http client")
}

fn github_token() -> Option<String> {
    env::var("YOINK_GITHUB_TOKEN")
        .ok()
        .or_else(|| env::var("GITHUB_TOKEN").ok())
}

fn github_api_base() -> String {
    env::var("YOINK_GITHUB_API_BASE").unwrap_or_else(|_| "https://api.github.com".to_string())
}

fn resolve_release_info(client: &Client, owner: &str, repo: &str) -> Result<ReleaseInfo> {
    let release = fetch_latest_release(client, owner, repo)?;
    let asset = pick_asset(&release.assets)?;
    let tag = release.tag_name.as_deref().unwrap_or("unknown").to_string();

    Ok(ReleaseInfo {
        owner: owner.to_string(),
        name: repo.to_string(),
        tag,
        asset_name: asset.name,
        asset_url: asset.browser_download_url,
    })
}

fn fetch_latest_release(client: &Client, owner: &str, repo: &str) -> Result<Release> {
    let base = github_api_base();
    let base = base.trim_end_matches('/');
    let url = format!("{base}/repos/{owner}/{repo}/releases/latest");
    let response = client
        .get(&url)
        .send()
        .with_context(|| format!("fetch latest release for {owner}/{repo}"))?
        .error_for_status()
        .with_context(|| format!("bad response for {owner}/{repo}"))?;
    response
        .json::<Release>()
        .with_context(|| format!("parse release for {owner}/{repo}"))
}

fn pick_asset(assets: &[Asset]) -> Result<Asset> {
    if assets.is_empty() {
        bail!("release has no assets")
    }

    let mut candidates: Vec<&Asset> = assets
        .iter()
        .filter(|asset| !is_ignored_asset(&asset.name))
        .collect();
    if candidates.is_empty() {
        candidates = assets.iter().collect();
    }

    let os_tokens = os_tokens();
    let arch_tokens = arch_tokens();

    let mut best: Option<(&Asset, i32)> = None;
    for asset in candidates {
        let score = asset_score(&asset.name, &os_tokens, &arch_tokens);
        if best
            .map(|(_, best_score)| score > best_score)
            .unwrap_or(true)
        {
            best = Some((asset, score));
        }
    }

    best.map(|(asset, _)| asset.clone())
        .context("no suitable assets")
}

fn asset_score(name: &str, os_tokens: &[&str], arch_tokens: &[&str]) -> i32 {
    let lower = name.to_lowercase();
    let mut score = 0;

    if contains_any(&lower, os_tokens) {
        score += 2;
    }
    if contains_any(&lower, arch_tokens) {
        score += 2;
    }
    if is_archive_name(&lower) {
        score += 1;
    }
    if lower.ends_with(".exe") {
        score += 1;
    }
    score
}

fn contains_any(haystack: &str, tokens: &[&str]) -> bool {
    tokens.iter().any(|token| haystack.contains(token))
}

fn os_tokens() -> Vec<&'static str> {
    match env::consts::OS {
        "macos" => vec!["darwin", "macos", "osx", "mac", "apple-darwin"],
        "linux" => vec!["linux", "gnu", "unknown-linux"],
        "windows" => vec!["windows", "win", "mingw", "msvc"],
        other => vec![other],
    }
}

fn arch_tokens() -> Vec<&'static str> {
    match env::consts::ARCH {
        "x86_64" => vec!["x86_64", "amd64", "x64"],
        "aarch64" => vec!["aarch64", "arm64"],
        "arm" => vec!["armv7", "armv7l", "armv6", "arm"],
        other => vec![other],
    }
}

fn is_ignored_asset(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".sha256")
        || lower.ends_with(".sha256sum")
        || lower.ends_with(".sha512")
        || lower.ends_with(".sig")
        || lower.ends_with(".asc")
        || lower.ends_with(".md5")
        || lower.contains("checksum")
        || lower.contains("checksums")
        || lower.contains("sbom")
}

fn is_archive_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".zip")
        || lower.ends_with(".tar.gz")
        || lower.ends_with(".tgz")
        || lower.ends_with(".tar.xz")
        || lower.ends_with(".tar.bz2")
}

fn is_gzip_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".gz") && !lower.ends_with(".tar.gz")
}

fn download_asset(client: &Client, url: &str, dest: &Path) -> Result<()> {
    let mut response = client
        .get(url)
        .send()
        .with_context(|| format!("download asset {url}"))?
        .error_for_status()
        .with_context(|| format!("bad download response {url}"))?;
    let mut file = fs::File::create(dest)
        .with_context(|| format!("create download file {}", dest.display()))?;
    io::copy(&mut response, &mut file)
        .with_context(|| format!("write download to {}", dest.display()))?;
    Ok(())
}

struct ExtractedPaths {
    primary: PathBuf,
    extras: Vec<PathBuf>,
    _temp_dir: TempDir,
}

fn extract_archive(archive_path: &Path, repo_name: &str) -> Result<ExtractedPaths> {
    let temp_dir = tempfile::tempdir().context("create extract dir")?;
    let extract_root = temp_dir.path();

    let name = archive_path
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or("")
        .to_lowercase();

    if name.ends_with(".zip") {
        extract_zip(archive_path, extract_root)?;
    } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        extract_tar_gz(archive_path, extract_root)?;
    } else if name.ends_with(".tar.xz") {
        extract_tar_xz(archive_path, extract_root)?;
    } else if name.ends_with(".tar.bz2") {
        extract_tar_bz2(archive_path, extract_root)?;
    } else {
        bail!("unsupported archive format: {}", archive_path.display());
    }

    let (primary, extras) = find_binaries(extract_root, repo_name)?;
    Ok(ExtractedPaths {
        primary,
        extras,
        _temp_dir: temp_dir,
    })
}

fn extract_gzip(gzip_path: &Path, repo_name: &str) -> Result<ExtractedPaths> {
    let temp_dir = tempfile::tempdir().context("create extract dir")?;
    let extract_root = temp_dir.path();

    let filename = gzip_path
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or("download");
    let dest_name = filename.trim_end_matches(".gz");
    let dest = extract_root.join(dest_name);

    let mut input =
        fs::File::open(gzip_path).with_context(|| format!("open {}", gzip_path.display()))?;
    let mut decoder = flate2::read::GzDecoder::new(&mut input);
    let mut output =
        fs::File::create(&dest).with_context(|| format!("create {}", dest.display()))?;
    io::copy(&mut decoder, &mut output).with_context(|| format!("write {}", dest.display()))?;

    let (primary, extras) = if dest_name == repo_name {
        (dest, Vec::new())
    } else {
        find_binaries(extract_root, repo_name)?
    };

    Ok(ExtractedPaths {
        primary,
        extras,
        _temp_dir: temp_dir,
    })
}

fn extract_zip(archive_path: &Path, dest: &Path) -> Result<()> {
    let file =
        fs::File::open(archive_path).with_context(|| format!("open {}", archive_path.display()))?;
    let mut archive = zip::ZipArchive::new(file)
        .with_context(|| format!("open zip {}", archive_path.display()))?;

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .with_context(|| format!("read zip entry {i}"))?;
        let out_path = dest.join(entry.mangled_name());
        if entry.is_dir() {
            fs::create_dir_all(&out_path)
                .with_context(|| format!("create {}", out_path.display()))?;
            continue;
        }
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        }
        let mut outfile = fs::File::create(&out_path)
            .with_context(|| format!("create {}", out_path.display()))?;
        io::copy(&mut entry, &mut outfile)
            .with_context(|| format!("write {}", out_path.display()))?;
    }

    Ok(())
}

fn extract_tar_gz(archive_path: &Path, dest: &Path) -> Result<()> {
    let file =
        fs::File::open(archive_path).with_context(|| format!("open {}", archive_path.display()))?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    archive
        .unpack(dest)
        .with_context(|| format!("unpack {}", archive_path.display()))
}

fn extract_tar_xz(archive_path: &Path, dest: &Path) -> Result<()> {
    let file =
        fs::File::open(archive_path).with_context(|| format!("open {}", archive_path.display()))?;
    let decoder = xz2::read::XzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    archive
        .unpack(dest)
        .with_context(|| format!("unpack {}", archive_path.display()))
}

fn extract_tar_bz2(archive_path: &Path, dest: &Path) -> Result<()> {
    let file =
        fs::File::open(archive_path).with_context(|| format!("open {}", archive_path.display()))?;
    let decoder = bzip2::read::BzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    archive
        .unpack(dest)
        .with_context(|| format!("unpack {}", archive_path.display()))
}

fn find_binaries(root: &Path, repo_name: &str) -> Result<(PathBuf, Vec<PathBuf>)> {
    let target = binary_name(repo_name).to_lowercase();
    let fallback = repo_name.to_lowercase();

    let mut exact_matches = Vec::new();
    let mut candidates = Vec::new();
    let mut probable_matches = Vec::new();

    for entry in WalkDir::new(root) {
        let entry = entry.context("walk archive")?;
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or("")
            .to_lowercase();
        if name == target || name == fallback {
            exact_matches.push(path.to_path_buf());
        }
        candidates.push(path.to_path_buf());
        if is_probable_binary_candidate(path) {
            probable_matches.push(path.to_path_buf());
        }
    }

    let primary = if exact_matches.len() == 1 {
        exact_matches.remove(0)
    } else if exact_matches.len() > 1 {
        exact_matches.sort_by_key(|path| path.to_string_lossy().len());
        exact_matches.remove(0)
    } else if probable_matches.len() == 1 {
        probable_matches[0].clone()
    } else if probable_matches.len() > 1 {
        let mut bin_matches: Vec<PathBuf> = probable_matches
            .iter()
            .filter(|path| path_has_component(path, "bin"))
            .cloned()
            .collect();
        if bin_matches.len() == 1 {
            bin_matches.remove(0)
        } else {
            let mut sorted = probable_matches.clone();
            sorted.sort_by_key(|path| path.to_string_lossy().len());
            sorted.remove(0)
        }
    } else if candidates.len() == 1 {
        candidates.remove(0)
    } else {
        bail!("unable to locate extracted binary");
    };

    let mut extras = Vec::new();
    if !probable_matches.is_empty() {
        extras = probable_matches
            .into_iter()
            .filter(|path| *path != primary)
            .collect();
        extras.sort_by_key(|path| path.to_string_lossy().len());
    }

    Ok((primary, extras))
}

fn path_has_component(path: &Path, needle: &str) -> bool {
    path.components().any(|component| {
        component
            .as_os_str()
            .to_str()
            .map(|segment| segment.eq_ignore_ascii_case(needle))
            .unwrap_or(false)
    })
}

fn is_probable_binary_candidate(path: &Path) -> bool {
    let name = match path.file_name().and_then(OsStr::to_str) {
        Some(name) => name.to_lowercase(),
        None => return false,
    };

    if name.starts_with('.')
        || name.starts_with("readme")
        || name.starts_with("license")
        || name.starts_with("changelog")
        || name.starts_with("notice")
        || name.starts_with("copying")
    {
        return false;
    }

    if path_has_component(path, "share")
        || path_has_component(path, "doc")
        || path_has_component(path, "docs")
        || path_has_component(path, "man")
        || path_has_component(path, "completions")
        || path_has_component(path, "completion")
    {
        return false;
    }

    if let Some(ext) = Path::new(&name).extension().and_then(OsStr::to_str) {
        let ext = ext.to_lowercase();
        if matches!(
            ext.as_str(),
            "md" | "txt"
                | "rst"
                | "json"
                | "yaml"
                | "yml"
                | "toml"
                | "ini"
                | "cfg"
                | "conf"
                | "1"
                | "2"
                | "3"
                | "4"
                | "5"
                | "6"
                | "7"
                | "8"
                | "9"
                | "asc"
                | "sig"
                | "sha256"
                | "sha512"
                | "md5"
        ) {
            return false;
        }
    }

    true
}

fn install_payload(payload_path: &Path, dest: &Path) -> Result<()> {
    if let Err(err) = install_binary(payload_path, dest) {
        if is_permission_denied(&err) {
            install_with_sudo(payload_path, dest)?;
        } else {
            return Err(err);
        }
    }
    Ok(())
}

fn default_install_dir() -> Result<PathBuf> {
    if let Ok(dir) = env::var("YOINKDIR") {
        return Ok(PathBuf::from(dir));
    }
    if let Ok(dir) = env::var("YOINK_BIN_DIR") {
        return Ok(PathBuf::from(dir));
    }
    if cfg!(windows) {
        let base = dirs_next::data_local_dir().context("determine local data dir")?;
        return Ok(base.join("Programs").join("yoink").join("bin"));
    }
    let home = env::var("HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(|| dirs_next::home_dir())
        .context("determine home dir")?;
    Ok(home.join(".local").join("bin"))
}

fn binary_name(repo_name: &str) -> String {
    if cfg!(windows) {
        format!("{repo_name}.exe")
    } else {
        repo_name.to_string()
    }
}

fn set_executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path)
            .with_context(|| format!("stat {}", path.display()))?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).with_context(|| format!("chmod {}", path.display()))?;
    }
    Ok(())
}

fn install_binary(payload_path: &Path, dest: &Path) -> Result<()> {
    fs::copy(payload_path, dest).with_context(|| format!("copy to {}", dest.display()))?;
    set_executable(dest)?;
    Ok(())
}

fn install_with_sudo(payload_path: &Path, dest: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        set_executable(payload_path)?;
        let status = Command::new("sudo")
            .arg("mv")
            .arg("--")
            .arg(payload_path)
            .arg(dest)
            .status()
            .with_context(|| {
                format!("run sudo mv {} {}", payload_path.display(), dest.display())
            })?;
        if !status.success() {
            bail!("sudo mv failed with status {}", status);
        }
        Ok(())
    }

    #[cfg(not(unix))]
    {
        let _ = payload_path;
        let _ = dest;
        bail!("install location requires permissions not supported on this platform");
    }
}

fn is_permission_denied(err: &anyhow::Error) -> bool {
    err.chain().any(|cause| {
        cause
            .downcast_ref::<io::Error>()
            .map(|io_err| io_err.kind() == io::ErrorKind::PermissionDenied)
            .unwrap_or(false)
    })
}

fn ensure_install_dir(install_dir: &Path) -> Result<()> {
    if let Err(err) = fs::create_dir_all(install_dir) {
        if err.kind() == io::ErrorKind::PermissionDenied {
            create_dir_with_sudo(install_dir)
                .with_context(|| format!("create install dir {}", install_dir.display()))?;
            return Ok(());
        }
        return Err(err).with_context(|| format!("create install dir {}", install_dir.display()));
    }
    Ok(())
}

fn create_dir_with_sudo(install_dir: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        let status = Command::new("sudo")
            .arg("mkdir")
            .arg("-p")
            .arg("--")
            .arg(install_dir)
            .status()
            .with_context(|| format!("run sudo mkdir -p {}", install_dir.display()))?;
        if !status.success() {
            bail!("sudo mkdir failed with status {}", status);
        }
        Ok(())
    }

    #[cfg(not(unix))]
    {
        let _ = install_dir;
        bail!("install location requires permissions not supported on this platform");
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct InstallState {
    #[serde(default)]
    installs: BTreeMap<String, InstallEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
struct InstallEntry {
    version: String,
    bin: PathBuf,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    bins: Vec<PathBuf>,
}

impl InstallEntry {
    fn all_bins(&self) -> impl Iterator<Item = &PathBuf> {
        std::iter::once(&self.bin).chain(self.bins.iter())
    }
}

fn record_install(repo: &str, version: &str, bins: &[PathBuf]) -> Result<()> {
    let state_path = state_path()?;
    if let Some(parent) = state_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create state dir {}", parent.display()))?;
    }

    let mut file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(&state_path)
        .with_context(|| format!("open state file {}", state_path.display()))?;
    file.lock_exclusive()
        .with_context(|| format!("lock state file {}", state_path.display()))?;

    let mut state = read_state_locked(&mut file)?;
    let (primary, extras) = bins
        .split_first()
        .context("record install without binaries")?;
    state.installs.insert(
        repo.to_string(),
        InstallEntry {
            version: version.to_string(),
            bin: primary.to_path_buf(),
            bins: extras.to_vec(),
        },
    );
    write_state_locked(&mut file, &state)?;
    file.unlock()
        .with_context(|| format!("unlock state file {}", state_path.display()))?;
    Ok(())
}

fn remove_install(repo: &str) -> Result<()> {
    let state_path = state_path()?;
    if !state_path.exists() {
        bail!("no installs recorded");
    }

    let mut file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&state_path)
        .with_context(|| format!("open state file {}", state_path.display()))?;
    file.lock_exclusive()
        .with_context(|| format!("lock state file {}", state_path.display()))?;

    let mut state = read_state_locked(&mut file)?;
    let entry = state
        .installs
        .remove(repo)
        .with_context(|| format!("{} not installed", repo))?;

    for bin in entry.all_bins() {
        let result = if bin.is_symlink() {
            fs::remove_file(bin)
        } else if bin.is_dir() {
            fs::remove_dir_all(bin)
        } else {
            fs::remove_file(bin)
        };
        match result {
            Ok(()) => {}
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => {
                file.unlock()
                    .with_context(|| format!("unlock state file {}", state_path.display()))?;
                return Err(err).with_context(|| format!("remove {}", bin.display()));
            }
        }
    }

    write_state_locked(&mut file, &state)?;
    file.unlock()
        .with_context(|| format!("unlock state file {}", state_path.display()))?;
    Ok(())
}

fn load_state() -> Result<InstallState> {
    let state_path = state_path()?;
    if !state_path.exists() {
        return Ok(InstallState::default());
    }

    let mut file = fs::File::open(&state_path)
        .with_context(|| format!("open state file {}", state_path.display()))?;
    file.lock_shared()
        .with_context(|| format!("lock state file {}", state_path.display()))?;
    let state = read_state_locked(&mut file)?;
    file.unlock()
        .with_context(|| format!("unlock state file {}", state_path.display()))?;
    Ok(state)
}

fn read_state_locked(file: &mut fs::File) -> Result<InstallState> {
    file.seek(SeekFrom::Start(0)).context("seek state file")?;
    let mut buf = String::new();
    file.read_to_string(&mut buf).context("read state file")?;
    let state = if buf.trim().is_empty() {
        InstallState::default()
    } else {
        serde_json::from_str(&buf).context("parse state json")?
    };
    Ok(state)
}

fn write_state_locked(file: &mut fs::File, state: &InstallState) -> Result<()> {
    file.set_len(0).context("truncate state file")?;
    file.seek(SeekFrom::Start(0)).context("seek state file")?;
    serde_json::to_writer_pretty(&mut *file, state).context("write state json")?;
    file.write_all(b"\n").context("write state newline")?;
    file.sync_all().context("sync state file")?;
    Ok(())
}

fn state_path() -> Result<PathBuf> {
    let base = dirs_next::data_dir()
        .or_else(|| dirs_next::home_dir().map(|dir| dir.join(".local").join("share")))
        .context("determine data dir")?;
    Ok(base.join("yoink").join("installed.json"))
}

fn display_version(version: &str) -> &str {
    if let Some(stripped) = version.strip_prefix('v') {
        if stripped
            .chars()
            .next()
            .map(|ch| ch.is_ascii_digit())
            .unwrap_or(false)
        {
            return stripped;
        }
    }
    version
}

fn exit_status_code(status: ExitStatus) -> i32 {
    if let Some(code) = status.code() {
        return code;
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return 128 + signal;
        }
    }
    1
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::collections::BTreeMap;
    use std::ffi::OsString;
    use std::io::{BufRead, BufReader, Cursor, Write};
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn state_write_roundtrip() {
        let mut file = tempfile::tempfile().expect("create temp file");
        let mut installs = BTreeMap::new();
        installs.insert(
            "mxcl/yoink".to_string(),
            InstallEntry {
                version: "v0.1.0".to_string(),
                bin: PathBuf::from("/tmp/yoink"),
                bins: Vec::new(),
            },
        );
        let state = InstallState { installs };

        write_state_locked(&mut file, &state).expect("write state");
        let read_back = read_state_locked(&mut file).expect("read state");

        assert_eq!(read_back.installs.len(), 1);
        let entry = read_back.installs.get("mxcl/yoink").expect("entry");
        assert_eq!(entry.version, "v0.1.0");
        assert_eq!(entry.bin, PathBuf::from("/tmp/yoink"));
    }

    #[test]
    fn parse_repo_validates_shape() {
        let (owner, name) = parse_repo("mxcl/yoink").expect("parse repo");
        assert_eq!(owner, "mxcl");
        assert_eq!(name, "yoink");
        assert!(parse_repo("mxcl").is_err());
        assert!(parse_repo("mxcl/yoink/extra").is_err());
        assert!(parse_repo("/yoink").is_err());
    }

    #[test]
    fn is_repo_shape_reports_validity() {
        assert!(is_repo_shape("mxcl/yoink"));
        assert!(!is_repo_shape("mxcl"));
    }

    #[test]
    fn display_version_strips_v_prefix() {
        assert_eq!(display_version("v1.2.3"), "1.2.3");
        assert_eq!(display_version("vbeta"), "vbeta");
        assert_eq!(display_version("1.2.3"), "1.2.3");
    }

    #[test]
    fn token_helpers_include_expected_tokens() {
        let os = os_tokens();
        match env::consts::OS {
            "macos" => assert!(os.contains(&"macos")),
            "linux" => assert!(os.contains(&"linux")),
            "windows" => assert!(os.contains(&"windows")),
            other => assert!(os.contains(&other)),
        }

        let arch = arch_tokens();
        match env::consts::ARCH {
            "x86_64" => assert!(arch.contains(&"x86_64")),
            "aarch64" => assert!(arch.contains(&"aarch64")),
            "arm" => assert!(arch.contains(&"arm")),
            other => assert!(arch.contains(&other)),
        }
    }

    #[test]
    fn asset_helpers_prefer_best_match() {
        let os = os_tokens();
        let arch = arch_tokens();
        let best_name = format!("tool-{}-{}.tar.gz", os[0], arch[0]);
        let assets = vec![
            Asset {
                name: "tool.sig".to_string(),
                browser_download_url: "http://example.com/tool.sig".to_string(),
            },
            Asset {
                name: format!("tool-{}", os[0]),
                browser_download_url: "http://example.com/tool-os".to_string(),
            },
            Asset {
                name: best_name.clone(),
                browser_download_url: "http://example.com/tool-best".to_string(),
            },
        ];
        let picked = pick_asset(&assets).expect("pick asset");
        assert_eq!(picked.name, best_name);
        assert!(is_ignored_asset("foo.sha256"));
        assert!(is_archive_name("foo.tar.gz"));
        assert!(is_gzip_name("foo.gz"));
        assert!(!is_gzip_name("foo.tar.gz"));
    }

    #[test]
    fn pick_asset_errors_on_empty_assets() {
        assert!(pick_asset(&[]).is_err());
    }

    #[test]
    fn pick_asset_falls_back_to_ignored_assets() {
        let assets = vec![
            Asset {
                name: "tool.sha256".to_string(),
                browser_download_url: "http://example.com/tool.sha256".to_string(),
            },
            Asset {
                name: "tool.sig".to_string(),
                browser_download_url: "http://example.com/tool.sig".to_string(),
            },
        ];
        let picked = pick_asset(&assets).expect("pick asset");
        assert!(picked.name.ends_with(".sha256") || picked.name.ends_with(".sig"));
    }

    #[test]
    fn asset_score_counts_exe() {
        assert_eq!(asset_score("tool.exe", &[], &[]), 1);
    }

    #[test]
    fn find_binaries_prefers_shortest_exact() {
        let temp = tempfile::tempdir().expect("temp dir");
        let root = temp.path();
        fs::create_dir_all(root.join("bin")).expect("mkdir");
        fs::create_dir_all(root.join("docs")).expect("mkdir docs");
        fs::write(root.join("tool"), b"bin").expect("write tool");
        fs::write(root.join("bin").join("tool"), b"bin").expect("write tool bin");
        fs::write(root.join("bin").join("helper"), b"bin").expect("write helper");
        fs::write(root.join("docs").join("readme.md"), b"doc").expect("write doc");

        let (primary, extras) = find_binaries(root, "tool").expect("find binaries");
        assert_eq!(primary, root.join("tool"));
        assert!(extras.contains(&root.join("bin").join("tool")));
        assert!(extras.contains(&root.join("bin").join("helper")));
    }

    #[test]
    fn find_binaries_prefers_bin_when_no_exact() {
        let temp = tempfile::tempdir().expect("temp dir");
        let root = temp.path();
        fs::create_dir_all(root.join("bin")).expect("mkdir");
        fs::create_dir_all(root.join("alt")).expect("mkdir");
        fs::write(root.join("bin").join("run"), b"bin").expect("write run");
        fs::write(root.join("alt").join("tool"), b"bin").expect("write tool");

        let (primary, _extras) = find_binaries(root, "yoink").expect("find binaries");
        assert_eq!(primary, root.join("bin").join("run"));
    }

    #[test]
    fn find_binaries_prefers_shortest_probable() {
        let temp = tempfile::tempdir().expect("temp dir");
        let root = temp.path();
        fs::create_dir_all(root.join("a")).expect("mkdir");
        fs::create_dir_all(root.join("longer").join("path")).expect("mkdir");
        fs::write(root.join("a").join("run"), b"bin").expect("write run");
        fs::write(root.join("longer").join("path").join("tool"), b"bin").expect("write tool");

        let (primary, _extras) = find_binaries(root, "yoink").expect("find binaries");
        assert_eq!(primary, root.join("a").join("run"));
    }

    #[test]
    fn find_binaries_single_candidate() {
        let temp = tempfile::tempdir().expect("temp dir");
        let root = temp.path();
        fs::write(root.join("only"), b"bin").expect("write file");
        let (primary, extras) = find_binaries(root, "yoink").expect("find binaries");
        assert_eq!(primary, root.join("only"));
        assert!(extras.is_empty());
    }

    #[test]
    fn find_binaries_falls_back_to_single_candidate() {
        let temp = tempfile::tempdir().expect("temp dir");
        let root = temp.path();
        fs::write(root.join("notes.txt"), b"doc").expect("write file");

        let (primary, _extras) = find_binaries(root, "yoink").expect("find binaries");
        assert_eq!(primary, root.join("notes.txt"));
    }

    #[test]
    fn find_binaries_errors_without_candidates() {
        let temp = tempfile::tempdir().expect("temp dir");
        let root = temp.path();
        assert!(find_binaries(root, "yoink").is_err());
    }

    #[test]
    fn probable_binary_filters_docs_and_extensions() {
        assert!(!is_probable_binary_candidate(Path::new("README.md")));
        assert!(!is_probable_binary_candidate(Path::new("docs/tool")));
        assert!(!is_probable_binary_candidate(Path::new("share/tool")));
        assert!(is_probable_binary_candidate(Path::new("bin/tool")));
    }

    #[test]
    fn probable_binary_handles_missing_filename() {
        assert!(!is_probable_binary_candidate(Path::new("")));
    }

    #[test]
    fn probable_binary_filters_extensions() {
        assert!(!is_probable_binary_candidate(Path::new("notes.json")));
    }

    #[test]
    fn path_has_component_handles_case() {
        let path = Path::new("Foo").join("Bin").join("tool");
        let path = path.as_path();
        assert!(path_has_component(path, "bin"));
        assert!(!path_has_component(path, "share"));
    }

    #[test]
    fn install_binary_copies_and_sets_mode() {
        let temp = tempfile::tempdir().expect("temp dir");
        let src = temp.path().join("src");
        let dest = temp.path().join("dest");
        fs::write(&src, b"hello").expect("write");

        install_binary(&src, &dest).expect("install binary");
        let contents = fs::read(&dest).expect("read dest");
        assert_eq!(contents, b"hello");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&dest).expect("stat").permissions().mode();
            assert!(mode & 0o111 != 0);
        }
    }

    #[test]
    fn install_payload_handles_normal_copy() {
        let temp = tempfile::tempdir().expect("temp dir");
        let src = temp.path().join("src");
        let dest = temp.path().join("dest");
        fs::write(&src, b"hello").expect("write");
        install_payload(&src, &dest).expect("install payload");
        assert!(dest.exists());
    }

    #[test]
    fn install_payload_errors_on_missing_dest_parent() {
        let temp = tempfile::tempdir().expect("temp dir");
        let src = temp.path().join("src");
        let dest = temp.path().join("missing").join("dest");
        fs::write(&src, b"hello").expect("write");
        assert!(install_payload(&src, &dest).is_err());
    }

    #[test]
    fn is_permission_denied_detects_nested_error() {
        let err = anyhow::Error::new(io::Error::new(io::ErrorKind::PermissionDenied, "nope"));
        assert!(is_permission_denied(&err));

        let err = anyhow::Error::new(io::Error::new(io::ErrorKind::Other, "nope"));
        assert!(!is_permission_denied(&err));
    }

    #[test]
    #[cfg(unix)]
    fn exit_status_code_reads_code() {
        use std::os::unix::process::ExitStatusExt;
        let status = ExitStatus::from_raw(9);
        assert_eq!(exit_status_code(status), 137);

        let status = Command::new("sh")
            .arg("-c")
            .arg("exit 42")
            .status()
            .expect("run");
        assert_eq!(exit_status_code(status), 42);
    }

    #[test]
    #[serial]
    fn default_install_dir_prefers_env_vars() {
        let temp = tempfile::tempdir().expect("temp dir");
        let _guard = EnvGuard::set("YOINKDIR", temp.path());
        let dir = default_install_dir().expect("default install dir");
        assert_eq!(dir, temp.path());
    }

    #[test]
    #[serial]
    fn default_install_dir_uses_bin_env_when_set() {
        let temp = tempfile::tempdir().expect("temp dir");
        let _guard = EnvGuard::set("YOINK_BIN_DIR", temp.path());
        env::remove_var("YOINKDIR");
        let dir = default_install_dir().expect("default install dir");
        assert_eq!(dir, temp.path());
    }

    #[test]
    #[serial]
    fn ensure_install_dir_creates_path() {
        let temp = tempfile::tempdir().expect("temp dir");
        let install_dir = temp.path().join("bin");
        ensure_install_dir(&install_dir).expect("ensure install dir");
        assert!(install_dir.exists());
    }

    #[test]
    #[serial]
    fn state_loads_empty_when_missing() {
        let temp = tempfile::tempdir().expect("temp dir");
        let _home = EnvGuard::set("HOME", temp.path());
        let _xdg = EnvGuard::set("XDG_DATA_HOME", temp.path());

        let state = load_state().expect("load state");
        assert!(state.installs.is_empty());
    }

    #[test]
    #[serial]
    fn record_and_remove_install_updates_state() {
        let temp = tempfile::tempdir().expect("temp dir");
        let _home = EnvGuard::set("HOME", temp.path());
        let _xdg = EnvGuard::set("XDG_DATA_HOME", temp.path());

        let bin = temp.path().join("bin").join("yoink");
        let extra = temp.path().join("bin").join("helper");
        fs::create_dir_all(bin.parent().expect("bin parent")).expect("mkdir");
        fs::write(&bin, b"bin").expect("write bin");
        fs::write(&extra, b"bin").expect("write extra");

        record_install("mxcl/yoink", "v1.2.3", &[bin.clone(), extra.clone()])
            .expect("record install");

        let installs = list_installs().expect("list installs");
        assert_eq!(installs.len(), 1);
        assert_eq!(installs[0].version, "1.2.3");

        remove_install("mxcl/yoink").expect("remove install");
        assert!(!bin.exists());
        assert!(!extra.exists());
    }

    #[test]
    #[serial]
    fn remove_install_errors_when_missing() {
        let temp = tempfile::tempdir().expect("temp dir");
        let _home = EnvGuard::set("HOME", temp.path());
        let _xdg = EnvGuard::set("XDG_DATA_HOME", temp.path());
        assert!(remove_install("mxcl/yoink").is_err());
    }

    #[test]
    #[serial]
    fn remove_install_handles_directories() {
        let temp = tempfile::tempdir().expect("temp dir");
        let _home = EnvGuard::set("HOME", temp.path());
        let _xdg = EnvGuard::set("XDG_DATA_HOME", temp.path());

        let bin_dir = temp.path().join("bin_dir");
        fs::create_dir_all(&bin_dir).expect("mkdir");
        record_install("mxcl/yoink", "v1.0.0", &[bin_dir.clone()]).expect("record install");

        assert!(remove_install("mxcl/yoink").is_ok());
        assert!(!bin_dir.exists());
    }

    #[test]
    fn read_state_defaults_on_empty_file() {
        let mut file = tempfile::tempfile().expect("temp file");
        let state = read_state_locked(&mut file).expect("read state");
        assert!(state.installs.is_empty());
    }

    #[test]
    fn extract_zip_archive() {
        let temp = tempfile::tempdir().expect("temp dir");
        let archive = temp.path().join("tool.zip");
        write_zip(&archive, &[("tool", b"bin"), ("README.md", b"doc")]);

        let extracted = extract_archive(&archive, "tool").expect("extract zip");
        assert!(extracted.primary.ends_with("tool"));
        assert!(extracted.primary.exists());
    }

    #[test]
    fn extract_zip_directory_entries() {
        let temp = tempfile::tempdir().expect("temp dir");
        let archive = temp.path().join("tool.zip");
        let file = fs::File::create(&archive).expect("create zip");
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::FileOptions::default();
        zip.add_directory("bin/", options).expect("add dir");
        zip.start_file("bin/tool", options).expect("start file");
        zip.write_all(b"bin").expect("write file");
        zip.finish().expect("finish zip");

        let extracted = extract_archive(&archive, "tool").expect("extract zip");
        assert!(extracted.primary.ends_with("tool"));
    }

    #[test]
    fn extract_tar_gz_archive() {
        let temp = tempfile::tempdir().expect("temp dir");
        let archive = temp.path().join("tool.tar.gz");
        write_tar_gz(&archive, &[("tool", b"bin")]);

        let extracted = extract_archive(&archive, "tool").expect("extract tar.gz");
        assert!(extracted.primary.ends_with("tool"));
    }

    #[test]
    fn extract_tar_xz_archive() {
        let temp = tempfile::tempdir().expect("temp dir");
        let archive = temp.path().join("tool.tar.xz");
        write_tar_xz(&archive, &[("tool", b"bin")]);

        let extracted = extract_archive(&archive, "tool").expect("extract tar.xz");
        assert!(extracted.primary.ends_with("tool"));
    }

    #[test]
    fn extract_tar_bz2_archive() {
        let temp = tempfile::tempdir().expect("temp dir");
        let archive = temp.path().join("tool.tar.bz2");
        write_tar_bz2(&archive, &[("tool", b"bin")]);

        let extracted = extract_archive(&archive, "tool").expect("extract tar.bz2");
        assert!(extracted.primary.ends_with("tool"));
    }

    #[test]
    fn extract_archive_rejects_unknown_format() {
        let temp = tempfile::tempdir().expect("temp dir");
        let archive = temp.path().join("tool.rar");
        fs::write(&archive, b"bad").expect("write");
        assert!(extract_archive(&archive, "tool").is_err());
    }

    #[test]
    fn extract_gzip_non_archive() {
        let temp = tempfile::tempdir().expect("temp dir");
        let archive = temp.path().join("payload.gz");
        write_gzip(&archive, b"bin");

        let extracted = extract_gzip(&archive, "tool").expect("extract gzip");
        assert!(extracted.primary.exists());
    }

    #[test]
    fn extract_gzip_matches_repo_name() {
        let temp = tempfile::tempdir().expect("temp dir");
        let archive = temp.path().join("tool.gz");
        write_gzip(&archive, b"bin");

        let extracted = extract_gzip(&archive, "tool").expect("extract gzip");
        assert!(extracted.primary.ends_with("tool"));
    }

    #[test]
    fn download_asset_from_local_server() {
        let server = TestServer::new(|base| {
            let mut responses = BTreeMap::new();
            responses.insert("/asset".to_string(), b"hello".to_vec());
            let _ = base;
            responses
        });

        let client = github_client().expect("client");
        let temp = tempfile::tempdir().expect("temp dir");
        let dest = temp.path().join("asset");
        let url = format!("{}/asset", server.base);
        download_asset(&client, &url, &dest).expect("download asset");
        assert_eq!(fs::read(&dest).expect("read"), b"hello");

        server.finish();
    }

    #[test]
    #[serial]
    fn github_client_uses_token_header() {
        let _guard = EnvGuard::set("YOINK_GITHUB_TOKEN", "token123");
        let _client = github_client().expect("client");
    }

    #[test]
    #[serial]
    fn release_info_uses_override_base() {
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
        let info = release_info("mxcl/tool").expect("release info");
        assert_eq!(info.owner, "mxcl");
        assert_eq!(info.name, "tool");
        assert_eq!(info.tag, "v1.0.0");
        assert_eq!(info.asset_name, "tool");

        server.finish();
    }

    #[test]
    #[serial]
    fn prepare_binary_downloads_and_extracts() {
        let server = TestServer::new(|base| {
            let mut responses = BTreeMap::new();
            let url = format!("{base}/download/tool.tar.gz");
            let body = format!(
                "{{\"tag_name\":\"v2.0.0\",\"assets\":[{{\"name\":\"tool.tar.gz\",\"browser_download_url\":\"{url}\"}}]}}"
            );
            let tar = make_tar_gz_bytes(&[("tool", b"bin")]);
            responses.insert(
                "/repos/mxcl/tool/releases/latest".to_string(),
                body.into_bytes(),
            );
            responses.insert("/download/tool.tar.gz".to_string(), tar);
            responses
        });

        let _guard = EnvGuard::set("YOINK_GITHUB_API_BASE", &server.base);
        let prepared = prepare_binary("mxcl/tool").expect("prepare binary");
        assert!(prepared.path.exists());
        assert_eq!(prepared.asset_name, "tool.tar.gz");
        assert!(prepared.extra_paths.is_empty());

        server.finish();
    }

    #[test]
    #[serial]
    fn prepare_binary_downloads_gzip() {
        let server = TestServer::new(|base| {
            let mut responses = BTreeMap::new();
            let url = format!("{base}/download/tool.gz");
            let body = format!(
                "{{\"tag_name\":\"v2.1.0\",\"assets\":[{{\"name\":\"tool.gz\",\"browser_download_url\":\"{url}\"}}]}}"
            );
            let gzip = make_gzip_bytes(b"bin");
            responses.insert(
                "/repos/mxcl/tool/releases/latest".to_string(),
                body.into_bytes(),
            );
            responses.insert("/download/tool.gz".to_string(), gzip);
            responses
        });

        let _guard = EnvGuard::set("YOINK_GITHUB_API_BASE", &server.base);
        let prepared = prepare_binary("mxcl/tool").expect("prepare binary");
        assert!(prepared.path.exists());
        assert_eq!(prepared.asset_name, "tool.gz");

        server.finish();
    }

    #[test]
    #[serial]
    fn download_to_dir_installs_extras() {
        let server = TestServer::new(|base| {
            let mut responses = BTreeMap::new();
            let url = format!("{base}/download/tool.zip");
            let body = format!(
                "{{\"tag_name\":\"v3.0.0\",\"assets\":[{{\"name\":\"tool.zip\",\"browser_download_url\":\"{url}\"}}]}}"
            );
            let zip = make_zip_bytes(&[("tool", b"bin"), ("helper", b"bin")]);
            responses.insert(
                "/repos/mxcl/tool/releases/latest".to_string(),
                body.into_bytes(),
            );
            responses.insert("/download/tool.zip".to_string(), zip);
            responses
        });

        let _guard = EnvGuard::set("YOINK_GITHUB_API_BASE", &server.base);
        let dest = tempfile::tempdir().expect("temp dir");
        let summary = download_to_dir("mxcl/tool", dest.path()).expect("download");
        assert_eq!(summary.tag, "v3.0.0");
        assert!(summary.primary_path.exists());
        assert_eq!(summary.paths.len(), 2);

        server.finish();
    }

    #[test]
    #[serial]
    fn download_to_dir_skips_duplicate_extras() {
        let server = TestServer::new(|base| {
            let mut responses = BTreeMap::new();
            let url = format!("{base}/download/tool.zip");
            let body = format!(
                "{{\"tag_name\":\"v3.1.0\",\"assets\":[{{\"name\":\"tool.zip\",\"browser_download_url\":\"{url}\"}}]}}"
            );
            let zip = make_zip_bytes(&[("bin/tool", b"bin"), ("alt/tool", b"bin")]);
            responses.insert(
                "/repos/mxcl/tool/releases/latest".to_string(),
                body.into_bytes(),
            );
            responses.insert("/download/tool.zip".to_string(), zip);
            responses
        });

        let _guard = EnvGuard::set("YOINK_GITHUB_API_BASE", &server.base);
        let dest = tempfile::tempdir().expect("temp dir");
        let summary = download_to_dir("mxcl/tool", dest.path()).expect("download");
        assert_eq!(summary.paths.len(), 1);

        server.finish();
    }

    #[test]
    #[serial]
    fn install_with_version_records_state() {
        let server = TestServer::new(|base| {
            let mut responses = BTreeMap::new();
            let url = format!("{base}/download/tool");
            let body = format!(
                "{{\"tag_name\":\"v4.0.0\",\"assets\":[{{\"name\":\"tool\",\"browser_download_url\":\"{url}\"}}]}}"
            );
            responses.insert(
                "/repos/mxcl/tool/releases/latest".to_string(),
                body.into_bytes(),
            );
            responses.insert("/download/tool".to_string(), b"bin".to_vec());
            responses
        });

        let home = tempfile::tempdir().expect("temp dir");
        let bin = tempfile::tempdir().expect("bin dir");
        let _home_guard = EnvGuard::set("HOME", home.path());
        let _xdg_guard = EnvGuard::set("XDG_DATA_HOME", home.path());
        let _dir_guard = EnvGuard::set("YOINKDIR", bin.path());
        let _api_guard = EnvGuard::set("YOINK_GITHUB_API_BASE", &server.base);

        let (path, version) = install_with_version("mxcl/tool").expect("install");
        assert!(path.exists());
        assert_eq!(version, "v4.0.0");

        let installs = list_installs().expect("list installs");
        assert_eq!(installs.len(), 1);

        server.finish();
    }

    #[test]
    #[serial]
    fn install_with_version_skips_duplicate_extras() {
        let server = TestServer::new(|base| {
            let mut responses = BTreeMap::new();
            let url = format!("{base}/download/tool.zip");
            let body = format!(
                "{{\"tag_name\":\"v4.1.0\",\"assets\":[{{\"name\":\"tool.zip\",\"browser_download_url\":\"{url}\"}}]}}"
            );
            let zip = make_zip_bytes(&[("bin/tool", b"bin"), ("alt/tool", b"bin")]);
            responses.insert(
                "/repos/mxcl/tool/releases/latest".to_string(),
                body.into_bytes(),
            );
            responses.insert("/download/tool.zip".to_string(), zip);
            responses
        });

        let home = tempfile::tempdir().expect("temp dir");
        let bin = tempfile::tempdir().expect("bin dir");
        let _home_guard = EnvGuard::set("HOME", home.path());
        let _xdg_guard = EnvGuard::set("XDG_DATA_HOME", home.path());
        let _dir_guard = EnvGuard::set("YOINKDIR", bin.path());
        let _api_guard = EnvGuard::set("YOINK_GITHUB_API_BASE", &server.base);

        let (path, _version) = install_with_version("mxcl/tool").expect("install");
        assert!(path.exists());

        server.finish();
    }

    #[test]
    #[serial]
    fn upgrade_all_installs_every_repo() {
        let server = TestServer::new(|base| {
            let mut responses = BTreeMap::new();
            let url = format!("{base}/download/tool");
            let body = format!(
                "{{\"tag_name\":\"v9.0.0\",\"assets\":[{{\"name\":\"tool\",\"browser_download_url\":\"{url}\"}}]}}"
            );
            responses.insert(
                "/repos/mxcl/tool/releases/latest".to_string(),
                body.into_bytes(),
            );
            responses.insert("/download/tool".to_string(), b"bin".to_vec());
            responses
        });

        let home = tempfile::tempdir().expect("temp dir");
        let bin = tempfile::tempdir().expect("bin dir");
        let _home_guard = EnvGuard::set("HOME", home.path());
        let _xdg_guard = EnvGuard::set("XDG_DATA_HOME", home.path());
        let _dir_guard = EnvGuard::set("YOINKDIR", bin.path());
        let _api_guard = EnvGuard::set("YOINK_GITHUB_API_BASE", &server.base);

        record_install("mxcl/tool", "v1.0.0", &[bin.path().join("tool")]).expect("record install");
        let upgrades = upgrade_all().expect("upgrade");
        assert_eq!(upgrades.len(), 1);
        assert_eq!(upgrades[0].version, "9.0.0");

        server.finish();
    }

    #[test]
    #[serial]
    fn uninstall_removes_install() {
        let temp = tempfile::tempdir().expect("temp dir");
        let _home = EnvGuard::set("HOME", temp.path());
        let _xdg = EnvGuard::set("XDG_DATA_HOME", temp.path());

        record_install("mxcl/yoink", "v1.0.0", &[temp.path().join("yoink")])
            .expect("record install");
        uninstall("mxcl/yoink").expect("uninstall");
        let state = load_state().expect("load state");
        assert!(state.installs.is_empty());
    }
    #[serial]
    #[cfg(unix)]
    fn run_executes_downloaded_binary() {
        let server = TestServer::new(|base| {
            let mut responses = BTreeMap::new();
            let url = format!("{base}/download/tool");
            let body = format!(
                "{{\"tag_name\":\"v5.0.0\",\"assets\":[{{\"name\":\"tool\",\"browser_download_url\":\"{url}\"}}]}}"
            );
            let script = b"#!/bin/sh\nexit 7\n";
            responses.insert(
                "/repos/mxcl/tool/releases/latest".to_string(),
                body.into_bytes(),
            );
            responses.insert("/download/tool".to_string(), script.to_vec());
            responses
        });

        let _api_guard = EnvGuard::set("YOINK_GITHUB_API_BASE", &server.base);
        let code = run("mxcl/tool", &[]).expect("run");
        assert_eq!(code, 7);

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

    fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let file = fs::File::create(path).expect("create zip");
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::FileOptions::default();
        for &(name, contents) in entries {
            zip.start_file(name, options).expect("start file");
            zip.write_all(contents).expect("write file");
        }
        zip.finish().expect("finish zip");
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

    fn write_tar_gz(path: &Path, entries: &[(&str, &[u8])]) {
        let file = fs::File::create(path).expect("create tar.gz");
        let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        write_tar_entries(encoder, entries, |encoder| {
            encoder.finish().expect("finish")
        });
    }

    fn write_tar_xz(path: &Path, entries: &[(&str, &[u8])]) {
        let file = fs::File::create(path).expect("create tar.xz");
        let encoder = xz2::write::XzEncoder::new(file, 6);
        write_tar_entries(encoder, entries, |encoder| {
            encoder.finish().expect("finish")
        });
    }

    fn write_tar_bz2(path: &Path, entries: &[(&str, &[u8])]) {
        let file = fs::File::create(path).expect("create tar.bz2");
        let encoder = bzip2::write::BzEncoder::new(file, bzip2::Compression::default());
        write_tar_entries(encoder, entries, |encoder| {
            encoder.finish().expect("finish")
        });
    }

    fn make_tar_gz_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let buffer = Cursor::new(Vec::new());
        let encoder = flate2::write::GzEncoder::new(buffer, flate2::Compression::default());
        let buffer = write_tar_entries(encoder, entries, |encoder| {
            encoder.finish().expect("finish")
        });
        buffer.into_inner()
    }

    fn make_gzip_bytes(contents: &[u8]) -> Vec<u8> {
        let buffer = Cursor::new(Vec::new());
        let mut encoder = flate2::write::GzEncoder::new(buffer, flate2::Compression::default());
        encoder.write_all(contents).expect("write gzip");
        encoder.finish().expect("finish gzip").into_inner()
    }

    fn write_tar_entries<W, F>(writer: W, entries: &[(&str, &[u8])], finish: F) -> W::Inner
    where
        W: std::io::Write + IntoInner,
        F: FnOnce(W) -> W::Inner,
    {
        let mut builder = tar::Builder::new(writer);
        for &(name, contents) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(contents.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(&mut header, name, &mut Cursor::new(contents))
                .expect("append data");
        }
        let writer = builder.into_inner().expect("tar inner");
        finish(writer)
    }

    trait IntoInner {
        type Inner;
    }

    impl IntoInner for flate2::write::GzEncoder<fs::File> {
        type Inner = fs::File;
    }

    impl IntoInner for flate2::write::GzEncoder<Cursor<Vec<u8>>> {
        type Inner = Cursor<Vec<u8>>;
    }

    impl IntoInner for xz2::write::XzEncoder<fs::File> {
        type Inner = fs::File;
    }

    impl IntoInner for bzip2::write::BzEncoder<fs::File> {
        type Inner = fs::File;
    }

    fn write_gzip(path: &Path, contents: &[u8]) {
        let file = fs::File::create(path).expect("create gzip");
        let mut encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        encoder.write_all(contents).expect("write gzip");
        encoder.finish().expect("finish gzip");
    }
}
