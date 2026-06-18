mod config;
mod init;
mod terminal;
mod vm;

use anyhow::Result;
use clap::Parser;
use config::VMConfig;

#[derive(Parser)]
#[command(
    name = "vm-rust-m4",
    about = "VM Manager - Ubuntu Server headless sur Apple Silicon",
    version
)]
struct Cli {
    /// Active les logs et messages de debug
    #[arg(short = 'v', long)]
    debug: bool,

    /// Mode daemon : lance la VM en arriere-plan (socket UNIX)
    #[arg(short = 'D', long)]
    daemon: bool,

    /// Initialise l'environnement (telecharge et prepare l'image cloud)
    #[arg(short = 'I', long)]
    init: bool,

    /// Taille du disque en Go (utilise avec --init)
    #[arg(long, default_value = "10")]
    disk_size: u32,

    /// URL de l'ISO Ubuntu ARM64 (utilise avec --init)
    #[arg(long)]
    iso_url: Option<String>,

    /// Fichier de configuration JSON
    #[arg(short, long, default_value = "vm_config.json")]
    config: String,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Logger : uniquement actif si --debug
    if cli.debug {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
            .format_timestamp_millis()
            .init();
    } else {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    }

    // ── Mode init ──
    if cli.init {
        log::info!("Mode initialisation...");
        init::bootstrap(cli.disk_size, cli.iso_url.as_deref())?;
        log::info!("Initialisation terminee. Relancez sans --init.");
        return Ok(());
    }

    // ── Verifier que l'image est prete ──
    if !init::is_ready() && !std::path::Path::new("data/ubuntu.iso").exists() {
        eprintln!("Aucune image trouvee. Lancez d'abord : vm-rust-m4 --init");
        anyhow::bail!("Environnement non initialise.");
    }

    // ── Nettoyage NVRAM corrompue ──
    if let Ok(meta) = std::fs::metadata("nvram.dat") {
        if meta.len() == 0 {
            log::info!("Suppression nvram.dat vide...");
            std::fs::remove_file("nvram.dat")?;
        }
    }

    // ── Charger config ──
    let config = VMConfig::load_or_default(&cli.config);

    if cli.debug {
        println!(
            "VM: {} | CPU: {} | RAM: {} Go | Mode: {}",
            config.name,
            config.cpu_count,
            config.memory_size_gb,
            if config.is_cloud_mode() {
                "Cloud pre-installee"
            } else {
                "Installation ISO"
            }
        );
    }

    // ── Mode daemon ──
    if cli.daemon {
        return run_daemon(config, cli.debug);
    }

    // ── Mode interactif (defaut) ──
    run_interactive(config, cli.debug)
}

fn run_interactive(config: VMConfig, debug: bool) -> Result<()> {
    terminal::print_banner(debug)?;

    let v = vm::VirtualMachine::new(config, debug);
    v.run()?;

    terminal::enable_raw_mode();
    terminal::install_handlers();

    vm::VirtualMachine::wait_forever();

    terminal::disable_raw_mode();
    Ok(())
}

fn run_daemon(config: VMConfig, debug: bool) -> Result<()> {
    log::info!("Lancement en mode daemon...");

    // TODO: fork + socket UNIX pour communication inter-processus
    // Pour l'instant, on lance simplement en interactif sans raw mode
    let v = vm::VirtualMachine::new(config, debug);
    v.run()?;

    log::info!("Daemon en attente (Ctrl+C pour arreter)...");
    vm::VirtualMachine::wait_forever();
    Ok(())
}
