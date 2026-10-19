//! Public release metadata and checksum admission; these values grant no install authority.
use serde::Deserialize;
use url::Url;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    LinuxAmd64,
    DarwinArm64,
    WindowsAmd64,
}
impl Target {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::LinuxAmd64 => "linux-amd64",
            Self::DarwinArm64 => "darwin-arm64",
            Self::WindowsAmd64 => "windows-amd64",
        }
    }
    pub(super) fn root_name(self) -> String {
        format!("graphit-{}", self.name())
    }
    pub(super) fn archive_name(self) -> String {
        format!("{}.tar.gz", self.root_name())
    }
    pub(crate) fn binaries(self) -> [&'static str; 2] {
        match self {
            Self::WindowsAmd64 => ["graphit.exe", "graphit-model-server.exe"],
            _ => ["graphit", "graphit-model-server"],
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MetadataError {
    CurrentVersion,
    Json,
    ReleaseVersion,
    NotPublishedFullRelease,
    MissingAsset,
    AmbiguousAsset,
    InvalidAsset,
    MissingChecksum,
    AmbiguousChecksum,
    InvalidChecksum,
}
impl std::fmt::Display for MetadataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "release metadata: {self:?}")
    }
}
impl std::error::Error for MetadataError {}

pub(super) struct Asset {
    pub(super) url: Url,
    pub(super) bytes: u64,
}
impl std::fmt::Debug for Asset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Asset")
            .field("bytes", &self.bytes)
            .finish_non_exhaustive()
    }
}
#[derive(Debug)]
pub(super) struct Release {
    pub(super) tag: String,
    pub(super) target: Target,
    pub(super) archive: Asset,
    pub(super) checksum: Asset,
}
#[derive(Debug)]
pub(super) enum Decision {
    Current,
    Available(Box<Release>),
}
#[derive(Deserialize)]
struct ReleaseWire {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<AssetWire>,
}
#[derive(Deserialize)]
struct AssetWire {
    name: String,
    browser_download_url: String,
    size: u64,
}

/// The caller can skip fetching a release for an actual development build.
pub(super) fn current_version(value: &str) -> Result<Option<semver::Version>, MetadataError> {
    if value == "dev" {
        return Ok(None);
    }
    semver::Version::parse(value.strip_prefix('v').unwrap_or(value))
        .map(Some)
        .map_err(|_| MetadataError::CurrentVersion)
}

/// Bytes have already passed the captured HTTP response bound.
pub(super) fn select(
    bytes: &[u8],
    current: &semver::Version,
    target: Target,
    asset_origin: &Url,
) -> Result<Decision, MetadataError> {
    let wire: ReleaseWire = serde_json::from_slice(bytes).map_err(|_| MetadataError::Json)?;
    if wire.draft || wire.prerelease {
        return Err(MetadataError::NotPublishedFullRelease);
    }
    let version = semver::Version::parse(wire.tag_name.strip_prefix('v').unwrap_or(&wire.tag_name))
        .map_err(|_| MetadataError::ReleaseVersion)?;
    if !version.pre.is_empty() {
        return Err(MetadataError::NotPublishedFullRelease);
    }
    if version.cmp_precedence(current) != std::cmp::Ordering::Greater {
        return Ok(Decision::Current);
    }
    let archive_name = target.archive_name();
    let archive = asset(&wire.assets, &archive_name, &wire.tag_name, asset_origin)?;
    let checksum = asset(
        &wire.assets,
        &format!("{archive_name}.sha256"),
        &wire.tag_name,
        asset_origin,
    )?;
    Ok(Decision::Available(Box::new(Release {
        tag: wire.tag_name,
        target,
        archive,
        checksum,
    })))
}
fn asset(
    assets: &[AssetWire],
    name: &str,
    tag: &str,
    asset_origin: &Url,
) -> Result<Asset, MetadataError> {
    let mut matching = assets.iter().filter(|asset| asset.name == name);
    let asset = matching.next().ok_or(MetadataError::MissingAsset)?;
    if matching.next().is_some() {
        return Err(MetadataError::AmbiguousAsset);
    }
    if asset.size == 0 {
        return Err(MetadataError::InvalidAsset);
    }
    let url = Url::parse(&asset.browser_download_url).map_err(|_| MetadataError::InvalidAsset)?;
    if url.origin() != asset_origin.origin()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(MetadataError::InvalidAsset);
    }
    let segments = url
        .path_segments()
        .ok_or(MetadataError::InvalidAsset)?
        .collect::<Vec<_>>();
    if segments.len() != 6
        || segments[..4] != ["graphit-labs", "graphit-code", "releases", "download"]
    {
        return Err(MetadataError::InvalidAsset);
    }
    let decoded_tag = percent_encoding::percent_decode_str(segments[4])
        .decode_utf8()
        .map_err(|_| MetadataError::InvalidAsset)?;
    let decoded_name = percent_encoding::percent_decode_str(segments[5])
        .decode_utf8()
        .map_err(|_| MetadataError::InvalidAsset)?;
    if decoded_tag != tag || decoded_name != name {
        return Err(MetadataError::InvalidAsset);
    }
    Ok(Asset {
        url,
        bytes: asset.size,
    })
}

