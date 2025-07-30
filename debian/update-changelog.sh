#!/bin/bash
# Script to update debian/changelog from GitHub releases

set -e

# Check if gh CLI is installed
if ! command -v gh &> /dev/null; then
    echo "Error: GitHub CLI (gh) is not installed"
    exit 1
fi

# Get all releases
RELEASES=$(gh release list --limit 100 --json tagName,publishedAt,name,body)

# Start fresh changelog
echo -n "" > debian/changelog.tmp

# Process each release
echo "$RELEASES" | jq -r '.[] | @base64' | while read -r release; do
    _jq() {
        echo "${release}" | base64 -d | jq -r "${1}"
    }
    
    TAG=$(_jq '.tagName')
    VERSION="${TAG#v}"
    DATE=$(_jq '.publishedAt')
    NAME=$(_jq '.name')
    BODY=$(_jq '.body')
    
    # Format date for debian changelog
    FORMATTED_DATE=$(date -d "$DATE" "+%a, %d %b %Y %H:%M:%S %z")
    
    # Write changelog entry
    cat >> debian/changelog.tmp << EOF
all-smi (${VERSION}-1~ubuntu1) jammy; urgency=medium

  * ${NAME}
$(echo "$BODY" | sed 's/^/  /')

 -- Jeongkyu Shin <inureyes@gmail.com>  ${FORMATTED_DATE}

EOF
done

# Replace the changelog
mv debian/changelog.tmp debian/changelog

echo "Updated debian/changelog from GitHub releases"