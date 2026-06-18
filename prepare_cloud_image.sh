#!/bin/bash
# prepare_cloud_image.sh
# Prepare l'image cloud Ubuntu et le seed cloud-init.
# A lancer UNE SEULE FOIS (ou quand tu veux recreer l'image).
set -e

echo "=== Preparation de l'image cloud Ubuntu ==="

INPUT_QCOW2="data/ubuntu-24.04-minimal-cloudimg-arm64.img"
OUTPUT_RAW="data/ubuntu-cloud-raw.img"
SEED_ISO="data/cidata.iso"
ROOT_PASSWORD="root"

# --- Etape 1: Convertir QCOW2 -> RAW ---
echo "[1/3] Conversion QCOW2 -> RAW..."
qemu-img convert -f qcow2 -O raw "$INPUT_QCOW2" "$OUTPUT_RAW"
echo "    Fait: $(ls -lh $OUTPUT_RAW | awk '{print $5}')"

# --- Etape 2: Redimensionner a 10 Go ---
echo "[2/3] Redimensionnement a 10 Go..."
qemu-img resize -f raw "$OUTPUT_RAW" 10G
echo "    Fait: $(ls -lh $OUTPUT_RAW | awk '{print $5}')"

# --- Etape 3: Creer l'ISO cloud-init seed ---
echo "[3/3] Creation du seed cloud-init (cidata.iso)..."

# Generer le hash du mot de passe
HASH=$(python3 -c "
import subprocess
result = subprocess.run(['openssl', 'passwd', '-6', '-salt', 'aBcDeFgH', '$ROOT_PASSWORD'], capture_output=True, text=True)
print(result.stdout.strip())
")
echo "    Hash genere: $HASH"

# Creer le dossier seed temporaire
rm -rf /tmp/vm_seed
mkdir -p /tmp/vm_seed

cat > /tmp/vm_seed/user-data << USERDATA
#cloud-config
hostname: ubuntu-hypervisor-vm
manage_etc_hosts: true
users:
  - name: root
    lock-passwd: false
    hashed_passwd: '$HASH'
    shell: /bin/bash
ssh_pwauth: true
chpasswd:
  expire: false
runcmd:
  - 'growpart /dev/vda 1 || true'
  - 'resize2fs /dev/vda1 || true'
  - 'echo "root:$ROOT_PASSWORD" | chpasswd'
USERDATA

cat > /tmp/vm_seed/meta-data << METADATA
instance-id: ubuntu-vm-001
local-hostname: ubuntu-hypervisor-vm
METADATA

# Creer l'ISO
rm -f "$SEED_ISO"
xorriso -as genisoimage \
  -output "$SEED_ISO" \
  -volid CIDATA \
  -joliet \
  -rock \
  /tmp/vm_seed/user-data \
  /tmp/vm_seed/meta-data \
  2>/dev/null

rm -rf /tmp/vm_seed
echo "    Seed ISO cree: $SEED_ISO"

echo ""
echo "=== Termine ! ==="
echo ""
echo "Tu peux maintenant lancer la VM :"
echo "  rm -f nvram.dat"
echo "  cargo build"
echo "  codesign --entitlements entitlements.plist --force -s - target/debug/vm-rust-m4"
echo "  ./target/debug/vm-rust-m4"
echo ""
echo "Login: root"
echo "Password: $ROOT_PASSWORD"
