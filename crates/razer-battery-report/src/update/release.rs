//! The GitHub release JSON: whether the latest release is newer and which of its files fits
//! this computer.

use semver::Version;
use serde::{Deserialize, Serialize};

use super::error::UpdateError;

const FILE_PREFIX: &str = env!("CARGO_PKG_NAME");
const TAG_PREFIX: &str = "v";
const DIGEST_PREFIX: &str = "sha256:";

/// The parts of GitHub's `releases/latest` answer that matter here.
#[derive(Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
    assets: Vec<ReleaseAsset>,
}

#[derive(Deserialize)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
    size: u64,
    digest: Option<String>,
}

/// A release that is newer than the running app, with the file to install it from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Available {
    pub version: String,
    pub html_url: String,
    pub asset: Asset,
}

/// A downloadable file of a release.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asset {
    pub name: String,
    pub url: String,
    /// Lowercase hex SHA-256 of the file.
    pub sha256: String,
    pub size: u64,
}

/// Reads the `releases/latest` answer. `None` when the release is not newer than `current` or
/// is a pre-release. The file is the one named `<package>-v<version>-<target><suffix>` and
/// must carry a digest, otherwise the download could not be checked.
pub fn evaluate(
    json: &[u8],
    current: &Version,
    target: &str,
    suffix: &str,
) -> Result<Option<Available>, UpdateError> {
    let release: Release = serde_json::from_slice(json).map_err(|error| {
        log::warn!("updater: cannot read the release: {error}");
        UpdateError::NoAsset
    })?;
    let tag = release
        .tag_name
        .strip_prefix(TAG_PREFIX)
        .unwrap_or(&release.tag_name);
    let Ok(version) = Version::parse(tag) else {
        log::warn!("updater: unusable release tag {}", release.tag_name);
        return Err(UpdateError::NoAsset);
    };
    if !version.pre.is_empty() || version <= *current {
        return Ok(None);
    }
    let name = format!("{FILE_PREFIX}-{TAG_PREFIX}{version}-{target}{suffix}");
    let asset = release
        .assets
        .into_iter()
        .find(|asset| asset.name == name)
        .and_then(|asset| {
            let digest = asset.digest?;
            Some(Asset {
                sha256: digest.strip_prefix(DIGEST_PREFIX)?.to_ascii_lowercase(),
                name: asset.name,
                url: asset.browser_download_url,
                size: asset.size,
            })
        })
        .ok_or(UpdateError::NoAsset)?;
    Ok(Some(Available {
        version: version.to_string(),
        html_url: release.html_url,
        asset,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TARGET: &str = "x86_64-pc-windows-msvc";

    fn release(tag: &str, digest: &str) -> String {
        format!(
            r#"{{"tag_name":"{tag}","html_url":"page","assets":[
            {{"name":"razer-battery-report-v2.0.0-{TARGET}.exe","browser_download_url":"exe",
              "size":1,"digest":{digest}}},
            {{"name":"razer-battery-report-v2.0.0-{TARGET}-setup.exe","browser_download_url":"setup",
              "size":2,"digest":{digest}}}]}}"#
        )
    }

    fn evaluate(json: &str, current: &str, suffix: &str) -> Result<Option<Available>, UpdateError> {
        super::evaluate(
            json.as_bytes(),
            &Version::parse(current).unwrap(),
            TARGET,
            suffix,
        )
    }

    #[test]
    fn the_file_of_each_install_kind_is_chosen() {
        let json = release("v2.0.0", r#""sha256:ABC""#);
        let portable = evaluate(&json, "1.0.0", ".exe").unwrap().unwrap();
        assert_eq!(portable.asset.url, "exe");
        assert_eq!(portable.asset.sha256, "abc");
        let installer = evaluate(&json, "1.0.0", "-setup.exe").unwrap().unwrap();
        assert_eq!(installer.asset.url, "setup");
    }

    #[test]
    fn only_a_newer_release_with_a_digest_is_offered() {
        let json = release("v2.0.0", r#""sha256:abc""#);
        assert_eq!(evaluate(&json, "2.0.0", ".exe"), Ok(None));
        assert_eq!(evaluate(&json, "3.0.0", ".exe"), Ok(None));
        let pre = release("v2.0.0-rc.1", r#""sha256:abc""#);
        assert_eq!(evaluate(&pre, "1.0.0", ".exe"), Ok(None));
        let no_digest = release("v2.0.0", "null");
        assert_eq!(
            evaluate(&no_digest, "1.0.0", ".exe"),
            Err(UpdateError::NoAsset)
        );
    }
}
