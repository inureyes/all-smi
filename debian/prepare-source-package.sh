#!/bin/bash
set -euo pipefail

# This script prepares the source package for Launchpad PPA upload
# It switches from binary-based packaging to source-based packaging

echo "Preparing source package for Launchpad..."

# Check if we're in the right directory
if [ ! -f "Cargo.toml" ]; then
    echo "Error: Must be run from the project root directory"
    exit 1
fi

# Backup original files
cp debian/control debian/control.binary
cp debian/rules debian/rules.binary

# Use source-based packaging files
cp debian/control.source debian/control
cp debian/rules.source debian/rules

# Make rules executable
chmod +x debian/rules

# Vendor Rust dependencies for offline build
echo "Vendoring Rust dependencies..."
cargo vendor debian/vendor

# Create cargo config to use vendored dependencies
mkdir -p .cargo
cat > .cargo/config.toml << 'EOF'
[source.crates-io]
replace-with = "vendored-sources"

[source.vendored-sources]
directory = "debian/vendor"
EOF

echo "Source package preparation complete!"
echo "The package will now build from source on Launchpad"