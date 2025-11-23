#!/usr/bin/env bash
# ==============================================================================
# cleanup_encrypted_volume.sh - Remove failed LUKS encrypted volume
# ==============================================================================
# Description: Cleans up partially created LUKS container from failed setup
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

echo -e "${YELLOW}=== Genetics Encrypted Volume Cleanup ===${NC}"
echo "This will remove any existing container and associated resources."
echo

# Check current state
echo "Current state:"
if [[ -f "$CONTAINER_PATH" ]]; then
    echo "  ✓ Container file exists: $(du -h "$CONTAINER_PATH" | cut -f1)"
else
    echo "  ○ Container file not found"
fi

if [[ -e "/dev/mapper/${MAPPER_NAME}" ]]; then
    echo "  ✓ LUKS device is open: /dev/mapper/${MAPPER_NAME}"
else
    echo "  ○ LUKS device is not open"
fi

if mountpoint -q "$MOUNT_POINT" 2>/dev/null; then
    echo "  ✓ Volume is mounted: ${MOUNT_POINT}"
else
    echo "  ○ Volume is not mounted"
fi

echo
read -p "Do you want to proceed with cleanup? (yes/no): " response
if [[ "$response" != "yes" ]]; then
    echo "Aborted."
    exit 0
fi

echo
echo -e "${GREEN}Starting cleanup...${NC}"

# Step 1: Unmount if mounted
if mountpoint -q "$MOUNT_POINT" 2>/dev/null; then
    echo "Unmounting ${MOUNT_POINT}..."
    umount "$MOUNT_POINT" && echo "✓ Unmounted" || echo "⚠ Failed to unmount"
fi

# Step 2: Close LUKS device if open
if [[ -e "/dev/mapper/${MAPPER_NAME}" ]]; then
    echo "Closing LUKS device ${MAPPER_NAME}..."
    cryptsetup close "$MAPPER_NAME" && echo "✓ Closed" || echo "⚠ Failed to close"
fi

# Step 3: Detach loop device if attached
if [[ -f "$CONTAINER_PATH" ]]; then
    LOOP_DEV=$(losetup -j "$CONTAINER_PATH" 2>/dev/null | cut -d: -f1)
    if [[ -n "$LOOP_DEV" ]]; then
        echo "Detaching loop device ${LOOP_DEV}..."
        losetup -d "$LOOP_DEV" && echo "✓ Detached" || echo "⚠ Failed to detach"
    fi
fi

# Step 4: Remove container file
if [[ -f "$CONTAINER_PATH" ]]; then
    echo "Removing container file..."
    rm -f "$CONTAINER_PATH" && echo "✓ Container file removed" || echo "⚠ Failed to remove"
fi

# Step 5: Clean up /etc/crypttab
if grep -q "$MAPPER_NAME" /etc/crypttab 2>/dev/null; then
    echo "Removing /etc/crypttab entry..."
    sed -i "/$MAPPER_NAME/d" /etc/crypttab && echo "✓ Removed from crypttab" || echo "⚠ Failed to update crypttab"
fi

# Step 6: Clean up /etc/fstab
if grep -q "$MOUNT_POINT" /etc/fstab 2>/dev/null; then
    echo "Removing /etc/fstab entry..."
    sed -i "\|$MOUNT_POINT|d" /etc/fstab && echo "✓ Removed from fstab" || echo "⚠ Failed to update fstab"
fi

echo
echo -e "${GREEN}Cleanup complete!${NC}"
echo "You can now run setup_encrypted_volume.sh again."
