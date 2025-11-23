#!/usr/bin/env bash
# ==============================================================================
# manage_encrypted_volume.sh - Mount/unmount genetics encrypted volume
# ==============================================================================
# Description: Helper script to mount/unmount LUKS-encrypted genetic data volume
# Author: Matt Barham
# Created: 2025-10-31
# Modified: 2026-01-17
# Version: 1.0.0
# Repository: https://github.com/captainzonks/GeneGnome
# ==============================================================================

set -euo pipefail
IFS=$'\n\t'

# Configuration (must match setup script)
CONTAINER_PATH="/var/lib/genetics-vault.img"
MAPPER_NAME="genetics-vault"
MOUNT_POINT="/mnt/genetics-encrypted"
KEY_FILE="/root/.genetics-vault.key"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Check if running as root
if [[ $EUID -ne 0 ]]; then
   echo -e "${RED}Error: This script must be run as root${NC}"
   exit 1
fi

# Function to check status
check_status() {
    echo -e "${GREEN}=== Genetics Encrypted Volume Status ===${NC}"

    # Check if container exists
    if [[ ! -f "$CONTAINER_PATH" ]]; then
        echo -e "${RED}✗ Container file not found: ${CONTAINER_PATH}${NC}"
        echo "  Run setup_encrypted_volume.sh first"
        return 1
    fi
    echo -e "${GREEN}✓${NC} Container file exists: $(du -h "$CONTAINER_PATH" | cut -f1)"

    # Check if LUKS device is open
    if [[ -e "/dev/mapper/${MAPPER_NAME}" ]]; then
        echo -e "${GREEN}✓${NC} LUKS device is open: /dev/mapper/${MAPPER_NAME}"
    else
        echo -e "${YELLOW}○${NC} LUKS device is closed"
    fi

    # Check if mounted
    if mountpoint -q "$MOUNT_POINT"; then
        echo -e "${GREEN}✓${NC} Volume is mounted: ${MOUNT_POINT}"
        echo
        df -h "$MOUNT_POINT"
    else
        echo -e "${YELLOW}○${NC} Volume is not mounted"
    fi
}

# Function to mount
do_mount() {
    echo -e "${GREEN}=== Mounting Genetics Encrypted Volume ===${NC}"

    # Check if already mounted
    if mountpoint -q "$MOUNT_POINT"; then
        echo -e "${YELLOW}Volume is already mounted${NC}"
        df -h "$MOUNT_POINT"
        return 0
    fi

    # Open LUKS device if not already open
    if [[ ! -e "/dev/mapper/${MAPPER_NAME}" ]]; then
        echo "Opening LUKS device..."
        if [[ ! -f "$KEY_FILE" ]]; then
            echo -e "${RED}Error: Key file not found: ${KEY_FILE}${NC}"
            exit 1
        fi
        # cryptsetup will automatically create a loop device for the file
        cryptsetup open --key-file "${KEY_FILE}" "$CONTAINER_PATH" "$MAPPER_NAME"
        echo "✓ LUKS device opened"
    else
        echo "✓ LUKS device already open"
    fi

    # Mount
    echo "Mounting filesystem..."
    mkdir -p "$MOUNT_POINT"
    mount "/dev/mapper/${MAPPER_NAME}" "$MOUNT_POINT"
    echo "✓ Volume mounted"

    echo
    df -h "$MOUNT_POINT"
}

# Function to unmount
do_unmount() {
    echo -e "${GREEN}=== Unmounting Genetics Encrypted Volume ===${NC}"

    # Check if mounted
    if mountpoint -q "$MOUNT_POINT"; then
        echo "Unmounting filesystem..."
        umount "$MOUNT_POINT"
        echo "✓ Filesystem unmounted"
    else
        echo "✓ Filesystem already unmounted"
    fi

    # Close LUKS device (this also removes the associated loop device)
    if [[ -e "/dev/mapper/${MAPPER_NAME}" ]]; then
        echo "Closing LUKS device..."
        cryptsetup close "$MAPPER_NAME"
        echo "✓ LUKS device closed (loop device automatically removed)"
    else
        echo "✓ LUKS device already closed"
    fi

    echo -e "${GREEN}Volume unmounted and encrypted${NC}"
}

# Function to show usage
usage() {
    cat << EOF
Usage: $0 {mount|unmount|status}

Commands:
  mount    - Open LUKS device and mount filesystem
  unmount  - Unmount filesystem and close LUKS device
  status   - Show current status

Examples:
  sudo $0 status
  sudo $0 mount
  sudo $0 unmount

Configuration:
  Container: ${CONTAINER_PATH}
  Device: /dev/mapper/${MAPPER_NAME}
  Mount: ${MOUNT_POINT}
  Key: ${KEY_FILE}
EOF
}

# Main
case "${1:-}" in
    mount)
        do_mount
        ;;
    unmount)
        do_unmount
        ;;
    status)
        check_status
        ;;
    *)
        usage
        exit 1
        ;;
esac
