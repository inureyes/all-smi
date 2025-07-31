#!/usr/bin/env bash
# --------------------------------------------------------------
# Regenerates debian/changelog from GitHub releases.
#   • If <tag> 인자를 주면 그 릴리스 하나만 반영
#   • 여러 태그를 공백으로 구분해 주면 그 순서대로
#   • 인자가 없으면 모든 릴리스를 최신순 100개
#   • -d|--distro 로 배포판(jammy/noble 등) 지정 (기본 jammy)
# --------------------------------------------------------------

set -euo pipefail

DISTRO="jammy"
TAGS=()

# --------------------- CLI 파싱 -------------------------------
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

# --------------------- 의존성 체크 ----------------------------
command -v gh   >/dev/null || { echo "❌ gh CLI not found"; exit 1; }
command -v jq   >/dev/null || { echo "❌ jq not found"; exit 1; }

# --------------------- 릴리스 목록 수집 ------------------------
if [[ ${#TAGS[@]} -eq 0 ]]; then
  # 최신 100개 전체
  MAPFILE -t TAGS < <(gh release list --limit 100 --json tagName -q '.[].tagName')
fi

# debian/changelog 새로 작성
> debian/changelog.tmp

for TAG in "${TAGS[@]}"; do
  echo "ℹ️  Processing $TAG"
  rel_json=$(gh release view "$TAG" --json tagName,publishedAt,name,body)

  VERSION="${TAG#v}"
  DATE=$(echo "$rel_json" | jq -r '.publishedAt')
  NAME=$(echo "$rel_json" | jq -r '.name')
  BODY=$(echo "$rel_json" | jq -r '.body')

  # Debian 날짜 형식
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