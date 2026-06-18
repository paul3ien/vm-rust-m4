use crate::config::VMConfig;
use crate::terminal;
use anyhow::Result;
use block::ConcreteBlock;
use objc::runtime::Object;
use objc::{class, msg_send, sel, sel_impl};
use objc_foundation::{INSString, NSString};
use std::path::PathBuf;

#[link(name = "Virtualization", kind = "framework")]
extern "C" {}

pub struct VirtualMachine {
    config: VMConfig,
    debug: bool,
}

impl VirtualMachine {
    pub fn new(config: VMConfig, debug: bool) -> Self {
        Self { config, debug }
    }

    pub fn run(&self) -> Result<()> {
        unsafe {
            let supported: bool = msg_send![class!(VZVirtualMachine), isSupported];
            if !supported {
                anyhow::bail!("Virtualisation non supportee sur cette machine.");
            }

            let vmc: *mut Object = msg_send![class!(VZVirtualMachineConfiguration), new];

            // CPU & RAM
            let _: () = msg_send![vmc, setCPUCount: self.config.cpu_count as u64];
            let mem = (self.config.memory_size_gb as u64) * 1024 * 1024 * 1024;
            let _: () = msg_send![vmc, setMemorySize: mem];

            // EFI
            self.configure_efi(vmc)?;
            // Devices
            self.configure_devices(vmc)?;

            // Validate
            let mut err: *mut Object = std::ptr::null_mut();
            let ok: bool = msg_send![vmc, validateWithError: &mut err];
            if !ok {
                let msg = Self::err_string(err);
                anyhow::bail!("Configuration invalide : {}", msg);
            }

            let vm: *mut Object = msg_send![class!(VZVirtualMachine), alloc];
            let _vm: *mut Object = msg_send![vm, initWithConfiguration: vmc];

            log::info!("VM '{}' prete, demarrage...", self.config.name);
            self.start_vm(vm)?;

            Ok(())
        }
    }

    unsafe fn configure_efi(&self, vmc: *mut Object) -> Result<()> {
        let bl: *mut Object = msg_send![class!(VZEFIBootLoader), new];

        let nvram = PathBuf::from("nvram.dat");
        let nvram_url = self.path_to_nsurl(&nvram)?;

        let vs: *mut Object = msg_send![class!(VZEFIVariableStore), alloc];
        let vs: *mut Object = if nvram.exists() {
            msg_send![vs, initWithURL: nvram_url]
        } else {
            let mut err: *mut Object = std::ptr::null_mut();
            let r: *mut Object = msg_send![
                vs,
                initCreatingVariableStoreAtURL: nvram_url
                options: 0u64
                error: &mut err
            ];
            if r.is_null() {
                anyhow::bail!("Erreur creation NVRAM");
            }
            r
        };

        let _: () = msg_send![bl, setVariableStore: vs];
        let _: () = msg_send![vmc, setBootLoader: bl];

        // Platform
        let plat: *mut Object = msg_send![class!(VZGenericPlatformConfiguration), new];
        let mid: *mut Object = msg_send![class!(VZGenericMachineIdentifier), new];
        let _: () = msg_send![plat, setMachineIdentifier: mid];
        let _: () = msg_send![vmc, setPlatform: plat];

        Ok(())
    }

    unsafe fn configure_devices(&self, vmc: *mut Object) -> Result<()> {
        // Serial console
        let sc = self.create_serial_console()?;
        let sa: *mut Object = msg_send![class!(NSMutableArray), new];
        let _: () = msg_send![sa, addObject: sc];
        let _: () = msg_send![vmc, setSerialPorts: sa];

        // Storage
        let st: *mut Object = msg_send![class!(NSMutableArray), new];

        // Disk
        if let Some(dp) = &self.config.disk_path {
            let d = self.create_disk_device(dp)?;
            let _: () = msg_send![st, addObject: d];
        }

        // ISO (install)
        if let Some(ip) = &self.config.iso_path {
            if PathBuf::from(ip).exists() {
                let iso = self.create_readonly_device(ip)?;
                let _: () = msg_send![st, addObject: iso];
                log::info!("ISO montee : {}", ip);
            }
        }

        // Cloud-init seed
        if let Some(sp) = &self.config.seed_path {
            if PathBuf::from(sp).exists() {
                let seed = self.create_readonly_device(sp)?;
                let _: () = msg_send![st, addObject: seed];
                log::info!("Seed cloud-init monte : {}", sp);
            }
        }

        let _: () = msg_send![vmc, setStorageDevices: st];

        // Network (NAT)
        let nat: *mut Object = msg_send![class!(VZNATNetworkDeviceAttachment), new];
        let net: *mut Object = msg_send![class!(VZVirtioNetworkDeviceConfiguration), new];
        let _: () = msg_send![net, setAttachment: nat];
        let na: *mut Object = msg_send![class!(NSMutableArray), new];
        let _: () = msg_send![na, addObject: net];
        let _: () = msg_send![vmc, setNetworkDevices: na];

        // Entropy
        let ent: *mut Object = msg_send![class!(VZVirtioEntropyDeviceConfiguration), new];
        let ea: *mut Object = msg_send![class!(NSMutableArray), new];
        let _: () = msg_send![ea, addObject: ent];
        let _: () = msg_send![vmc, setEntropyDevices: ea];

        Ok(())
    }

