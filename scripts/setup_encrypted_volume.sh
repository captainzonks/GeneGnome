#!/usr/bin/env bash
# ==============================================================================
# setup_encrypted_volume.sh - Create LUKS-encrypted volume for genetic data
# ==============================================================================
# Description: Creates file-based LUKS container for genetic data storage
#              Uses AES-256-XTS encryption with argon2id key derivation
# Author: Matt Barham
# Created: 2025-10-31
# Modified: 2026-01-17
# Version: 1.0.3
# Repository: https://github.com/captainzonks/GeneGnome
# ==============================================================================
# Security: CRITICAL - Creates encrypted storage for genetic data
# Requirements: root/sudo access, cryptsetup installed
# ==============================================================================

set -euo pipefail
IFS=$'\n\t'

# Cleanup function for errors
cleanup_on_error() {
    local exit_code=$?
    if [[ $exit_code -ne 0 ]]; then
        echo -e "\n${RED}Error occurred during setup. Cleaning up...${NC}"

        # Unmount if mounted
        if mountpoint -q "$MOUNT_POINT" 2>/dev/null; then
            echo "Unmounting ${MOUNT_POINT}..."
            umount "$MOUNT_POINT" 2>/dev/null || true
        fi

        # Close LUKS device if open
        if [[ -e "/dev/mapper/${MAPPER_NAME}" ]]; then
            echo "Closing LUKS device..."
            cryptsetup close "$MAPPER_NAME" 2>/dev/null || true
        fi

        # Detach loop device if attached
        local attached_loop=$(losetup -j "$CONTAINER_PATH" 2>/dev/null | cut -d: -f1)
        if [[ -n "$attached_loop" ]]; then
            echo "Detaching loop device ${attached_loop}..."
            losetup -d "$attached_loop" 2>/dev/null || true
        fi

        echo -e "${RED}Setup failed. Container file preserved at ${CONTAINER_PATH}${NC}"
        echo "You can try running the script again to recreate the container."
    fi
}

trap cleanup_on_error EXIT

# Configuration
CONTAINER_SIZE_GB=100
CONTAINER_PATH="/var/lib/genetics-vault.img"
MAPPER_NAME="genetics-vault"
MOUNT_POINT="/mnt/genetics-encrypted"
KEY_FILE="/root/.genetics-vault.key"
OWNER_UID=3000  # genetics processor user
OWNER_GID=3000  # genetics processor group

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Check if running as root
if [[ $EUID -ne 0 ]]; then
   echo -e "${RED}Error: This script must be run as root${NC}"
   exit 1
fi

# Check for kernel version mismatch
RUNNING_KERNEL=$(uname -r)
INSTALLED_KERNEL=$(pacman -Q linux 2>/dev/null | awk '{print $2}' | sed 's/-[0-9]*$//')
if [[ -n "$INSTALLED_KERNEL" && ! "$RUNNING_KERNEL" =~ ^${INSTALLED_KERNEL} ]]; then
    echo -e "${YELLOW}WARNING: Kernel version mismatch detected!${NC}"
    echo "  Running kernel:   ${RUNNING_KERNEL}"
    echo "  Installed kernel: ${INSTALLED_KERNEL}"
    echo ""
    echo -e "${YELLOW}You may have updated the kernel without rebooting.${NC}"
    echo "This can cause device-mapper errors during LUKS setup."
    echo ""
    read -p "Do you want to continue anyway? (yes/no): " response
    if [[ "$response" != "yes" ]]; then
        echo "Aborted. Please reboot and try again."
        exit 0
    fi
    echo ""
fi

echo -e "${GREEN}=== Genetics Data Encrypted Volume Setup ===${NC}"
echo "Container size: ${CONTAINER_SIZE_GB} GB"
echo "Container path: ${CONTAINER_PATH}"
echo "Mount point: ${MOUNT_POINT}"
echo

