use objc::runtime::{Class, Object};
use objc::{class, msg_send, sel, sel_impl};
use objc_foundation::{INSString, NSString};
use objc_id::{Id, Owned};
use std::path::PathBuf;
use anyhow::{Result, Context};
use serde::{Deserialize, Serialize};
use block::ConcreteBlock;

// --- CONFIGURATION ---

#[derive(Debug, Serialize, Deserialize)]
struct VMConfig {
    name: String,
    cpu_count: u32,
    memory_size_gb: u32,
    kernel_path: String,
    initrd_path: String,
    disk_path: Option<String>,
    iso_path: Option<String>,
}

impl Default for VMConfig {
    fn default() -> Self {
        Self {
            name: "UbuntuVM".to_string(),
            cpu_count: 2,
            memory_size_gb: 4, // 4GB RAM pour être à l'aise avec l'installateur
            kernel_path: "data/vmlinuz".to_string(),
            initrd_path: "data/initrd.img".to_string(),
            disk_path: Some("data/disk.img".to_string()),
            iso_path: Some("data/ubuntu.iso".to_string()),
        }
    }
}

#[link(name = "Virtualization", kind = "framework")]
extern "C" {}

struct VirtualMachine {
    config: VMConfig,
}

impl VirtualMachine {
    fn new(config: VMConfig) -> Self {
        Self { config }
    }

    fn create_vm(&self) -> Result<()> {
        unsafe {
            // 1. Vérification du support
            let supported: bool = msg_send![class!(VZVirtualMachine), isSupported];
            if !supported {
                anyhow::bail!("Virtualisation non supportée sur ce Mac.");
            }

            let vm_config: *mut Object = msg_send![class!(VZVirtualMachineConfiguration), new];
            
            // 2. CPU & RAM
            let _: () = msg_send![vm_config, setCPUCount: self.config.cpu_count as u64];
            let memory_bytes = (self.config.memory_size_gb as u64) * 1024 * 1024 * 1024;
            let _: () = msg_send![vm_config, setMemorySize: memory_bytes];

            // 3. Bootloader (Kernel + Initrd)
            let bootloader = self.create_linux_bootloader()?;
            let _: () = msg_send![vm_config, setBootLoader: bootloader];

            // 4. Périphériques (Disques, ISO, Console, Réseau)
            self.configure_devices(vm_config)?;

            // 5. Validation
            let mut error: *mut Object = std::ptr::null_mut();
            let is_valid: bool = msg_send![vm_config, validateWithError: &mut error];
            
            if !is_valid {
                let error_msg = if !error.is_null() {
                    let desc: *mut Object = msg_send![error, localizedDescription];
                    let desc_str: *const i8 = msg_send![desc, UTF8String];
                    std::ffi::CStr::from_ptr(desc_str).to_string_lossy().to_string()
                } else {
                    "Erreur inconnue".to_string()
                };
                anyhow::bail!("Configuration invalide : {}", error_msg);
            }

            // 6. Instanciation
            let vm: *mut Object = msg_send![class!(VZVirtualMachine), alloc];
            let vm: *mut Object = msg_send![vm, initWithConfiguration: vm_config];

            println!("✓ VM Prête : {}", self.config.name);
            println!("  ISO : {:?}", self.config.iso_path);
            
            self.start_vm(vm)?;

            Ok(())
        }
    }

    unsafe fn create_linux_bootloader(&self) -> Result<*mut Object> {
        let bootloader: *mut Object = msg_send![class!(VZLinuxBootLoader), new];

        // Kernel
        let kernel_path = PathBuf::from(&self.config.kernel_path);
        let kernel_url = self.path_to_nsurl(&kernel_path)?;
        let _: () = msg_send![bootloader, setKernelURL: kernel_url];

        // Initrd
        let initrd_path = PathBuf::from(&self.config.initrd_path);
        let initrd_url = self.path_to_nsurl(&initrd_path)?;
        let _: () = msg_send![bootloader, setInitialRamdiskURL: initrd_url];

        // Ligne de commande Ubuntu
        // console=hvc0 : Affiche le texte sur le terminal
        // boot=casper  : Indique le mode Live CD
        let cmd_line = NSString::from_str("console=hvc0 boot=casper ---");
        let _: () = msg_send![bootloader, setCommandLine: &*cmd_line];

        Ok(bootloader)
    }

