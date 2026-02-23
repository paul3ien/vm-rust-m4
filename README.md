# VM Rust macOS — Ubuntu Server Headless

Manager de machine virtuelle Ubuntu Server pour macOS Silicon (ARM64), utilisant le framework **Virtualization** d'Apple et écrit en Rust. Conçu pour un environnement **headless** (sans interface graphique), adressant ubuntu via une console série.

## 🎯 Objectifs

- **Headless only** : pas de GUI, console série uniquement
- **Ubuntu Server** : installation minimale et efficace
- **ARM64 natif** : optimisé pour Apple Silicon (M1/M2/M3+)
- **Mode raw terminal** : touches spéciales et flèches fléchées directement accessibles dans la VM
- **Configuration simple** : JSON pour personnaliser CPU, RAM, disque

---

## 📋 Prérequis

### Système
- **macOS 14+** (Sonoma ou plus récent) sur Apple Silicon
- Accès administrateur (pour `codesign` et les permissions de virtualisation)
- **Xcode Command Line Tools** (pour Rust et Cargo)

### Fichiers nécessaires
- `data/disk.img` : image disque (créée automatiquement au premier lancement)
- `data/ubuntu.iso` : ISO Ubuntu Server ARM64 (télécharger depuis [ubuntu.com/download/server](https://ubuntu.com/download/server), choisir "Other architectures" → ARM64)

### Dépendances Rust
- Rust 1.70+ (gérées automatiquement via `cargo`)
- Dépendances principales : `objc`, `serde_json`, `anyhow`, `block`, `libc`

---

## 🚀 Installation et Build

### 1. Cloner/préparer le projet
```bash
cd /Users/apple/Documents/Code/Rust/vm-rust-m4
```

### 2. Télécharger Ubuntu Server ISO (ARM64)
```bash
mkdir -p data
# Télécharger depuis : https://ubuntu.com/download/server
# Choisir : "Other architectures" → ARM64 (generic QEMU image)
# Sauvegarder dans : data/ubuntu.iso
```

### 3. Compiler
```bash
cargo build --release
```

### 4. Signer avec droits de virtualisation
```bash
codesign --entitlements entitlements.plist --force -s - target/release/vm-rust-m4
```

### 5. Lancer
```bash
./target/release/vm-rust-m4
```

---

## 💻 Utilisation

### Démarrage
```bash
# Nettoyage + rebuild + signature + lancement
rm -f data/disk.img nvram.dat
cargo build --release
codesign --entitlements entitlements.plist --force -s - target/release/vm-rust-m4
./target/release/vm-rust-m4
```

Or use the provided script:
```bash
bash cmd.txt  # (après édition si nécessaire)
```

### Premier lancement - Installation Ubuntu

1. **GRUB Boot Menu** s'affiche
   - Flèches ↓ pour "Try or Install Ubuntu"
   - Entrée pour confirmer

2. **Ubuntu Installer** démarre
   - Mode text/ncurses
   - Touches Tab + Entrée pour naviguer
   - Suivre guide standard d'installation

3. **Après installation**
   - Reboot automatique
   - Console login via terminal série (user créé during install)

### Console interactive

- **Touches spéciales fonctionne** : flèches, Tab, Escape, etc.
- **Ctrl+C** : arrête la VM et restaure le terminal local
- **Raw mode** : activé automatiquement (voir [Terminal Raw Mode](#-terminal-raw-mode))

---

## ⚙️ Configuration

### `vm_config.json`

Fichier de configuration (optionnel, defaults utilisés sinon) :

```json
{
  "name": "UbuntuEFI",
  "cpu_count": 4,
  "memory_size_gb": 4,
  "disk_path": "data/disk.img",
  "disk_size_gb": 10,
  "iso_path": "data/ubuntu.iso"
}
```

**Champs** :
- `name` : identifiant/label de la VM
- `cpu_count` : nombre de cores (recommandé : 2-8)
- `memory_size_gb` : RAM allouée (recommandé : 2-8 Go)
- `disk_path` : chemin vers l'image disque (créée si n'existe pas)
- `disk_size_gb` : taille disque (minimum 8 Go pour Ubuntu Server)
- `iso_path` : chemin vers ubuntu.iso (optionnel, démarrage sur disque sinon)

---

## 🏗️ Architecture

### Composants clés

#### **Framework Virtualization** (Apple)
Accès bas-niveau aux APIs de virtualisation ARM64 :
- `VZVirtualMachine` : instance VM
- `VZEFIBootLoader` + `VZEFIVariableStore` : firmware EFI + NVRAM
- `VZVirtioConsoleDeviceSerialPortConfiguration` : console série (stdout/stdin)
- `VZVirtioNetworkDeviceConfiguration` : réseau NAT
- `VZUSBMassStorageDeviceConfiguration` : ISO en tant qu'USB (compatible installeur)

#### **Console Série** (Raw Terminal Mode)
```
┌─────────────────────┐
│  Linux VM (guest)   │
│  console=hvc0       │ ← Kernel redirects output here
└──────┬──────────────┘
       │ VirtIO Serial Port
       │ (hvc0 host side)
       │
┌──────▼──────────────┐
│  macOS Term (host)  │
│  Raw Mode Enabled   │ ← Flèches/Escape passés directement
└─────────────────────┘
```

### Terminal Raw Mode

Le mode **raw** désactive :
- Echo automatique sur le terminal hôte
- Traitement de ^C au niveau hôte
- Interprétation des séquences d'échappement par le shell

Ceci permet aux touches spéciales (flèches, tab, escape) d'arriver intactes à la VM via la console série.

**Nettoyage automatique** : signal handlers + `atexit()` restaurent les paramètres terminaux si crash/interruption.

---

## 🔧 Troubleshooting

### ❌ "fatal runtime error: Rust cannot catch foreign exceptions"

**Cause** : Appel Objective-C invalide (mauvais type, sélecteur non supporté).

**Solutions** :
1. Vérifier la version macOS (`sw_vers -productVersion`)
2. Vérifier que l'ISO est bien ARM64 (non x86)
3. Supprimer `nvram.dat` et relancer : `rm -f nvram.dat`

### ❌ Terminal figé après Ctrl+C

Le programme a crash avant de restaurer les termios. Fix :
```bash
reset
```

### ❌ Ubuntu Installer ne s'affiche pas

Ajouter `console=hvc0` au kernel bootline (GRUB) :
1. Au menu GRUB, appuyer `e`
2. Trouver la ligne `linux ...`
3. Aller à la fin, ajouter ` console=hvc0`
4. Ctrl+X pour booter

### ❌ VM très lente lors de l'installation

Normal pendant le partitioning/formattage. L'ISO en USB Mass Storage a un petit overhead. Attendre 2-3 min.

### ❌ I/O errors ou corruption disque

Vérifier espace disque libre sur le Mac : `df -h`

---

## 📁 Structure du projet

```
.
├── Cargo.toml              # Dépendances Rust
├── Cargo.lock              # Lock file
├── entitlements.plist      # Macros signataire (iOS-style sandboxing)
├── vm_config.json          # Configuration (optionnel)
├── cmd.txt                 # Script launcher
├── src/
│   └── main.rs             # Code source principal (~420 lignes)
├── data/
│   ├── disk.img            # Image disque Ubuntu (créée auto, ~10 Go)
│   ├── ubuntu.iso          # ISO Ubuntu Server ARM64 (manuel, ~2.8 Go)
│   └── old/                # Archives kernel/initrd (non utilisé)
└── target/
    └── debug/ ou release/  # Build artifacts
```

---

## 🚦 État et limitations

### ✅ Fonctionnalités
- [x] Boot EFI avec NVRAM persistant
- [x] Console série (VirtIO, hvc0)
- [x] Réseau NAT (outbound OK, inbound via host forwarding possible)
- [x] Installation Ubuntu complète
- [x] Terminal raw mode (flèches, touches spéciales)
- [x] Configuration JSON
- [x] Entropic device (accelerated RNG)

### ❌ Non supporté (actuellement)
- [ ] Interface graphique
- [ ] SSH direct (nécessite configuration réseau avancée)
- [ ] USB pass-through (pas de périphériques)
- [ ] GPU support (pas de card graphique)
- [ ] Snapshots/save-restore

### 🔮 Possible futur
- VirtIO Socket (host ↔ guest communication)
- Disque persistant multi-VM
- Web UI pour gestion
- Scripts d'automatisation post-install

---

## 📚 Ressources

- [Apple Virtualization Framework Documentation](https://developer.apple.com/documentation/virtualization)
- [Ubuntu Server Download (ARM64)](https://ubuntu.com/download/server)
- [Linux Console (hvc0)](https://www.kernel.org/doc/html/latest/admin-guide/serial-console.html)
- [Rust objc crate](https://docs.rs/objc/)

---

## 📝 Notes développeur

### Objc FFI
Les appels à Virtualization framework passent par `objc` crate avec macriques `msg_send![]` et `class![]`. Attention aux sélecteurs : les types doivent correspondre exactement (bool, u64, pointers, etc.).

### Memory safety
UNSAFE utilisé abondamment pour les calls Objective-C. Zéro overhead runtime, mais demande prudence en maintien.

### Console série
`NSFileHandle::fileHandleWithStandardInput/Output` crée un attachment direct aux stdio du processus hôte. Idéal pour terminal headless.

---

## 📄 License

MIT / Apache 2.0 (à définir selon besoins)

---

## 👤 Auteur & Support

Projet développé pour exploration de virtualisation macOS en Rust.  
Problèmes ? Vérifier:
1. ISO arquiteture (ARM64, non x86)
2. Espace disque hôte (min 15 Go libre)
3. macOS 14+ (Sonoma+)
4. Entitlements signés correctement

**Bon virtualisage !** 🚀
