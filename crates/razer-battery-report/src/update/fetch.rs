//! The network side of the updater: the latest release and the download of its file.

use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest as _, Sha256};
use ureq::tls::{RootCerts, TlsConfig, TlsProvider};

use super::error::UpdateError;
use super::release::Asset;

const LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/xzeldon/razer-battery-report/releases/latest";
/// Debug builds fetch the release from this URL instead, so a local server can stand in.
#[cfg(debug_assertions)]
const URL_OVERRIDE_ENV: &str = "RBR_UPDATE_URL";
const ACCEPT: &str = "application/vnd.github+json";
const API_TIMEOUT: Duration = Duration::from_secs(30);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_mins(10);

pub fn latest_release() -> Result<Vec<u8>, UpdateError> {
    let mut response = get(&release_url(), API_TIMEOUT, true)?;
    response
        .body_mut()
        .read_to_vec()
        .map_err(|error| UpdateError::from_request(&error))
}

/// Downloads `asset` into `dir` and checks it against its digest. A file that fails the check
/// is deleted. Returns the path of the verified file.
pub fn download(asset: &Asset, dir: &Path) -> Result<PathBuf, UpdateError> {
    let save_error = |error: &io::Error| UpdateError::from_save(error, dir, asset.size);
    fs::create_dir_all(dir).map_err(|error| save_error(&error))?;
    let path = dir.join(&asset.name);
    let response = get(&asset.url, DOWNLOAD_TIMEOUT, false)?;
    let saved = File::create(&path)
        .and_then(|mut file| io::copy(&mut response.into_body().into_reader(), &mut file))
        .map_err(|error| match error.kind() {
            ErrorKind::StorageFull | ErrorKind::PermissionDenied => save_error(&error),
            _ => {
                log::warn!("updater: download stopped: {error}");
                UpdateError::Interrupted
            }
        });
    let verified = saved.and_then(|_| verify(&path, &asset.sha256));
    if verified.is_err() {
        let _ = fs::remove_file(&path);
    }
    verified.map(|()| path)
}

fn release_url() -> String {
    #[cfg(debug_assertions)]
    if let Ok(url) = std::env::var(URL_OVERRIDE_ENV) {
        return url;
    }
    LATEST_RELEASE_URL.to_owned()
}

/// Hashes the file as stored on disk, not the stream: antivirus software often removes or
/// locks a download right after it was written.
fn verify(path: &Path, expected: &str) -> Result<(), UpdateError> {
    let hash = fs::read(path).map(|bytes| {
        Sha256::digest(bytes)
            .iter()
            .fold(String::new(), |mut hex, byte| {
                let _ = write!(hex, "{byte:02x}");
                hex
            })
    });
    match hash {
        Ok(hash) if hash == expected => Ok(()),
        Ok(hash) => {
            log::warn!("updater: digest mismatch: expected {expected}, got {hash}");
            Err(UpdateError::Damaged)
        }
        Err(error) => {
            log::warn!("updater: cannot read the download back: {error}");
            Err(match error.kind() {
                ErrorKind::NotFound | ErrorKind::PermissionDenied => UpdateError::Blocked,
                _ => UpdateError::CannotSave {
                    path: path.display().to_string(),
                },
            })
        }
    }
}

/// A GET request that turns an error status into an [`UpdateError`]. Only the release API gets
/// the `Accept` header.
fn get(
    url: &str,
    timeout: Duration,
    api: bool,
) -> Result<ureq::http::Response<ureq::Body>, UpdateError> {
    let tls = TlsConfig::builder()
        .provider(TlsProvider::NativeTls)
        .root_certs(RootCerts::PlatformVerifier)
        .build();
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .tls_config(tls)
        .user_agent(format!(
            "{}/{}",
            env!("CARGO_PKG_NAME"),
            env!("CARGO_PKG_VERSION")
        ))
        .http_status_as_error(false)
        .https_only(!cfg!(debug_assertions))
        .timeout_global(Some(timeout))
        .build()
        .into();
    let request = agent.get(url);
    let request = match api {
        true => request.header("Accept", ACCEPT),
        false => request,
    };
    let response = request.call().map_err(|error| {
        log::warn!("updater: {url}: {error}");
        UpdateError::from_request(&error)
    })?;
    let header = |name| {
        response
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
    };
    let failure = UpdateError::from_status(
        response.status().as_u16(),
        header("x-ratelimit-remaining"),
        header("x-ratelimit-reset"),
        super::now(),
    );
    match failure {
        None => Ok(response),
        Some(error) => {
            log::warn!("updater: {url}: HTTP {}", response.status());
            Err(error)
        }
    }
}
