use anyhow::Result;
use block::ConcreteBlock;
use objc::runtime::Object;
use objc::{class, msg_send, sel, sel_impl};
use objc_foundation::{INSString, NSString};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Parametres terminaux originaux, sauvegardes pour restauration a la sortie.
static mut ORIG_TERMIOS: Option<libc::termios> = None;

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
struct VMConfig {
    name: String,
    cpu_count: u32,
    memory_size_gb: u32,
    disk_path: Option<String>,
    disk_size_gb: u32,
    /// ISO d'installation (pour le mode installation).
    /// Si absent et seed_path present -> mode cloud-image pre-installee.
    iso_path: Option<String>,
    /// ISO cloud-init seed (cidata.iso) pour configurer l'image cloud au 1er boot.
    /// Contient user-data et meta-data.
    seed_path: Option<String>,
}

impl Default for VMConfig {
    fn default() -> Self {
        Self {
            name: "UbuntuEFI".to_string(),
            cpu_count: 4,
            memory_size_gb: 4,
            disk_path: Some("data/ubuntu-cloud-raw.img".to_string()),
            disk_size_gb: 10,
            iso_path: None,
            seed_path: Some("data/cidata.iso".to_string()),
        }
    }
}

impl VMConfig {
    fn load_or_default() -> Self {
        let path = "vm_config.json";
        if std::path::Path::new(path).exists() {
            if let Ok(content) = std::fs::read_to_string(path) {
                if let Ok(config) = serde_json::from_str::<VMConfig>(&content) {
                    println!("Configuration chargee depuis {}", path);
                    return config;
                } else {
                    eprintln!("Erreur de parsing dans {}, utilisation des defauts.", path);
                }
            }
        }
        Self::default()
    }
}

#[link(name = "Virtualization", kind = "framework")]
extern "C" {}

// --- Mode raw du terminal ----------------------------------------

fn enable_raw_mode() {
    unsafe {
        let mut termios: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(libc::STDIN_FILENO, &mut termios) == 0 {
            ORIG_TERMIOS = Some(termios);
            let mut raw = termios;
            libc::cfmakeraw(&mut raw);
            // Garder ISIG pour que Ctrl+C permette de quitter proprement
            raw.c_lflag |= libc::ISIG;
            libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &raw);
        }
    }
}

fn disable_raw_mode() {
    unsafe {
        if let Some(ref termios) = ORIG_TERMIOS {
            libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, termios);
        }
    }
}

extern "C" fn on_exit_cleanup() {
    disable_raw_mode();
}

extern "C" fn signal_handler(sig: libc::c_int) {
    disable_raw_mode();
    unsafe {
        libc::signal(sig, libc::SIG_DFL);
        libc::raise(sig);
    }
}

// --- VirtualMachine ----------------------------------------------

struct VirtualMachine {
    config: VMConfig,
}

impl VirtualMachine {
    fn new(config: VMConfig) -> Self {
        Self { config }
    }

    fn create_vm(&self) -> Result<()> {
        unsafe {
            let supported: bool = msg_send![class!(VZVirtualMachine), isSupported];
            if !supported {
                anyhow::bail!("Virtualisation non supportee.");
            }

            let vm_config: *mut Object = msg_send![class!(VZVirtualMachineConfiguration), new];

            // CPU & RAM
            let _: () = msg_send![vm_config, setCPUCount: self.config.cpu_count as u64];
            let memory_bytes = (self.config.memory_size_gb as u64) * 1024 * 1024 * 1024;
            let _: () = msg_send![vm_config, setMemorySize: memory_bytes];

            // EFI (BIOS)
            self.configure_efi(vm_config)?;

            // Peripheriques
            self.configure_devices(vm_config)?;

            // Validation
            let mut error: *mut Object = std::ptr::null_mut();
            let is_valid: bool = msg_send![vm_config, validateWithError: &mut error];
            if !is_valid {
                let error_msg = if !error.is_null() {
                    let desc: *mut Object = msg_send![error, localizedDescription];
                    let desc_str: *const i8 = msg_send![desc, UTF8String];
                    std::ffi::CStr::from_ptr(desc_str)
                        .to_string_lossy()
                        .to_string()
                } else {
                    "Erreur inconnue".to_string()
                };
                anyhow::bail!("Config invalide : {}", error_msg);
            }

            // Instanciation
            let vm: *mut Object = msg_send![class!(VZVirtualMachine), alloc];
            let vm: *mut Object = msg_send![vm, initWithConfiguration: vm_config];

            println!("Mode EFI active pour : {}", self.config.name);
            self.start_vm(vm)?;

            Ok(())
        }
    }