# Check if container already exists
if [[ -f "$CONTAINER_PATH" ]]; then
    echo -e "${YELLOW}Warning: Container file already exists at ${CONTAINER_PATH}${NC}"
    read -p "Do you want to remove it and create a new one? (yes/no): " response
    if [[ "$response" != "yes" ]]; then
        echo "Aborted."
        exit 0
    fi

    # Unmount and close if currently open
    if mountpoint -q "$MOUNT_POINT"; then
        echo "Unmounting ${MOUNT_POINT}..."
        umount "$MOUNT_POINT"
    fi

    if [[ -e "/dev/mapper/${MAPPER_NAME}" ]]; then
        echo "Closing LUKS device..."
        cryptsetup close "$MAPPER_NAME"
    fi

    echo "Removing old container..."
    rm -f "$CONTAINER_PATH"
fi

# Step 1: Create container file (use fallocate for proper space allocation)
echo -e "${GREEN}Step 1/8: Creating ${CONTAINER_SIZE_GB}GB container file...${NC}"
echo "Allocating ${CONTAINER_SIZE_GB}GB of disk space (this may take a moment)..."
fallocate -l "${CONTAINER_SIZE_GB}G" "$CONTAINER_PATH"
chmod 600 "$CONTAINER_PATH"
echo "✓ Container file created and space allocated"

# Step 2: Generate random key
echo -e "${GREEN}Step 2/8: Generating encryption key...${NC}"
if [[ -f "$KEY_FILE" ]]; then
    echo -e "${YELLOW}Warning: Key file already exists${NC}"
    read -p "Use existing key? (yes/no): " response
    if [[ "$response" != "yes" ]]; then
        dd if=/dev/urandom of="${KEY_FILE}" bs=4096 count=1 status=none
        echo "✓ New key generated"
    else
        echo "✓ Using existing key"
    fi
else
    dd if=/dev/urandom of="${KEY_FILE}" bs=4096 count=1 status=none
    chmod 600 "${KEY_FILE}"
    echo "✓ Key generated and secured"
fi

# Step 3: Set up loop device
echo -e "${GREEN}Step 3/8: Setting up loop device...${NC}"
LOOP_DEVICE=$(losetup -f)
echo "Available loop device: ${LOOP_DEVICE}"
losetup "$LOOP_DEVICE" "$CONTAINER_PATH"
echo "✓ Loop device ${LOOP_DEVICE} attached to container"

# Step 4: Initialize LUKS container
echo -e "${GREEN}Step 4/8: Initializing LUKS encryption (this may take a minute)...${NC}"
echo "Using: AES-256-XTS with argon2id key derivation"

cryptsetup luksFormat \
    --type luks2 \
    --cipher aes-xts-plain64 \
    --key-size 512 \
    --hash sha512 \
    --pbkdf argon2id \
    --pbkdf-memory 524288 \
    --pbkdf-parallel 2 \
    --iter-time 2000 \
    --sector-size 4096 \
    --use-random \
    --key-file "${KEY_FILE}" \
    "$LOOP_DEVICE"

echo "✓ LUKS container initialized on ${LOOP_DEVICE}"

# Step 5: Open LUKS container
echo -e "${GREEN}Step 5/8: Opening encrypted container...${NC}"
echo "Running: cryptsetup open ${LOOP_DEVICE} ${MAPPER_NAME}"

# Add verbose output for debugging
if cryptsetup -v open --key-file "${KEY_FILE}" "$LOOP_DEVICE" "$MAPPER_NAME" 2>&1; then
    echo "✓ Container opened at /dev/mapper/${MAPPER_NAME}"
else
    echo -e "${RED}Failed to open LUKS container${NC}"
    echo ""
    echo "Diagnostic information:"
    echo "Loop device status:"
    losetup -l "$LOOP_DEVICE"
    echo ""
    echo "LUKS header dump:"
    cryptsetup luksDump "$LOOP_DEVICE" 2>&1 | head -20
    echo ""
    echo "Attempting to test key:"
    cryptsetup --test-passphrase --key-file "${KEY_FILE}" open "$LOOP_DEVICE" && echo "✓ Key is valid" || echo "✗ Key validation failed"
    losetup -d "$LOOP_DEVICE"
    exit 1
