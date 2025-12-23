# Ubuntu PPA Setup Guide

This guide explains how to set up the Debian packaging and Ubuntu PPA upload for all-smi.

## Prerequisites

1. **Launchpad Account**: Create an account at https://launchpad.net
2. **PPA Created**: Create a PPA at https://launchpad.net/~YOUR_USERNAME/+archive/ubuntu/+new
3. **GPG Key**: Generate and upload a GPG key to Launchpad

## Setting up GitHub Secrets

The workflow requires the following secrets to be configured in the GitHub repository under Settings → Secrets and variables → Actions → Repository secrets:

### Required Secrets

1. **GPG_PRIVATE_KEY**
   ```bash
   # Export your GPG private key
   gpg --armor --export-secret-keys YOUR_KEY_ID > private.key
   
   # Copy the contents of private.key to this secret
   ```

2. **GPG_KEY_ID**
   ```bash
   # Find your key ID
   gpg --list-secret-keys --keyid-format=long
   
   # Use the ID after the key type (e.g., "3AA5C34371567BD2")
   # For ed25519 keys: look for the ID after "sec ed25519/"
   # For rsa4096 keys: look for the ID after "sec rsa4096/"
   ```

3. **GPG_PASSPHRASE** (Optional)
   ```bash
   # If your GPG key has a passphrase, add it as a secret
   # This is the passphrase you use to unlock your GPG key
   ```

## Workflow Usage

### Automatic Trigger
The Debian package workflow automatically runs after a successful release build:
1. Create a new release on GitHub
2. The Release workflow builds binaries
3. The Debian package workflow triggers automatically
4. Packages are built and uploaded to PPA

### Manual Trigger
You can also manually trigger the workflow:
1. Go to Actions → "Build and Upload Debian Package"
2. Click "Run workflow"
3. Enter the release tag (e.g., "v0.6.3")
4. Choose whether to upload to PPA

## PPA Configuration

The workflow is configured to upload to: `ppa:lablup/backend-ai`

To change this:
1. Edit `.github/workflows/debian_package.yml`
2. Update the `incoming` field in the dput configuration:
   ```
   incoming = ~YOUR_LAUNCHPAD_USERNAME/ubuntu/YOUR_PPA_NAME/
   ```

## Installing from PPA

Once packages are uploaded and built by Launchpad:

```bash
# Add the PPA
sudo add-apt-repository ppa:lablup/backend-ai
sudo apt update

# Install all-smi
sudo apt install all-smi
```

## Updating Changelog

To update the changelog from GitHub releases:
```bash
cd debian/
./update-changelog.sh
```

This will fetch all releases from GitHub and format them for the Debian changelog.

## Package Structure

- **Binary Package**: Downloads pre-built binaries from GitHub releases
- **No Compilation**: Uses existing release artifacts to save build time
- **Multi-Architecture**: Supports amd64 and arm64
- **Multi-Distribution**: Builds for Ubuntu 22.04, 24.04, and 24.10

## Rust Toolchain and Cargo.lock Compatibility

### Why rustup is Required

The PPA build process uses **rustup** to install the latest stable Rust toolchain instead of relying on Ubuntu's system-provided Rust packages. This is necessary due to Cargo.lock format compatibility:

- **Ubuntu 24.04 (Noble)** ships with **Rust 1.75.0**
- **Cargo.lock version 4** requires **Rust 1.78+** to parse
- The repository uses lockfile v4 (generated with newer Rust versions)
- Rust 1.75's cargo cannot parse v4 lockfiles, even to regenerate them

### How It Works

The build process in `debian/rules`:

1. **Installation**: During `override_dh_auto_configure`:
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
     sh -s -- -y --default-toolchain stable --profile minimal
   ```

2. **Build**: During `override_dh_auto_build`:
   ```bash
   . $(CARGO_HOME)/env && cargo build --release --locked
   ```

3. **Requirements**:
   - Network access during build (Launchpad provides this)
   - `curl` and `ca-certificates` in Build-Depends
   - `HOME` and `CARGO_HOME` environment variables set

### Benefits

- **Full Cargo.lock v4 Support**: Latest stable Rust can parse modern lockfile formats
- **Reproducible Builds**: Using `--locked` ensures exact dependency versions
- **No Lockfile Version Maintenance**: No need to downgrade or regenerate lockfiles
- **Future-Proof**: Automatically gets Rust updates that support new lockfile formats

### Build Time Impact

- First build downloads rustup (~20-30 seconds)
- Subsequent builds use cached toolchain
- Minimal overhead compared to compilation time

## Troubleshooting

### GPG Key Issues
- Ensure your GPG key is uploaded to Ubuntu keyserver: `gpg --keyserver keyserver.ubuntu.com --send-keys YOUR_KEY_ID`
- The key must match your Launchpad account email

### PPA Upload Failures
- Check that the version number is unique (can't upload same version twice)
- Verify the GPG signature matches your Launchpad key
- Ensure the distribution name is valid (jammy, noble, oracular)

### Build Failures
- Check Launchpad build logs at https://launchpad.net/~YOUR_USERNAME/+archive/ubuntu/YOUR_PPA/+packages
- Common issues: missing dependencies, architecture mismatches