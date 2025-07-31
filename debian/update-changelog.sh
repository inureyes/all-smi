#!/usr/bin/env bash
# --------------------------------------------------------------
# Regenerates debian/changelog from GitHub releases.
# •	If you pass a <tag> argument, only that release is processed.
# •	If you pass multiple tags separated by spaces, they’re processed in that order.
# •	If no tags are supplied, the script processes the latest 100 releases.
# •	Use -d | --distro to set the target distribution (e.g., jammy, noble); the default is jammy.
# --------------------------------------------------------------

set -euo pipefail

DISTRO="jammy"
TAGS=()

# --------------------- CLI parsing ---------------------------
while [[ $# -gt 0 ]]; do
  case "$1" in
    -d|--distro)
      DISTRO="$2"; shift 2 ;;
    -h|--help)
      echo "Usage: $0 [-d distro] [tag1 [tag2 ...]]"; exit 0 ;;
    *)
      TAGS+=("$1"); shift ;;
  esac
done

# --------------------- Dependency check ----------------------
command -v gh   >/dev/null || { echo "❌ gh CLI not found"; exit 1; }
command -v jq   >/dev/null || { echo "❌ jq not found"; exit 1; }

# --------------------- Collect release list ------------------
if [[ ${#TAGS[@]} -eq 0 ]]; then
  # Latest 100 tags if no specific tags provided
  mapfile -t TAGS < <(gh release list --limit 100 --json tagName -q '.[].tagName')
fi

# debian/changelog 
> debian/changelog.tmp

for TAG in "${TAGS[@]}"; do
  echo "ℹ️  Processing $TAG"
  rel_json=$(gh release view "$TAG" --json tagName,publishedAt,name,body)

  VERSION="${TAG#v}"
  DATE=$(echo "$rel_json" | jq -r '.publishedAt')
  NAME=$(echo "$rel_json" | jq -r '.name')
  BODY=$(echo "$rel_json" | jq -r '.body')

  # Debian date format
  FORMATTED_DATE=$(date -d "$DATE" "+%a, %d %b %Y %H:%M:%S %z")

  cat >> debian/changelog.tmp <<EOF
all-smi (${VERSION}-1~${DISTRO}1) ${DISTRO}; urgency=medium

  * ${NAME}
$(echo "$BODY" | sed 's/^/  /')

 -- Jeongkyu Shin <inureyes@gmail.com>  ${FORMATTED_DATE}

EOF
done

mv debian/changelog.tmp debian/changelog
echo "✅ debian/changelog updated for: ${TAGS[*]}"