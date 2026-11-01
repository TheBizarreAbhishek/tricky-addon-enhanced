pub mod sources;

use std::path::Path;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use tracing::info;

pub use sources::PifPayload;
use crate::cli::PifAction;
use crate::config::Config;
use crate::platform::fs::atomic_write;

pub const PRIMARY_PIF: &str = "/data/adb/pif.json";
pub const MODULE_PIF_FIX: &str = "/data/adb/modules/playintegrityfix/custom.pif.json";
pub const MODULE_PIF_FORK: &str = "/data/adb/modules/playintegrityfork/custom.pif.json";
pub const TS_PIF: &str = "/data/adb/tricky_store/pif.json";

pub fn kill_gms() {
    info!("restarting Google Play Services attestation process (com.google.android.gms.unstable)");
    let _ = std::process::Command::new("killall")
        .args(["-9", "com.google.android.gms.unstable"])
        .output();

    let _ = std::process::Command::new("killall")
        .args(["com.google.android.gms"])
        .output();
}

pub fn apply(payload: &PifPayload, kill: bool) -> Result<()> {
    let json_bytes = serde_json::to_vec_pretty(payload)
        .context("failed to serialize PIF payload")?;

    // 1. Write to primary /data/adb/pif.json
    atomic_write(Path::new(PRIMARY_PIF), &json_bytes)
        .context("failed to write /data/adb/pif.json")?;

    // 2. Write to Tricky Store directory
    let ts_path = Path::new(TS_PIF);
    if let Some(parent) = ts_path.parent() {
        if parent.exists() {
            let _ = atomic_write(ts_path, &json_bytes);
        }
    }

    // 3. Write to PlayIntegrityFix module if present
    let fix_dir = Path::new("/data/adb/modules/playintegrityfix");
    if fix_dir.is_dir() {
        let _ = atomic_write(Path::new(MODULE_PIF_FIX), &json_bytes);
    }

    // 4. Write to PlayIntegrityFork module if present
    let fork_dir = Path::new("/data/adb/modules/playintegrityfork");
    if fork_dir.is_dir() {
        let _ = atomic_write(Path::new(MODULE_PIF_FORK), &json_bytes);
    }

    info!("applied PIF configuration: model={}, fingerprint={}", payload.model, payload.fingerprint);

    if kill {
        kill_gms();
    }

    Ok(())
}

pub fn fetch_and_apply(cfg: &Config) -> Result<PifPayload> {
    let payload = match cfg.pif.source.to_ascii_lowercase().as_str() {
        "generator" => sources::fetch_generator(Some(&cfg.pif.target_device))?,
        "custom" => sources::fetch_custom(&cfg.pif.custom_url)?,
        _ => sources::fetch_flashstation(&cfg.pif.target_device)?,
    };

    apply(&payload, cfg.pif.kill_gms)?;
    Ok(payload)
}

pub fn read_current() -> Option<PifPayload> {
    let candidates = [
        PRIMARY_PIF,
        MODULE_PIF_FIX,
        MODULE_PIF_FORK,
        TS_PIF,
    ];

    for path in candidates {
        if let Ok(content) = std::fs::read_to_string(path) {
            if let Ok(payload) = serde_json::from_str::<PifPayload>(&content) {
                return Some(payload);
            }
        }
    }

    None
}

pub fn handle_pif(action: PifAction, cfg: &Config) -> Result<()> {
    match action {
        PifAction::Fetch => {
            let pif = fetch_and_apply(cfg)?;
            println!("PIF successfully updated:");
            println!("  Model:       {}", pif.model);
            println!("  Product:     {}", pif.product);
            println!("  Fingerprint: {}", pif.fingerprint);
            println!("  Patch:       {}", pif.security_patch);
            Ok(())
        }
        PifAction::Show => {
            match read_current() {
                Some(pif) => {
                    println!("{}", serde_json::to_string_pretty(&pif)?);
                }
                None => {
                    println!("No active pif.json found");
                }
            }
            Ok(())
        }
        PifAction::SetCustom { path } => {
            let content = std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read custom PIF file at '{path}'"))?;
            let payload: PifPayload = serde_json::from_str(&content)
                .context("file content is not valid PIF JSON")?;
            apply(&payload, cfg.pif.kill_gms)?;
            println!("Custom PIF applied successfully (Model: {})", payload.model);
            Ok(())
        }
        PifAction::Apply => {
            match read_current() {
                Some(pif) => {
                    apply(&pif, true)?;
                    println!("PIF reapplied and GMS restarted");
                }
                None => {
                    bail!("No current PIF found to apply. Run 'ta-enhanced pif fetch' first.");
                }
            }
            Ok(())
        }
        PifAction::KillGms => {
            kill_gms();
            println!("Google Play Services killed");
            Ok(())
        }
    }
}
