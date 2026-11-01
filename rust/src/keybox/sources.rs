use anyhow::{Context, Result, bail};
use base64::Engine;
use tracing::{debug, info};

use crate::platform::network;

// TAKR community automated mirror (mirrors https://keybox.kowx712.cc without Cloudflare captcha)
const TAKR_CATALOG_URL: &str = "https://rawbin.dpejoh.com/catalog";
const TAKR_BASE_URL: &str = "https://rawbin.dpejoh.com/key";

// MeowDump / Integrity-Box Megatron
const MEOW_URL: &str =
    "https://github.com/MeowDump/Integrity-Box/releases/download/archive/Megatron";

const SPECTRE_SHUFFLED: &[u8] = b"1dgWnocayqxU3r6vA5lCIPYfHmkV08b4tz+KMsp2NQ9LRXihODwSj7BEFJ/ZuGTe";
const SPECTRE_STD: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn fetch_takr() -> Result<Vec<u8>> {
    debug!("fetching keybox from TAKR mirror (keybox.kowx712.cc)");

    let mut candidates: Vec<(String, String)> = Vec::new();
    if let Ok(cat_text) = network::download_text(TAKR_CATALOG_URL) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&cat_text) {
            if let Some(entries) = v.get("entries").and_then(|e| e.as_array()) {
                for e in entries {
                    let revoked = e.get("revoked").and_then(|r| r.as_bool()).unwrap_or(false);
                    if !revoked {
                        let src = e.get("source").and_then(|s| s.as_str()).unwrap_or("");
                        let ver = e.get("version").and_then(|s| s.as_str()).unwrap_or("");
                        if !src.is_empty() && !ver.is_empty() {
                            candidates.push((src.to_string(), ver.to_string()));
                        }
                    }
                }
            }
        }
    }

    // Known active fallbacks if catalog is momentarily unreachable
    if candidates.is_empty() {
        candidates = vec![
            ("TAKR".into(), "34".into()),
            ("TAKR".into(), "33".into()),
            ("TAKR".into(), "32".into()),
            ("TAKR".into(), "31".into()),
            ("TAKR".into(), "28".into()),
        ];
    }

    for (src, ver) in candidates {
        let url = format!("{TAKR_BASE_URL}/{src}/{ver}");
        debug!("attempting TAKR candidate: {url}");
        if let Ok(bytes) = network::download(&url) {
            if let Ok(xml) = decode_spectre(&bytes) {
                if xml.windows(18).any(|w| w == b"AndroidAttestation") {
                    info!("successfully fetched & decoded TAKR keybox ({src}/{ver})");
                    return Ok(xml);
                }
            }
        }
    }

    bail!("all TAKR keybox candidates failed")
}

pub fn decode_spectre(data: &[u8]) -> Result<Vec<u8>> {
    let mut table = [0u8; 256];
    for (&shuf, &std) in SPECTRE_SHUFFLED.iter().zip(SPECTRE_STD.iter()) {
        table[shuf as usize] = std;
    }

    let mut mapped = Vec::with_capacity(data.len());
    for &b in data {
        if b.is_ascii_whitespace() {
            continue;
        }
        let m = table[b as usize];
        if m != 0 {
            mapped.push(m);
        } else {
            mapped.push(b);
        }
    }

    let decoded = base64::engine::general_purpose::STANDARD
        .decode(&mapped)
        .context("spectre base64 decode failed")?;
    Ok(decoded)
}

pub fn fetch_meow() -> Result<Vec<u8>> {
    debug!("fetching keybox from MeowDump (Integrity-Box)");
    let bytes = network::download(MEOW_URL).context("MeowDump download failed")?;
    let decoded = decode_meow(bytes).context("MeowDump decode failed")?;
    if !decoded.windows(18).any(|w| w == b"AndroidAttestation") {
        bail!("MeowDump output missing AndroidAttestation");
    }
    info!("successfully fetched & decoded MeowDump keybox");
    Ok(decoded)
}

pub fn decode_meow(mut data: Vec<u8>) -> Result<Vec<u8>> {
    for step in 1..=10 {
        let clean: Vec<u8> = data.iter().copied().filter(|b| !b.is_ascii_whitespace()).collect();
        data = base64::engine::general_purpose::STANDARD
            .decode(&clean)
            .with_context(|| format!("meow base64 step {step} failed"))?;
    }
    let mut raw = hex_decode_bytes(&data).context("meow hex decode failed")?;
    rot13(&mut raw);
    Ok(raw)
}

fn hex_decode_bytes(s: &[u8]) -> Result<Vec<u8>> {
    let clean: Vec<u8> = s.iter().copied().filter(|b| b.is_ascii_hexdigit()).collect();
    if clean.len() % 2 != 0 {
        bail!("hex string has odd length");
    }
    (0..clean.len())
        .step_by(2)
        .map(|i| {
            let hex_str = std::str::from_utf8(&clean[i..i + 2])?;
            u8::from_str_radix(hex_str, 16).context("invalid hex byte")
        })
        .collect()
}

fn rot13(bytes: &mut [u8]) {
    for b in bytes.iter_mut() {
        if b.is_ascii_lowercase() {
            *b = b'a' + (*b - b'a' + 13) % 26;
        } else if b.is_ascii_uppercase() {
            *b = b'A' + (*b - b'A' + 13) % 26;
        }
    }
}

pub fn fetch_custom_url(url: &str) -> Result<Vec<u8>> {
    debug!("fetching keybox from custom URL: {}", url);
    let data = network::download(url)
        .context("custom URL download failed")?;

    let trimmed = data.iter().copied().filter(|b| !b.is_ascii_whitespace()).collect::<Vec<u8>>();
    if trimmed.starts_with(b"<") {
        return Ok(data);
    }

    if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(&trimmed) {
        if decoded.windows(18).any(|w| w == b"AndroidAttestation") {
            return Ok(decoded);
        }
    }

    Ok(data)
}

// Legacy aliases mapping to modern working sources
pub fn fetch_yurikey() -> Result<Vec<u8>> {
    fetch_takr()
}

pub fn fetch_upstream() -> Result<Vec<u8>> {
    fetch_meow()
}

pub fn compute_sha256(data: &[u8]) -> String {
    use std::io::Write;
    let output = std::process::Command::new("sha256sum")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            child.stdin.take().unwrap().write_all(data)?;
            child.wait_with_output()
        });
    match output {
        Ok(o) => {
            let out = String::from_utf8_lossy(&o.stdout);
            out.split_whitespace().next().unwrap_or("").to_string()
        }
        Err(_) => String::new(),
    }
}
