# LUKS Encrypted Volume Troubleshooting

## Issue: device-mapper reload ioctl failed

### Error Message
```
device-mapper: reload ioctl on genetics-vault (253:0) failed: Invalid argument
```

### Root Cause
This error can occur due to several issues:
1. **Kernel update without reboot** ⚠️ **MOST COMMON** - Running old kernel with updated cryptsetup
2. **Sector size mismatch** - Default 512-byte sectors don't match modern drives
3. **Aggressive argon2id parameters** - Too much memory/parallelism
4. **Device-mapper kernel issues** - Compatibility problems

### Solutions Applied (v1.0.2)

#### 1. Explicit Loop Device Setup
- Now explicitly creates loop device before LUKS operations
- Ensures proper block device structure for device-mapper

#### 2. Reduced argon2id Parameters
**Before:**
- `--pbkdf-memory 1048576` (1GB)
- `--pbkdf-parallel 4`

**After:**
- `--pbkdf-memory 524288` (512MB)
- `--pbkdf-parallel 2`
- `--iter-time 2000` (2 seconds)

**Note:** Still provides strong security, just less resource-intensive

#### 3. Explicit Sector Size
Added `--sector-size 4096` to match modern drive geometry

**Why:** Modern SSDs/HDDs use 4096-byte physical sectors. Using 512-byte logical sectors can cause device-mapper issues.

#### 4. Enhanced Diagnostics
Added verbose output and key validation testing on failure

## Critical First Step: Check Kernel Version

**IMPORTANT:** If you recently updated your kernel, you MUST reboot before running the setup script.

```bash
# Check running kernel vs installed kernel
uname -r                    # Currently running kernel
pacman -Q linux             # Installed kernel package

# If versions don't match, reboot first!
sudo reboot
```

**Why:** Device-mapper is a kernel subsystem. Running an old kernel with new cryptsetup tools causes API mismatches that manifest as "device already exists or device is busy" errors.

## Testing the Fix

```bash
cd scripts

# Clean up any previous attempts
sudo ./cleanup_encrypted_volume.sh

# Run setup with fixes
sudo ./setup_encrypted_volume.sh
```

## If Still Failing

### Check Kernel Modules
```bash
lsmod | grep -E "dm_|crypt"
sudo modprobe dm_crypt
```

### Check for Conflicting Devices
```bash
ls -la /dev/mapper/
sudo dmsetup ls
losetup -a
```

### Try Manual Test
```bash
# Create test container
sudo fallocate -l 10M /tmp/test-luks.img

# Format with minimal parameters
sudo cryptsetup luksFormat \
  --type luks2 \
  --sector-size 4096 \
  /tmp/test-luks.img

# Try to open
sudo cryptsetup open /tmp/test-luks.img test-vault

# Clean up
sudo cryptsetup close test-vault
sudo rm /tmp/test-luks.img
```

If the manual test works, the issue is with the script parameters. If it fails, there's a deeper system issue with device-mapper or kernel modules.

## Security Trade-offs

### Sector Size 4096 vs 512
- **4096**: Better performance, modern drive compatibility, **slightly larger metadata overhead**
- **512**: Traditional, more compatible, but may cause device-mapper issues

The security impact is minimal - encryption strength comes from AES-256-XTS and key derivation, not sector size.

### Argon2id Memory Reduction
- **1GB**: Maximum resistance to GPU/ASIC attacks, may cause system issues
- **512MB**: Still excellent security, more system-friendly

**Context:** For genetic data processor with automated key files (not user passwords), 512MB argon2id is still cryptographically strong. The key file itself (/root/.genetics-vault.key) is 4KB of random data, providing 32768 bits of entropy.

## References

- [cryptsetup FAQ - sector size](https://gitlab.com/cryptsetup/cryptsetup/-/wikis/FrequentlyAskedQuestions#5-security-aspects)
- [LUKS2 specification](https://gitlab.com/cryptsetup/LUKS2-docs)
- [argon2id parameters](https://tools.ietf.org/html/rfc9106)

---

**Last Updated:** 2025-11-01
**Script Version:** 1.0.2
**Resolution:** Kernel reboot required after system update