/// Match the exact archive name, as the maintained release installer does.
pub(super) fn checksum(bytes: &[u8], name: &str) -> Result<[u8; 32], MetadataError> {
    let text = std::str::from_utf8(bytes).map_err(|_| MetadataError::InvalidChecksum)?;
    let mut digest = None;
    for line in text.lines() {
        let mut fields = line.split_ascii_whitespace();
        let (Some(value), Some(file)) = (fields.next(), fields.next()) else {
            continue;
        };
        if file != name {
            continue;
        }
        if digest.is_some() {
            return Err(MetadataError::AmbiguousChecksum);
        }
        if fields.next().is_some()
            || value.len() != 64
            || !value.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(MetadataError::InvalidChecksum);
        }
        let mut decoded = [0; 32];
        for (index, byte) in decoded.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
                .map_err(|_| MetadataError::InvalidChecksum)?;
        }
        digest = Some(decoded);
    }
    digest.ok_or(MetadataError::MissingChecksum)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    fn wire(tag: &str, target: Target) -> Value {
        let archive = target.archive_name();
        json!({"tag_name":tag,"draft":false,"prerelease":false,"assets":[
            {"name":archive,"browser_download_url":format!("https://github.com/graphit-labs/graphit-code/releases/download/{tag}/{archive}"),"size":123},
            {"name":format!("{archive}.sha256"),"browser_download_url":format!("https://github.com/graphit-labs/graphit-code/releases/download/{tag}/{archive}.sha256"),"size":100}
        ]})
    }
    fn selected(value: &Value, target: Target) -> Result<Decision, MetadataError> {
        select(
            &serde_json::to_vec(value).unwrap(),
            &semver::Version::new(1, 0, 0),
            target,
            &Url::parse("https://github.com").unwrap(),
        )
    }
    #[test]
    fn exact_current_package_assets_and_versions_for_each_target() {
        for target in [
            Target::LinuxAmd64,
            Target::DarwinArm64,
            Target::WindowsAmd64,
        ] {
            let Decision::Available(release) = selected(&wire("v1.2.3", target), target).unwrap()
            else {
                panic!("new published release required")
            };
            assert_eq!(release.target, target);
            assert_eq!(release.tag, "v1.2.3");
            assert_eq!(release.archive.bytes, 123);
            assert!(release.checksum.url.path().ends_with(".tar.gz.sha256"));
            assert_eq!(
                target.binaries()[0].ends_with(".exe"),
                target == Target::WindowsAmd64
            );
        }
        assert!(current_version("dev").unwrap().is_none());
        assert_eq!(
            current_version("v1.2.3").unwrap(),
            Some(semver::Version::new(1, 2, 3))
        );
        assert_eq!(
            current_version("arbitrary-build"),
            Err(MetadataError::CurrentVersion)
        );
        for tag in ["v1.0.0", "v0.9.9"] {
            assert!(matches!(
                selected(&wire(tag, Target::LinuxAmd64), Target::LinuxAmd64).unwrap(),
                Decision::Current
            ));
        }
    }
    #[test]
    fn missing_duplicate_or_wrong_release_assets_cannot_be_promoted() {
        let target = Target::LinuxAmd64;
        let mut value = wire("v1.2.3", target);
        value["assets"][1]["name"] = json!("graphit-linux-amd64.sha256");
        assert!(matches!(
            selected(&value, target),
            Err(MetadataError::MissingAsset)
        ));
        let mut value = wire("v1.2.3", target);
        let duplicate = value["assets"][0].clone();
        value["assets"].as_array_mut().unwrap().push(duplicate);
        assert!(matches!(
            selected(&value, target),
            Err(MetadataError::AmbiguousAsset)
        ));
        for bad in [
            "https://github.com/foreign/repo/releases/download/v1.2.3/graphit-linux-amd64.tar.gz",
            "https://github.com/graphit-labs/graphit-code/releases/download/v1.2.2/graphit-linux-amd64.tar.gz",
            "http://github.com/graphit-labs/graphit-code/releases/download/v1.2.3/graphit-linux-amd64.tar.gz",
            "https://private:canary@github.com/graphit-labs/graphit-code/releases/download/v1.2.3/graphit-linux-amd64.tar.gz",
        ] {
            let mut value = wire("v1.2.3", target);
            value["assets"][0]["browser_download_url"] = json!(bad);
            assert!(matches!(
                selected(&value, target),
                Err(MetadataError::InvalidAsset)
            ));
        }
        for field in ["draft", "prerelease"] {
            let mut value = wire("v1.2.3", target);
            value[field] = json!(true);
            assert!(matches!(
                selected(&value, target),
                Err(MetadataError::NotPublishedFullRelease)
            ));
        }
    }
    #[test]
    fn encoded_tag_is_valid_and_asset_debug_never_prints_url_queries() {
        let target = Target::LinuxAmd64;
        let mut value = wire("v1.2.3+build", target);
        for asset in value["assets"].as_array_mut().unwrap() {
            asset["browser_download_url"] = json!(
                asset["browser_download_url"]
                    .as_str()
                    .unwrap()
                    .replace("+build", "%2Bbuild")
                    + "?private=fixture-canary"
            );
        }
        let decision = selected(&value, target).unwrap();
        assert!(!format!("{decision:?}").contains("fixture-canary"));
    }
    #[test]
    fn exact_named_checksum_is_required_and_ambiguity_is_not_ignored() {
        let name = "graphit-linux-amd64.tar.gz";
        let hash = "AB".repeat(32);
        assert_eq!(
            checksum(format!("{hash}  {name}\n").as_bytes(), name).unwrap(),
            [0xab; 32]
        );
        assert_eq!(
            checksum(format!("{hash}  other.tar.gz\n").as_bytes(), name),
            Err(MetadataError::MissingChecksum)
        );
        assert_eq!(
            checksum(format!("{hash}  {name}\n{hash}  {name}\n").as_bytes(), name),
            Err(MetadataError::AmbiguousChecksum)
        );
        assert_eq!(
            checksum(format!("bad  {name}\n").as_bytes(), name),
            Err(MetadataError::InvalidChecksum)
        );
        assert_eq!(
            checksum(format!("{hash}  {name} extra\n").as_bytes(), name),
            Err(MetadataError::InvalidChecksum)
        );
    }
}