    unsafe fn create_serial_console(&self) -> Result<*mut Object> {
        let wh: *mut Object = msg_send![class!(NSFileHandle), fileHandleWithStandardOutput];
        let rh: *mut Object = msg_send![class!(NSFileHandle), fileHandleWithStandardInput];
        if wh.is_null() || rh.is_null() {
            anyhow::bail!("Terminal inaccessible.");
        }

        let att: *mut Object = msg_send![class!(VZFileHandleSerialPortAttachment), alloc];
        let att: *mut Object = msg_send![
            att,
            initWithFileHandleForReading: rh
            fileHandleForWriting: wh
        ];

        let sc: *mut Object = msg_send![class!(VZVirtioConsoleDeviceSerialPortConfiguration), new];
        let _: () = msg_send![sc, setAttachment: att];
        Ok(sc)
    }

    unsafe fn create_disk_device(&self, path_str: &str) -> Result<*mut Object> {
        let path = PathBuf::from(path_str);
        if !path.exists() {
            anyhow::bail!(
                "Disque introuvable : {}. Lance prepare_cloud_image.sh.",
                path_str
            );
        }
        self.create_block_device(path_str, false)
    }

    unsafe fn create_readonly_device(&self, path_str: &str) -> Result<*mut Object> {
        self.create_block_device(path_str, true)
    }

    unsafe fn create_block_device(&self, path_str: &str, readonly: bool) -> Result<*mut Object> {
        let path = PathBuf::from(path_str);
        let url = self.path_to_nsurl(&path)?;
        let mut err: *mut Object = std::ptr::null_mut();

        let att: *mut Object = msg_send![class!(VZDiskImageStorageDeviceAttachment), alloc];
        let att: *mut Object = msg_send![
            att,
            initWithURL: url
            readOnly: readonly
            error: &mut err
        ];

        if att.is_null() {
            let msg = Self::err_string(err);
            anyhow::bail!("Erreur attachement {} : {}", path_str, msg);
        }

        let cfg: *mut Object = msg_send![class!(VZVirtioBlockDeviceConfiguration), alloc];
        let cfg: *mut Object = msg_send![cfg, initWithAttachment: att];
        Ok(cfg)
    }

    unsafe fn start_vm(&self, vm: *mut Object) -> Result<()> {
        let debug = self.debug;
        let block = ConcreteBlock::new(move |err: *mut Object| {
            if !err.is_null() {
                terminal::disable_raw_mode();
                let msg = VirtualMachine::err_string(err);
                eprintln!("\nErreur VM : {}", msg);
                std::process::exit(1);
            }
            if debug {
                log::info!("VM demarree avec succes.");
            }
        });
        let block = block.copy();
        let _: () = msg_send![vm, startWithCompletionHandler: &*block];
        Ok(())
    }

    unsafe fn path_to_nsurl(&self, path: &PathBuf) -> Result<*mut Object> {
        let abs = if path.exists() {
            std::fs::canonicalize(path)?
        } else {
            std::env::current_dir()?.join(path)
        };
        let s = NSString::from_str(abs.to_str().unwrap());
        Ok(msg_send![class!(NSURL), fileURLWithPath: &*s])
    }

    unsafe fn err_string(err: *mut Object) -> String {
        if err.is_null() {
            return "erreur inconnue".into();
        }
        let desc: *mut Object = msg_send![err, localizedDescription];
        let cs: *const i8 = msg_send![desc, UTF8String];
        std::ffi::CStr::from_ptr(cs).to_string_lossy().into()
    }

    /// Boucle d'attente : bloque sur le runloop jusqu'a Ctrl+C.
    pub fn wait_forever() {
        unsafe {
            let rl: *mut Object = msg_send![class!(NSRunLoop), mainRunLoop];
            let _: () = msg_send![rl, run];
        }
    }
}