    unsafe fn configure_efi(&self, vm_config: *mut Object) -> Result<()> {
        println!("Configuration de l'EFI (Mode Headless)...");

        // 1. Bootloader
        let bootloader: *mut Object = msg_send![class!(VZEFIBootLoader), new];

        let nvram_path = PathBuf::from("nvram.dat");
        let nvram_url = self.path_to_nsurl(&nvram_path)?;

        let variable_store: *mut Object = msg_send![class!(VZEFIVariableStore), alloc];
        let variable_store: *mut Object = if nvram_path.exists() {
            msg_send![variable_store, initWithURL: nvram_url]
        } else {
            let mut error: *mut Object = std::ptr::null_mut();
            let res: *mut Object = msg_send![
                variable_store,
                initCreatingVariableStoreAtURL: nvram_url
                options: 0u64
                error: &mut error
            ];
            if res.is_null() {
                anyhow::bail!("Erreur NVRAM");
            }
            res
        };

        let _: () = msg_send![bootloader, setVariableStore: variable_store];
        let _: () = msg_send![vm_config, setBootLoader: bootloader];

        // 2. Platform
        let platform: *mut Object = msg_send![class!(VZGenericPlatformConfiguration), new];
        let machine_id: *mut Object = msg_send![class!(VZGenericMachineIdentifier), new];
        let _: () = msg_send![platform, setMachineIdentifier: machine_id];
        let _: () = msg_send![vm_config, setPlatform: platform];

        // 3. IMPORTANT : PAS DE CARTE GRAPHIQUE !
        // Cela force l'EFI a utiliser la console serie.

        Ok(())
    }

    unsafe fn configure_devices(&self, vm_config: *mut Object) -> Result<()> {
        // 1. Console Serie (seul moyen de communication en headless)
        let serial_config = self.create_interactive_console()?;
        let serial_array: *mut Object = msg_send![class!(NSMutableArray), new];
        let _: () = msg_send![serial_array, addObject: serial_config];
        let _: () = msg_send![vm_config, setSerialPorts: serial_array];

        // 2. Stockage
        let storage_array: *mut Object = msg_send![class!(NSMutableArray), new];

        // 2a. Disque principal (VirtIO block - rapide)
        if let Some(disk_path) = &self.config.disk_path {
            let disk = self.create_disk_device(disk_path)?;
            let _: () = msg_send![storage_array, addObject: disk];
        }

        // 2b. ISO d'installation - VirtIO block read-only (mode installation)
        if let Some(iso_path) = &self.config.iso_path {
            if std::path::Path::new(iso_path).exists() {
                let iso_dev = self.create_virtio_readonly_device(iso_path)?;
                let _: () = msg_send![storage_array, addObject: iso_dev];
                println!("  ISO d'installation montee en VirtIO : {}", iso_path);
            } else {
                println!("  Pas d'ISO trouvee a : {} (boot disque)", iso_path);
            }
        }

        // 2c. Cloud-init seed (cidata.iso) - pour image cloud pre-installee
        if let Some(seed_path) = &self.config.seed_path {
            if std::path::Path::new(seed_path).exists() {
                let seed_dev = self.create_virtio_readonly_device(seed_path)?;
                let _: () = msg_send![storage_array, addObject: seed_dev];
                println!("  Seed cloud-init montee en VirtIO : {}", seed_path);
            } else {
                eprintln!("  Seed cloud-init introuvable : {}", seed_path);
            }
        }

        let _: () = msg_send![vm_config, setStorageDevices: storage_array];

        // 3. Reseau (NAT)
        let nat: *mut Object = msg_send![class!(VZNATNetworkDeviceAttachment), new];
        let net: *mut Object = msg_send![class!(VZVirtioNetworkDeviceConfiguration), new];
        let _: () = msg_send![net, setAttachment: nat];
        let net_arr: *mut Object = msg_send![class!(NSMutableArray), new];
        let _: () = msg_send![net_arr, addObject: net];
        let _: () = msg_send![vm_config, setNetworkDevices: net_arr];

        // 4. Entropie (accelere la generation de cles SSH, etc.)
        let entropy: *mut Object = msg_send![class!(VZVirtioEntropyDeviceConfiguration), new];
        let entropy_arr: *mut Object = msg_send![class!(NSMutableArray), new];
        let _: () = msg_send![entropy_arr, addObject: entropy];
        let _: () = msg_send![vm_config, setEntropyDevices: entropy_arr];

        Ok(())
    }