    unsafe fn configure_devices(&self, vm_config: *mut Object) -> Result<()> {
        // --- A. Console Interactive (Clavier + Écran) ---
        let serial_config = self.create_interactive_console()?;
        let serial_array: *mut Object = msg_send![class!(NSMutableArray), new];
        let _: () = msg_send![serial_array, addObject: serial_config];
        let _: () = msg_send![vm_config, setSerialPorts: serial_array];

        // --- B. Stockage (Disque + ISO) ---
        let storage_array: *mut Object = msg_send![class!(NSMutableArray), new];

        // 1. Disque Dur (Installation)
        if let Some(disk_path) = &self.config.disk_path {
            let disk = self.create_block_device(disk_path, false)?;
            let _: () = msg_send![storage_array, addObject: disk];
        }

        // 2. ISO Ubuntu (Source)
        if let Some(iso_path) = &self.config.iso_path {
            if std::path::Path::new(iso_path).exists() {
                println!("Attachement de l'ISO : {}", iso_path);
                let iso = self.create_block_device(iso_path, true)?;
                let _: () = msg_send![storage_array, addObject: iso];
            } else {
                println!("⚠️ ISO non trouvé : {}", iso_path);
            }
        }
        let _: () = msg_send![vm_config, setStorageDevices: storage_array];

        // --- C. Réseau (NAT) ---
        let nat: *mut Object = msg_send![class!(VZNATNetworkDeviceAttachment), new];
        let net: *mut Object = msg_send![class!(VZVirtioNetworkDeviceConfiguration), new];
        let _: () = msg_send![net, setAttachment: nat];
        let net_array: *mut Object = msg_send![class!(NSMutableArray), new];
        let _: () = msg_send![net_array, addObject: net];
        let _: () = msg_send![vm_config, setNetworkDevices: net_array];

        // --- D. Entropie ---
        let entropy: *mut Object = msg_send![class!(VZVirtioEntropyDeviceConfiguration), new];
        let entropy_array: *mut Object = msg_send![class!(NSMutableArray), new];
        let _: () = msg_send![entropy_array, addObject: entropy];
        let _: () = msg_send![vm_config, setEntropyDevices: entropy_array];

        Ok(())
    }

    unsafe fn create_interactive_console(&self) -> Result<*mut Object> {
        println!("Configuration de la console interactive (Stdin/Stdout)...");

        // Sortie (Écran)
        let write_handle: *mut Object = msg_send![class!(NSFileHandle), fileHandleWithStandardOutput];
        
        // Entrée (Clavier)
        let read_handle: *mut Object = msg_send![class!(NSFileHandle), fileHandleWithStandardInput];

        if write_handle.is_null() || read_handle.is_null() {
            anyhow::bail!("Impossible d'accéder au terminal.");
        }

        let attachment: *mut Object = msg_send![class!(VZFileHandleSerialPortAttachment), alloc];
        let attachment: *mut Object = msg_send![
            attachment,
            initWithFileHandleForReading: read_handle
            fileHandleForWriting: write_handle
        ];

        let serial_config: *mut Object = msg_send![class!(VZVirtioConsoleDeviceSerialPortConfiguration), new];
        let _: () = msg_send![serial_config, setAttachment: attachment];

        Ok(serial_config)
    }

    unsafe fn create_block_device(&self, path_str: &str, read_only: bool) -> Result<*mut Object> {
        let path = PathBuf::from(path_str);
        
        if !read_only && !path.exists() {
            println!("Création du disque : {}", path_str);
            if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
            let size = 10u64 * 1024 * 1024 * 1024; // 10 GB
            std::fs::File::create(&path)?.set_len(size)?;
        }

        let url = self.path_to_nsurl(&path)?;
        let mut error: *mut Object = std::ptr::null_mut();
        
        let attachment: *mut Object = msg_send![class!(VZDiskImageStorageDeviceAttachment), alloc];
        let attachment: *mut Object = msg_send![
            attachment,
            initWithURL: url
            readOnly: read_only
            error: &mut error
        ];

        if attachment.is_null() {
            anyhow::bail!("Impossible d'attacher : {}", path_str);
        }

        let config: *mut Object = msg_send![class!(VZVirtioBlockDeviceConfiguration), alloc];
        let config: *mut Object = msg_send![config, initWithAttachment: attachment];

        Ok(config)
    }

    unsafe fn start_vm(&self, vm: *mut Object) -> Result<()> {
        println!("\n🚀 Démarrage...");
        
        let block = ConcreteBlock::new(|error: *mut Object| {
            if !error.is_null() {
                let desc: *mut Object = msg_send![error, localizedDescription];
                let desc_str: *const i8 = msg_send![desc, UTF8String];
                let error_msg = std::ffi::CStr::from_ptr(desc_str).to_string_lossy();
                eprintln!("\n❌ ERREUR VM : {}\n", error_msg);
                std::process::exit(1);
            }
        });
        
        let block = block.copy();
        let _: () = msg_send![vm, startWithCompletionHandler: &*block];

        Ok(())
    }

    unsafe fn path_to_nsurl(&self, path: &PathBuf) -> Result<*mut Object> {
        let abs = std::fs::canonicalize(path).context(format!("Chemin introuvable: {:?}", path))?;
        let s = NSString::from_str(abs.to_str().unwrap());
        Ok(msg_send![class!(NSURL), fileURLWithPath: &*s])
    }
}

fn main() -> Result<()> {
    println!("=== VM Manager (Mac Silicon) ===");
    
    // Nettoyage de l'ancien log
    if std::path::Path::new("linux.log").exists() {
        let _ = std::fs::remove_file("linux.log");
    }

    // Flush important
    use std::io::Write;
    let _ = std::io::stdout().flush();

    let config = VMConfig::default();
    let vm = VirtualMachine::new(config);
    
    vm.create_vm()?;

    println!("\n⌨️  Console interactive active.");
    println!("(Appuyez sur Entrée si l'affichage semble bloqué)");
    println!("(Ctrl+C pour quitter)\n");

    unsafe {
        let run_loop: *mut Object = msg_send![class!(NSRunLoop), mainRunLoop];
        let _: () = msg_send![run_loop, run];
    }
    
    Ok(())
}