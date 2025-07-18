#!/usr/bin/env bash
set -e

echo "🚀 Starting full Luwen crate vendoring workflow (cross-platform)..."

# Detect sed inline edit syntax based on OS
if [[ "$OSTYPE" == "darwin"* ]]; then
  SED_INPLACE="sed -i ''"
else
  SED_INPLACE="sed -i"
fi

SRC_ROOT="references/luwen/crates"
DEST_ROOT="vendor"

declare -A CRATE_RENAMES=(
  ["luwen-core"]="all-smi-luwen-core"
  ["luwen-if"]="all-smi-luwen-if"
  ["luwen-ref"]="all-smi-luwen-ref"
  ["ttkmd-if"]="all-smi-ttkmd-if"
)

mkdir -p "$DEST_ROOT"

# 1. Copy and rename crates
echo "📦 Copying and renaming crates..."
for OLD_NAME in "${!CRATE_RENAMES[@]}"; do
  NEW_NAME="${CRATE_RENAMES[$OLD_NAME]}"
  SRC="$SRC_ROOT/$OLD_NAME"
  DEST="$DEST_ROOT/$NEW_NAME"

  echo "  🛠️ $OLD_NAME → $NEW_NAME"
  rm -rf "$DEST"
  cp -r "$SRC" "$DEST"

  rm -rf "$DEST/.git" "$DEST/tests" "$DEST/examples" "$DEST/target"

  $SED_INPLACE "s/^name = .*/name = \"$NEW_NAME\"/" "$DEST/Cargo.toml"
  $SED_INPLACE "/^publish *= *false/d" "$DEST/Cargo.toml"
done

# Copy axi-data into all-smi-luwen-if
echo "📁 Copying axi-data into all-smi-luwen-if..."
cp -r "references/luwen/axi-data" "$DEST_ROOT/all-smi-luwen-if/axi-data"

# Patch RustEmbed attribute in chip_comms.rs
EMBED_TARGET="$DEST_ROOT/all-smi-luwen-if/src/chip/communication/chip_comms.rs"
if [ -f "$EMBED_TARGET" ]; then
  echo "🛠️ Patching #[folder] path in $EMBED_TARGET"
  $SED_INPLACE 's|#\[folder = \"../../axi-data\"\]|#[folder = \"axi-data\"]|' "$EMBED_TARGET"
fi

# 2. Update dependencies in Cargo.toml
echo "🔧 Updating Cargo.toml dependencies..."
find "$DEST_ROOT" -type f -name Cargo.toml | while read file; do
  for OLD in "${!CRATE_RENAMES[@]}"; do
    NEW="${CRATE_RENAMES[$OLD]}"
    $SED_INPLACE "s|^$OLD *= *{|$NEW = {|g" "$file"
    $SED_INPLACE "s|path *= *\"[^\"]*${OLD}\"|path = \"../${NEW}\"|g" "$file"
    $SED_INPLACE "s|^$OLD *= *\"|$NEW = \"|g" "$file"
  done
done

# 3. Patch .rs files
echo "🔁 Fixing Rust module references..."
declare -A RENAME_MODULES=(
  ["luwen_core"]="all_smi_luwen_core"
  ["luwen_if"]="all_smi_luwen_if"
  ["luwen_ref"]="all_smi_luwen_ref"
  ["ttkmd_if"]="all_smi_ttkmd_if"
)

for OLD_MOD in "${!RENAME_MODULES[@]}"; do
  NEW_MOD="${RENAME_MODULES[$OLD_MOD]}"
  echo "  🔧 $OLD_MOD:: → $NEW_MOD::"
  find "$DEST_ROOT" -type f -name "*.rs" | while read file; do
    $SED_INPLACE "s/${OLD_MOD}::/${NEW_MOD}::/g" "$file"
  done
done

# 4. cargo check
echo "🧪 Running cargo check on vendored crates..."
for NEW_NAME in "${CRATE_RENAMES[@]}"; do
  echo "  🔍 Checking $NEW_NAME..."
  (cd "$DEST_ROOT/$NEW_NAME" && cargo check || echo "⚠️ Failed: $NEW_NAME")
done

echo "✅ All done!"