    unsafe fn create_interactive_console(&self) -> Result<*mut Object> {
        let write_handle: *mut Object =
            msg_send![class!(NSFileHandle), fileHandleWithStandardOutput];
        let read_handle: *mut Object = msg_send![class!(NSFileHandle), fileHandleWithStandardInput];

        if write_handle.is_null() || read_handle.is_null() {
            anyhow::bail!("Terminal inaccessible.");
        }

        let attachment: *mut Object = msg_send![class!(VZFileHandleSerialPortAttachment), alloc];
        let attachment: *mut Object = msg_send![
            attachment,
            initWithFileHandleForReading: read_handle
            fileHandleForWriting: write_handle
        ];

        let serial_config: *mut Object =
            msg_send![class!(VZVirtioConsoleDeviceSerialPortConfiguration), new];
        let _: () = msg_send![serial_config, setAttachment: attachment];

        Ok(serial_config)
    }

    /// Cree un disque VirtIO block (pour le disque principal).
    unsafe fn create_disk_device(&self, path_str: &str) -> Result<*mut Object> {
        let path = PathBuf::from(path_str);

        if !path.exists() {
            anyhow::bail!(
                "Disque introuvable : {}. Lance ./prepare_cloud_image.sh d'abord.",
                path_str
            );
        }

        let url = self.path_to_nsurl(&path)?;
        let mut error: *mut Object = std::ptr::null_mut();

        let attachment: *mut Object = msg_send![class!(VZDiskImageStorageDeviceAttachment), alloc];
        let attachment: *mut Object = msg_send![
            attachment,
            initWithURL: url
            readOnly: false
            error: &mut error
        ];

        if attachment.is_null() {
            let err_msg = Self::nsobj_error_string(error);
            anyhow::bail!("Impossible d'attacher le disque {} : {}", path_str, err_msg);
        }

        let config: *mut Object = msg_send![class!(VZVirtioBlockDeviceConfiguration), alloc];
        let config: *mut Object = msg_send![config, initWithAttachment: attachment];
        Ok(config)
    }

