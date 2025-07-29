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
   
   # Use the ID after "sec rsa4096/" (e.g., "3AA5C34371567BD2")
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

The workflow is configured to upload to: `ppa:inureyes/all-smi`

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
sudo add-apt-repository ppa:inureyes/all-smi
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