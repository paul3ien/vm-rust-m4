# VM Rust macOS — Ubuntu Server Headless

Manager de machine virtuelle Ubuntu Server pour macOS Silicon (ARM64), utilisant le framework **Virtualization** d'Apple et ecrit en Rust. Headless uniquement via console serie.

## 🚀 Quick Start

```bash
# 1. Initialiser (telecharge et prepare l'image cloud)
cargo build --release
codesign --entitlements entitlements.plist --force -s - target/release/vm-rust-m4
./target/release/vm-rust-m4 --init

# 2. Lancer
./target/release/vm-rust-m4

# Login: root / Password: root
```

## 📋 Commandes

| Commande | Description |
|---|---|
| `vm-rust-m4` | Lance la VM (mode interactif) |
| `vm-rust-m4 --debug` ou `-v` | Active les logs de debug |
| `vm-rust-m4 --init` ou `-I` | Initialise (telecharge + prepare image cloud) |
| `vm-rust-m4 --init --disk-size 20` | Init avec disque de 20 Go |
| `vm-rust-m4 --daemon` ou `-D` | Mode daemon (arriere-plan) |
| `vm-rust-m4 --config vm_config.json` | Config personnalisee |

## 🏗️ Structure du projet

```
src/
├── main.rs       # CLI, orchestration
├── config.rs     # Configuration JSON
├── vm.rs         # VirtualMachine (Framework Virtualization)
├── init.rs       # Telechargement et initialisation
└── terminal.rs   # Mode raw, signaux
```

## 📦 .app Bundle

```bash
bash build_app.sh
open target/UbuntuVM.app
```

## ⚙️ vm_config.json

```json
{
  "name": "UbuntuEFI",
  "cpu_count": 4,
  "memory_size_gb": 4,
  "disk_path": "data/ubuntu-cloud-raw.img",
  "disk_size_gb": 10,
  "iso_path": null,
  "seed_path": "data/cidata.iso"
}
```

- `iso_path = "data/ubuntu.iso"` → mode installation depuis ISO
- `iso_path = null` + `seed_path` → mode cloud pre-installee

## 🔧 Troubleshooting

- **Terminal fige** : `reset`
- **Aucune image trouvee** : lancer `--init`
- **Boot lent sur M2** : utiliser le mode cloud (defaut), pas l'ISO
