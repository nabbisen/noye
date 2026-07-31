#!/usr/bin/env bash
# Prints the body of a dated CHANGELOG.md section for <version> — the
# content of `## [<version>] — <date>`, up to but excluding the next
# `## [` heading.
#
# Used by .github/workflows/release.yml to source the published
# release notes from the curated changelog rather than an
# auto-generated commit summary (subject 04a, G-35). Read-only: this
# script never writes CHANGELOG.md. A release whose changelog section
# was never written must fail here, before anything is published —
# not fall back to a generated body.
#
# Usage: scripts/changelog-section.sh <version>
#   <version> is matched exactly against `## [<version>]` — a prefix
#   match would let `0.28.1` match `## [0.28.10]`.
#
# CHANGELOG_FILE overrides the changelog path (default: CHANGELOG.md
# at the repo root). Not used by release.yml — for
# scripts/check-changelog-section.sh to exercise this script against
# disposable fixtures instead of the real, ever-changing file.
#
# Exits non-zero, with a message on stderr, when:
#   - no `## [<version>]` heading exists in CHANGELOG.md
#   - the heading exists but its body (up to the next `## [` heading,
#     or end of file) is empty or whitespace-only
set -eu

VERSION="${1:?Usage: $0 <version>}"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CHANGELOG="${CHANGELOG_FILE:-$REPO_ROOT/CHANGELOG.md}"
HEADING="## [${VERSION}]"

if SECTION="$(awk -v heading="$HEADING" '
  {
    if (substr($0, 1, length(heading)) == heading) {
      if (found) { exit }
      found = 1
      printing = 1
      next
    }
    if (substr($0, 1, 4) == "## [") {
      if (printing) { exit }
      next
    }
    if (printing) { print }
  }
  END { exit (found ? 0 : 2) }
' "$CHANGELOG")"; then
  : # heading found
else
  echo "changelog-section: no '${HEADING}' section found in $CHANGELOG" >&2
  exit 1
fi

if [ -z "$(printf '%s' "$SECTION" | tr -d '[:space:]')" ]; then
  echo "changelog-section: '${HEADING}' section is empty in $CHANGELOG" >&2
  exit 1
fi

printf '%s\n' "$SECTION"
