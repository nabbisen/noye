#!/usr/bin/env bash
# scripts/changelog-section.sh gate (subject: rfcs/handoffs/04a-release-notes-source.md).
#
# scripts/changelog-section.sh is a release gate: release.yml refuses to
# publish when it fails, and trusts its output verbatim when it
# succeeds. A regression here either blocks every release or publishes
# the wrong notes, and nothing else would catch it before someone
# tags. This exercises it against disposable fixtures — same shape as
# scripts/check-migrations.sh — instead of leaving it verified only in
# an evidence log against fixtures that no longer exist.
#
#   T-179  — prints only the requested section, not an adjacent one
#   T-179a — the last section in the file terminates at EOF, not an error
#   T-181  — exits non-zero when the section body is empty
#   T-181a — exits non-zero when the section body is whitespace-only
#   T-182  — an exact heading match, not a prefix: 0.28.1 vs 0.28.10
#   T-182a — duplicate headings for one version: the first section wins
#   (unnumbered) — a version with no heading at all exits non-zero
#
# Per standing practice (T-166, T-170, T-177): also proves the checks
# themselves can fail, by running the extractor against a fixture
# where at least one property is deliberately violated and confirming
# a mismatch is detected — a check that only ever passes is
# indistinguishable from one that does not run.

set -u
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EXTRACTOR="$REPO_ROOT/scripts/changelog-section.sh"
WORKDIR="$(mktemp -d)"
trap 'rm -rf "$WORKDIR"' EXIT

fail() {
  echo "FAIL: $1" >&2
  exit 1
}

# run <fixture-file> <version> -- prints extractor stdout, returns its exit code
run() {
  CHANGELOG_FILE="$1" bash "$EXTRACTOR" "$2"
}

# ── T-179 — only the requested section, not an adjacent one ──
FIXTURE="$WORKDIR/t179.md"
cat >"$FIXTURE" <<'EOF'
# Changelog

## [Unreleased]

### Added

- unreleased-marker-should-not-leak

## [1.2.0] — 2026-01-02

### Added

- correct-section-content

## [1.1.0] — 2026-01-01

### Added

- older-section-should-not-leak
EOF
OUT="$(run "$FIXTURE" 1.2.0)" || fail "T-179: extractor exited non-zero on a valid section"
echo "$OUT" | grep -q "correct-section-content" || fail "T-179: requested section's own content missing"
echo "$OUT" | grep -q "unreleased-marker-should-not-leak" && fail "T-179: leaked the section above"
echo "$OUT" | grep -q "older-section-should-not-leak" && fail "T-179: leaked the section below"
echo "PASS T-179: prints only the requested section"

# ── T-179a — the last section in the file terminates at EOF ──
FIXTURE="$WORKDIR/t179a.md"
cat >"$FIXTURE" <<'EOF'
# Changelog

## [Unreleased]

### Added

## [1.0.0] — 2026-01-01

### Added

- last-section-at-eof
EOF
OUT="$(run "$FIXTURE" 1.0.0)" || fail "T-179a: extractor exited non-zero on the last section in the file"
echo "$OUT" | grep -q "last-section-at-eof" || fail "T-179a: last-section content missing"
echo "PASS T-179a: the last section in the file terminates at EOF"

# ── T-181 — exits non-zero when the section body is empty ──
FIXTURE="$WORKDIR/t181.md"
cat >"$FIXTURE" <<'EOF'
# Changelog

## [2.0.0] — 2026-01-01

## [1.0.0] — 2025-01-01

### Added

- prior release content
EOF
if run "$FIXTURE" 2.0.0 >/dev/null 2>&1; then
  fail "T-181: extractor succeeded on an empty section body"
fi
echo "PASS T-181: exits non-zero when the section body is empty"

# ── T-181a — exits non-zero when the section body is whitespace-only ──
FIXTURE="$WORKDIR/t181a.md"
printf '# Changelog\n\n## [3.0.0] — 2026-01-01\n\n   \n\t\n\n## [2.0.0] — 2025-06-01\n\n### Added\n\n- content\n' >"$FIXTURE"
if run "$FIXTURE" 3.0.0 >/dev/null 2>&1; then
  fail "T-181a: extractor succeeded on a whitespace-only section body"