fi

# Step 6: Create filesystem
echo -e "${GREEN}Step 6/8: Creating ext4 filesystem...${NC}"
mkfs.ext4 -L genetics-vault "/dev/mapper/${MAPPER_NAME}"
echo "✓ Filesystem created"

# Step 7: Create mount point and mount
echo -e "${GREEN}Step 7/8: Creating mount point and mounting...${NC}"
mkdir -p "$MOUNT_POINT"
mount -o noatime,nodiratime,noexec,nosuid,nodev "/dev/mapper/${MAPPER_NAME}" "$MOUNT_POINT"
echo "✓ Mounted at ${MOUNT_POINT}"

# Step 8: Set ownership and permissions
echo -e "${GREEN}Step 8/8: Setting ownership and permissions...${NC}"

# Check if user/group exist
if ! id -u $OWNER_UID >/dev/null 2>&1; then
    echo -e "${YELLOW}Warning: UID $OWNER_UID does not exist yet${NC}"
    echo "Setting ownership to root:root for now"
    echo "Run 'chown -R ${OWNER_UID}:${OWNER_GID} ${MOUNT_POINT}' after creating user"
    chown root:root "$MOUNT_POINT"
else
    chown ${OWNER_UID}:${OWNER_GID} "$MOUNT_POINT"
    echo "✓ Ownership set to ${OWNER_UID}:${OWNER_GID}"
fi

chmod 700 "$MOUNT_POINT"
echo "✓ Permissions set to 700"

# Create directory structure
echo "Creating directory structure..."
mkdir -p "${MOUNT_POINT}/uploads"
mkdir -p "${MOUNT_POINT}/processing"
mkdir -p "${MOUNT_POINT}/results"

if id -u $OWNER_UID >/dev/null 2>&1; then
    chown -R ${OWNER_UID}:${OWNER_GID} "${MOUNT_POINT}"
fi

echo "✓ Directory structure created"

# Display configuration info
echo
echo -e "${GREEN}=== Setup Complete ===${NC}"
echo "Encrypted volume information:"
echo "  Container file: ${CONTAINER_PATH}"
echo "  LUKS device: /dev/mapper/${MAPPER_NAME}"
echo "  Mount point: ${MOUNT_POINT}"
echo "  Filesystem: ext4"
echo "  Encryption: AES-256-XTS (LUKS2)"
echo "  Key file: ${KEY_FILE}"
echo
echo -e "${YELLOW}IMPORTANT:${NC}"
echo "1. Keep ${KEY_FILE} secure - it's needed to unlock the volume"
echo "2. Backup ${KEY_FILE} to a secure location"
echo "3. Volume will NOT auto-mount on boot (for security)"
echo

# Create /etc/crypttab and /etc/fstab entries
echo "Creating /etc/crypttab entry..."
cat >> /etc/crypttab << EOF
# Genetics data encrypted volume
${MAPPER_NAME} ${CONTAINER_PATH} ${KEY_FILE} luks
EOF
echo "✓ Added to /etc/crypttab"

echo "Creating /etc/fstab entry..."
MAPPER_UUID=$(blkid -s UUID -o value "/dev/mapper/${MAPPER_NAME}")
cat >> /etc/fstab << EOF
# Genetics data encrypted volume
UUID=${MAPPER_UUID} ${MOUNT_POINT} ext4 noatime,nodiratime,noexec,nosuid,nodev 0 2
EOF
echo "✓ Added to /etc/fstab"

echo
echo -e "${GREEN}Setup completed successfully!${NC}"
echo
echo "To manually mount/unmount later:"
echo "  Mount:   cryptsetup open --key-file ${KEY_FILE} ${CONTAINER_PATH} ${MAPPER_NAME} && mount ${MOUNT_POINT}"
echo "  Unmount: umount ${MOUNT_POINT} && cryptsetup close ${MAPPER_NAME}"
echo
echo "Current mount status:"
df -h "$MOUNT_POINT"

# Disable error trap on successful completion
trap - EXIT
