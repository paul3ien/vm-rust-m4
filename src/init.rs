use anyhow::Result;
use std::path::Path;
use std::process::Command;

const UBUNTU_ARM64_ISO_URL: &str =
    "https://cdimage.ubuntu.com/ubuntu-server/jammy/daily-live/current/jammy-server-cloudimg-arm64.img";
const UBUNTU_CLOUD_IMG: &str = "data/ubuntu-24.04-minimal-cloudimg-arm64.img";
const CLOUD_RAW_IMG: &str = "data/ubuntu-cloud-raw.img";

/// Verifie si l'image cloud pre-installee est prete.
pub fn is_ready() -> bool {
    Path::new(CLOUD_RAW_IMG).exists()
}

/// Lance la preparation complete :
/// 1. Telecharge l'image cloud QCOW2 (si absente)
/// 2. Lance prepare_cloud_image.sh
pub fn bootstrap(disk_size_gb: u32, _iso_url: Option<&str>) -> Result<()> {
    // Si l'image RAW existe deja, skip
    if Path::new(CLOUD_RAW_IMG).exists() {
        log::info!("Image cloud deja prete : {}", CLOUD_RAW_IMG);
        return Ok(());
    }

    // Verifier l'image QCOW2 source
    if !Path::new(UBUNTU_CLOUD_IMG).exists() {
        log::info!(
            "Telechargement de l'image cloud Ubuntu ARM64 ({} Mo)...",
            220
        );
        let status = Command::new("curl")
            .args([
                "-L",
                "-o",
                UBUNTU_CLOUD_IMG,
                "https://cloud-images.ubuntu.com/minimal/releases/noble/release/ubuntu-24.04-minimal-cloudimg-arm64.img",
            ])
            .status()?;

        if !status.success() {
            anyhow::bail!("Echec du telechargement de l'image cloud Ubuntu.");
        }
    }

    // Lancer prepare_cloud_image.sh avec la taille de disque
    log::info!("Preparation de l'image RAW ({} Go)...", disk_size_gb);
    let status = Command::new("bash")
        .args(["prepare_cloud_image.sh", &disk_size_gb.to_string()])
        .status()?;

    if !status.success() {
        anyhow::bail!("Echec de prepare_cloud_image.sh");
    }

    log::info!("Initialisation terminee.");
    Ok(())
}

/// Nettoie les fichiers temporaires (ISO d'installation par ex.)
pub fn cleanup_iso() {
    let iso = "data/ubuntu.iso";
    if Path::new(iso).exists() {
        log::info!("Suppression de l'ISO d'installation...");
        let _ = std::fs::remove_file(iso);
    }
}