    /// Cree un peripherique VirtIO block read-only (pour l'ISO).
    unsafe fn create_virtio_readonly_device(&self, path_str: &str) -> Result<*mut Object> {
        let path = PathBuf::from(path_str);
        let url = self.path_to_nsurl(&path)?;
        let mut error: *mut Object = std::ptr::null_mut();

        let attachment: *mut Object = msg_send![class!(VZDiskImageStorageDeviceAttachment), alloc];
        let attachment: *mut Object = msg_send![
            attachment,
            initWithURL: url
            readOnly: true
            error: &mut error
        ];

        if attachment.is_null() {
            let err_msg = Self::nsobj_error_string(error);
            anyhow::bail!("Impossible d'attacher l'ISO {} : {}", path_str, err_msg);
        }

        let config: *mut Object = msg_send![class!(VZVirtioBlockDeviceConfiguration), alloc];
        let config: *mut Object = msg_send![config, initWithAttachment: attachment];
        Ok(config)
    }

    /// Extrait le message d'erreur d'un NSError.
    unsafe fn nsobj_error_string(error: *mut Object) -> String {
        if !error.is_null() {
            let desc: *mut Object = msg_send![error, localizedDescription];
            let desc_str: *const i8 = msg_send![desc, UTF8String];
            std::ffi::CStr::from_ptr(desc_str)
                .to_string_lossy()
                .to_string()
        } else {
            "erreur inconnue".to_string()
        }
    }

    unsafe fn start_vm(&self, vm: *mut Object) -> Result<()> {
        println!("\nDemarrage de la VM...");
        let block = ConcreteBlock::new(|error: *mut Object| {
            if !error.is_null() {
                disable_raw_mode();
                let msg = VirtualMachine::nsobj_error_string(error);
                eprintln!("\nErreur au demarrage de la VM : {}", msg);
                std::process::exit(1);
            }
        });
        let block = block.copy();
        let _: () = msg_send![vm, startWithCompletionHandler: &*block];
        Ok(())
    }

    unsafe fn path_to_nsurl(&self, path: &PathBuf) -> Result<*mut Object> {
        let abs_path = if path.exists() {
            std::fs::canonicalize(path)?
        } else {
            std::env::current_dir()?.join(path)
        };

        let s = NSString::from_str(abs_path.to_str().unwrap());
        Ok(msg_send![class!(NSURL), fileURLWithPath: &*s])
    }
}

fn main() -> Result<()> {
    println!("=== VM Manager (EFI Headless) ===");

    // Nettoyage nvram corrompue
    if std::path::Path::new("nvram.dat").exists() {
        let metadata = std::fs::metadata("nvram.dat")?;
        if metadata.len() == 0 {
            println!("Suppression nvram.dat vide...");
            std::fs::remove_file("nvram.dat")?;
        }
    }

    let config = VMConfig::load_or_default();
    let disk_info = match &config.iso_path {
        Some(iso) if std::path::Path::new(iso).exists() => "Mode: Installation depuis ISO",
        _ => "Mode: Image cloud pre-installee",
    };
    println!(
        "VM: {} | CPU: {} | RAM: {} Go | {}",
        config.name, config.cpu_count, config.memory_size_gb, disk_info
    );

    let vm = VirtualMachine::new(config);
    vm.create_vm()?;

    println!("\n┌──────────────────────────────────────────────────┐");
    println!("│  Console serie active - mode raw                  │");
    println!("│  Fleches / Tab / touches speciales : OK           │");
    println!("│  Ctrl+C pour arreter la VM.                       │");
    println!("│                                                    │");
    println!("│  Login: root    Password: root                    │");
    println!("│  (defini via cloud-init seed)                     │");
    println!("└──────────────────────────────────────────────────┘");

    use std::io::Write;
    std::io::stdout().flush()?;

    enable_raw_mode();

    unsafe {
        libc::atexit(on_exit_cleanup);
        libc::signal(libc::SIGINT, signal_handler as libc::sighandler_t);
        libc::signal(libc::SIGTERM, signal_handler as libc::sighandler_t);
    }

    // Boucle principale (bloque ici indefiniment)
    unsafe {
        let run_loop: *mut Object = msg_send![class!(NSRunLoop), mainRunLoop];
        let _: () = msg_send![run_loop, run];
    }

    disable_raw_mode();
    Ok(())
}