fi
echo "PASS T-181a: exits non-zero when the section body is whitespace-only"

# ── T-182 — an exact heading match, not a prefix ──
FIXTURE="$WORKDIR/t182.md"
cat >"$FIXTURE" <<'EOF'
# Changelog

## [0.28.10] — 2099-01-01

### Added

- should-not-match-0.28.1-query

## [0.28.1] — 2026-07-30

### Added

- correct-0.28.1-content
EOF
OUT="$(run "$FIXTURE" 0.28.1)" || fail "T-182: extractor exited non-zero on 0.28.1"
echo "$OUT" | grep -q "correct-0.28.1-content" || fail "T-182: 0.28.1's own content missing"
echo "$OUT" | grep -q "should-not-match-0.28.1-query" && fail "T-182: 0.28.1 query matched the 0.28.10 section"
echo "PASS T-182: 0.28.1 does not match ## [0.28.10]"

# ── T-182a — duplicate headings for one version: the first wins ──
FIXTURE="$WORKDIR/t182a.md"
cat >"$FIXTURE" <<'EOF'
# Changelog

## [1.5.0] — 2026-01-01

### Added

- first-section-should-win

## [1.5.0] — 2026-01-01

### Added

- second-section-should-not-appear
EOF
OUT="$(run "$FIXTURE" 1.5.0)" || fail "T-182a: extractor exited non-zero on a duplicated heading"
echo "$OUT" | grep -q "first-section-should-win" || fail "T-182a: first section's content missing"
echo "$OUT" | grep -q "second-section-should-not-appear" && fail "T-182a: leaked the second, duplicate section"
echo "PASS T-182a: duplicate headings for one version — the first section wins"

# ── (unnumbered) — no heading at all exits non-zero, with a message ──
FIXTURE="$WORKDIR/missing.md"
cat >"$FIXTURE" <<'EOF'
# Changelog

## [1.0.0] — 2026-01-01

### Added

- content
EOF
ERR="$(run "$FIXTURE" 9.9.9 2>&1 >/dev/null)"
STATUS=$?
[ "$STATUS" -ne 0 ] || fail "missing-section: extractor succeeded for a version with no heading"
echo "$ERR" | grep -q "9.9.9" || fail "missing-section: stderr did not name the missing version"
echo "PASS missing-section: a version with no heading exits non-zero, naming it on stderr"

# ── Prove T-182's assertion can fail (T-166/T-170/T-177 pattern) ──
# A check that only ever passes is indistinguishable from one that
# never runs. Reintroduce the exact bug T-182 exists to catch — drop
# the closing bracket from the match, so a prefix collides — in a
# throwaway copy of the extractor, and confirm T-182's own assertion
# (not a new one) now detects the leak. This is deliberately the same
# fixture and the same grep as T-182 above, run against a mutated
# script instead of a mutated fixture.
BROKEN_EXTRACTOR="$WORKDIR/changelog-section-broken.sh"
sed 's/substr(\$0, 1, length(heading)) == heading/substr($0, 1, length(heading)-1) == substr(heading, 1, length(heading)-1)/' \
  "$EXTRACTOR" >"$BROKEN_EXTRACTOR"
diff -q "$EXTRACTOR" "$BROKEN_EXTRACTOR" >/dev/null && fail "self-check: sed did not mutate the extractor copy — the pattern no longer matches the source"
OUT="$(CHANGELOG_FILE="$WORKDIR/t182.md" bash "$BROKEN_EXTRACTOR" 0.28.1)" || fail "self-check: mutated extractor exited non-zero unexpectedly"
echo "$OUT" | grep -q "should-not-match-0.28.1-query" || fail "self-check: T-182's regression was NOT reproduced — the mutation, the fixture, or T-182's own assertion no longer matches the defect it exists to catch"
echo "PASS self-check: T-182's assertion catches the exact-match regression when reintroduced"

echo
echo "All changelog-section gate checks passed."
