use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tracing::{debug, info};

const GOOGLE_FLASHSTATION_BASE: &str = "https://content-flashstation-pa.googleapis.com/v1/builds";
const GOOGLE_FLASHSTATION_KEY: &str = "AIzaSyD-bwHpMvFCN3PfRN4Txsw_ECg_iptNfMQ";

const PIF_GENERATOR_LATEST: &str =
    "https://api.github.com/repos/xprateek/PIF-JSON-Generator/releases/latest";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub struct PifPayload {
    #[serde(default)]
    pub product: String,
    #[serde(default)]
    pub device: String,
    #[serde(default)]
    pub manufacturer: String,
    #[serde(default)]
    pub brand: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub fingerprint: String,
    #[serde(default)]
    pub security_patch: String,
    #[serde(default, rename = "FIRST_API_LEVEL")]
    pub first_api_level: Option<String>,
    #[serde(default, rename = "DEVICE_INITIAL_SDK_INT")]
    pub device_initial_sdk_int: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub incremental: Option<String>,
    #[serde(default)]
    pub r#type: Option<String>,
    #[serde(default)]
    pub tags: Option<String>,
    #[serde(default)]
    pub tag: Option<String>,
    #[serde(default)]
    pub release: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct FlashstationResponse {
    #[serde(default, rename = "flashstationBuild")]
    builds: Vec<FlashstationBuild>,
}

#[derive(Debug, Deserialize)]
struct FlashstationBuild {
    product: String,
    #[serde(rename = "buildId")]
    build_id: String,
    #[serde(rename = "releaseCandidateName")]
    release_candidate_name: String,
    #[serde(default, rename = "previewMetadata")]
    preview_metadata: Option<PreviewMetadata>,
}

#[derive(Debug, Deserialize)]
struct PreviewMetadata {
    #[serde(default, rename = "releaseTrackName")]
    release_track_name: Option<String>,
}

fn map_model_name(product: &str) -> (&'static str, &'static str) {
    // Returns (Model Name, First API Level)
    match product {
        "akita_beta" | "akita" => ("Pixel 8a", "34"),
        "tokay_beta" | "tokay" => ("Pixel 9", "34"),
        "caiman_beta" | "caiman" => ("Pixel 9 Pro", "34"),
        "komodo_beta" | "komodo" => ("Pixel 9 Pro XL", "34"),
        "shiba_beta" | "shiba" => ("Pixel 8", "34"),
        "husky_beta" | "husky" => ("Pixel 8 Pro", "34"),
        "lynx_beta" | "lynx" => ("Pixel 7a", "33"),
        "cheetah_beta" | "cheetah" => ("Pixel 7 Pro", "33"),
        "panther_beta" | "panther" => ("Pixel 7", "33"),
        "tangorpro_beta" | "tangorpro" => ("Pixel Tablet", "33"),
        "felix_beta" | "felix" => ("Pixel Fold", "33"),
        "bluejay_beta" | "bluejay" => ("Pixel 6a", "32"),
        _ => ("Pixel", "34"),
    }
}

pub fn fetch_flashstation(product_name: &str) -> Result<PifPayload> {
    let product = if product_name.trim().is_empty() {
        "akita_beta"
    } else {
        product_name.trim()
    };

    let url = format!("{GOOGLE_FLASHSTATION_BASE}?product={product}&key={GOOGLE_FLASHSTATION_KEY}");
    debug!("querying Google Flashstation API for product: {product}");

    let resp = ureq::get(&url)
        .set("Referer", "https://flash.android.com")
        .set("User-Agent", "Mozilla/5.0 (Android; Mobile; rv:130.0)")
        .timeout(std::time::Duration::from_secs(30))
        .call()
        .with_context(|| format!("failed to connect to Google Flashstation API: {url}"))?;

    let resp_str = resp.into_string()
        .context("failed to read Google Flashstation response body")?;

    let parsed: FlashstationResponse = serde_json::from_str(&resp_str)
        .context("failed to parse Google Flashstation JSON response")?;

    let latest_build = parsed.builds.into_iter().rev().next()
        .ok_or_else(|| anyhow::anyhow!("no builds returned for product '{product}'"))?;

    let device = product.strip_suffix("_beta").unwrap_or(product);
    let (model, first_api) = map_model_name(product);

    let release = latest_build.preview_metadata
        .as_ref()
        .and_then(|p| p.release_track_name.as_deref())
        .and_then(|t| {
            if t.contains("16") { Some("16") }
            else if t.contains("15") { Some("15") }
            else if t.contains("14") { Some("14") }
            else { None }
        })
        .unwrap_or("15");

    let fingerprint = format!(
        "google/{product}/{device}:{release}/{}/{}:user/release-keys",
        latest_build.release_candidate_name,
        latest_build.build_id
    );

    // Derive security patch date (defaults to current month or system patch date)
    let patch_date = crate::security_patch::get_system_patch_date()
        .unwrap_or_else(|| "2026-10-05".to_string());

    info!("successfully generated PIF profile for {model} ({product}) from Google Flashstation");

    Ok(PifPayload {
        product: product.to_string(),
        device: device.to_string(),
        manufacturer: "Google".to_string(),
        brand: "google".to_string(),
        model: model.to_string(),
        fingerprint,
        security_patch: patch_date,
        first_api_level: Some(first_api.to_string()),
        device_initial_sdk_int: Some(first_api.to_string()),
        id: Some(latest_build.release_candidate_name),
        incremental: Some(latest_build.build_id),
        r#type: Some("user".to_string()),
        tags: Some("release-keys".to_string()),
        tag: Some("release-keys".to_string()),
        release: Some(release.to_string()),
        extra: BTreeMap::new(),
    })
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

pub fn fetch_generator(preferred_device: Option<&str>) -> Result<PifPayload> {
    debug!("querying PIF Generator GitHub releases: {PIF_GENERATOR_LATEST}");

    let resp = ureq::get(PIF_GENERATOR_LATEST)
        .set("User-Agent", "TrickyStore-Addon-Enhanced/1.0")
        .set("Accept", "application/vnd.github.v3+json")
        .timeout(std::time::Duration::from_secs(30))
        .call()
        .context("failed to fetch latest release from PIF Generator")?;

    let rel_str = resp.into_string()
        .context("failed to read PIF Generator release body")?;

    let release: GithubRelease = serde_json::from_str(&rel_str)
        .context("failed to parse PIF Generator release JSON")?;

    let pref = preferred_device.unwrap_or("Akita").to_ascii_lowercase();

    // Find asset matching preferred device, or fallback to first json asset
    let asset = release.assets.iter().find(|a| {
        let name_lower = a.name.to_ascii_lowercase();
        name_lower.ends_with(".json") && name_lower.contains(&pref)
    }).or_else(|| {
        release.assets.iter().find(|a| a.name.ends_with(".json"))
    }).ok_or_else(|| anyhow::anyhow!("no JSON asset found in PIF Generator release {}", release.tag_name))?;

    info!("downloading PIF asset '{}' from {}", asset.name, release.tag_name);

    let asset_resp = ureq::get(&asset.browser_download_url)
        .set("User-Agent", "TrickyStore-Addon-Enhanced/1.0")
        .timeout(std::time::Duration::from_secs(30))
        .call()
        .with_context(|| format!("failed to download PIF asset from {}", asset.browser_download_url))?;

    let asset_str = asset_resp.into_string()
        .context("failed to read PIF asset body")?;

    let payload: PifPayload = serde_json::from_str(&asset_str)
        .context("failed to parse downloaded asset as PIF JSON payload")?;

    info!("successfully fetched PIF payload for {} from generator", payload.model);
    Ok(payload)
}

pub fn fetch_custom(url: &str) -> Result<PifPayload> {
    if url.trim().is_empty() {
        bail!("custom URL is empty");
    }
    debug!("downloading custom PIF JSON from: {url}");

    let resp = ureq::get(url.trim())
        .set("User-Agent", "TrickyStore-Addon-Enhanced/1.0")
        .timeout(std::time::Duration::from_secs(30))
        .call()
        .with_context(|| format!("failed to fetch custom PIF from {url}"))?;

    let custom_str = resp.into_string()
        .context("failed to read custom PIF response body")?;

    let payload: PifPayload = serde_json::from_str(&custom_str)
        .context("failed to parse custom response as PIF JSON payload")?;

    info!("successfully loaded custom PIF payload: {}", payload.model);
    Ok(payload)
}
