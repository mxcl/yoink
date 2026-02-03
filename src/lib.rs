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

pub fn install(repo: &str) -> Result<PathBuf> {
    let (owner, name) = parse_repo(repo)?;
    let client = github_client()?;
    let release = fetch_latest_release(&client, &owner, &name)?;
    let asset = pick_asset(&release.assets)?;
    let version = release
        .tag_name
        .as_deref()
        .unwrap_or("unknown")
        .to_string();

    let temp_dir = tempfile::tempdir().context("create temp dir")?;
    let download_path = temp_dir.path().join(&asset.name);
    download_asset(&client, &asset.browser_download_url, &download_path)?;

    let mut _extracted = None;
    let payload_path = if is_archive_name(&asset.name) {
        let extracted_path = extract_archive(&download_path, &name)?;
        let path = extracted_path.path.clone();
        _extracted = Some(extracted_path);
        path
    } else if is_gzip_name(&asset.name) {
        let extracted_path = extract_gzip(&download_path, &name)?;
        let path = extracted_path.path.clone();
        _extracted = Some(extracted_path);
        path
    } else {
        download_path
    };

    let install_dir = default_install_dir()?;
    fs::create_dir_all(&install_dir)
        .with_context(|| format!("create install dir {}", install_dir.display()))?;

    let dest = install_dir.join(binary_name(&name));
    fs::copy(&payload_path, &dest)
        .with_context(|| format!("copy to {}", dest.display()))?;
    set_executable(&dest)?;
    record_install(&format!("{owner}/{name}"), &version, &dest)?;

    Ok(dest)
}

pub fn is_repo_shape(input: &str) -> bool {
    parse_repo(input).is_ok()
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

fn parse_repo(repo: &str) -> Result<(String, String)> {
    let mut parts = repo.split('/');
    let owner = parts.next().unwrap_or("");
    let name = parts.next().unwrap_or("");
    if owner.is_empty() || name.is_empty() || parts.next().is_some() {
        bail!("expected repo in owner/name form")
    }
    Ok((owner.to_string(), name.to_string()))
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
        let header = reqwest::header::HeaderValue::from_str(&value)
            .context("parse GitHub token header")?;
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

fn fetch_latest_release(client: &Client, owner: &str, repo: &str) -> Result<Release> {
    let url = format!(
        "https://api.github.com/repos/{owner}/{repo}/releases/latest"
    );
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

struct ExtractedPath {
    path: PathBuf,
    _temp_dir: TempDir,
}

fn extract_archive(archive_path: &Path, repo_name: &str) -> Result<ExtractedPath> {
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

    let path = find_binary(extract_root, repo_name)?;
    Ok(ExtractedPath {
        path,
        _temp_dir: temp_dir,
    })
}

fn extract_gzip(gzip_path: &Path, repo_name: &str) -> Result<ExtractedPath> {
    let temp_dir = tempfile::tempdir().context("create extract dir")?;
    let extract_root = temp_dir.path();

    let filename = gzip_path
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or("download");
    let dest_name = filename.trim_end_matches(".gz");
    let dest = extract_root.join(dest_name);

    let mut input = fs::File::open(gzip_path)
        .with_context(|| format!("open {}", gzip_path.display()))?;
    let mut decoder = flate2::read::GzDecoder::new(&mut input);
    let mut output = fs::File::create(&dest)
        .with_context(|| format!("create {}", dest.display()))?;
    io::copy(&mut decoder, &mut output)
        .with_context(|| format!("write {}", dest.display()))?;

    let path = if dest_name == repo_name {
        dest
    } else {
        find_binary(extract_root, repo_name)?
    };

    Ok(ExtractedPath {
        path,
        _temp_dir: temp_dir,
    })
}

fn extract_zip(archive_path: &Path, dest: &Path) -> Result<()> {
    let file = fs::File::open(archive_path)
        .with_context(|| format!("open {}", archive_path.display()))?;
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
            fs::create_dir_all(parent)
                .with_context(|| format!("create {}", parent.display()))?;
        }
        let mut outfile = fs::File::create(&out_path)
            .with_context(|| format!("create {}", out_path.display()))?;
        io::copy(&mut entry, &mut outfile)
            .with_context(|| format!("write {}", out_path.display()))?;
    }

    Ok(())
}

fn extract_tar_gz(archive_path: &Path, dest: &Path) -> Result<()> {
    let file = fs::File::open(archive_path)
        .with_context(|| format!("open {}", archive_path.display()))?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    archive
        .unpack(dest)
        .with_context(|| format!("unpack {}", archive_path.display()))
}

fn extract_tar_xz(archive_path: &Path, dest: &Path) -> Result<()> {
    let file = fs::File::open(archive_path)
        .with_context(|| format!("open {}", archive_path.display()))?;
    let decoder = xz2::read::XzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    archive
        .unpack(dest)
        .with_context(|| format!("unpack {}", archive_path.display()))
}

fn extract_tar_bz2(archive_path: &Path, dest: &Path) -> Result<()> {
    let file = fs::File::open(archive_path)
        .with_context(|| format!("open {}", archive_path.display()))?;
    let decoder = bzip2::read::BzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    archive
        .unpack(dest)
        .with_context(|| format!("unpack {}", archive_path.display()))
}

fn find_binary(root: &Path, repo_name: &str) -> Result<PathBuf> {
    let target = binary_name(repo_name).to_lowercase();
    let fallback = repo_name.to_lowercase();

    let mut exact_matches = Vec::new();
    let mut candidates = Vec::new();

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
        } else {
            candidates.push(path.to_path_buf());
        }
    }

    if exact_matches.len() == 1 {
        return Ok(exact_matches.remove(0));
    }
    if exact_matches.len() > 1 {
        exact_matches.sort_by_key(|path| path.to_string_lossy().len());
        return Ok(exact_matches.remove(0));
    }
    if candidates.len() == 1 {
        return Ok(candidates.remove(0));
    }

    bail!("unable to locate extracted binary")
}

fn default_install_dir() -> Result<PathBuf> {
    if let Ok(dir) = env::var("YOINK_BIN_DIR") {
        return Ok(PathBuf::from(dir));
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
        fs::set_permissions(path, perms)
            .with_context(|| format!("chmod {}", path.display()))?;
    }
    Ok(())
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
}

fn record_install(repo: &str, version: &str, bin: &Path) -> Result<()> {
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
    state.installs.insert(
        repo.to_string(),
        InstallEntry {
            version: version.to_string(),
            bin: bin.to_path_buf(),
        },
    );
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
    file.seek(SeekFrom::Start(0))
        .context("seek state file")?;
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
    file.seek(SeekFrom::Start(0))
        .context("seek state file")?;
    serde_json::to_writer_pretty(file, state).context("write state json")?;
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
