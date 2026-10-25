# Release-candidate procedure

**For:** implementer / tester, at every release
**Standing document** — not a subject. Followed once per release, from
M0.1 (`0.28.1`) onward.

This exists because release-candidate production recurs at every
milestone. A subject per release would be six near-identical documents;
this is the one that gets maintained.

The governance assigns release-candidate production to the implementer,
with the reviewer accountable and the **owner alone approving the
release and creating the tag.**

---

## Ordering is fixed, and not by preference

`package.sh` derives its tag from `[workspace.package].version` and
refuses to build when `HEAD` is not at that tag (subject 03d, T-174). So
the version bump **must** precede the tag, and the tag **must** be at the
bumped commit. There is no order in which this works otherwise.

| # | Step | Who |
|---|---|---|
| 1 | Bump `[workspace.package].version` in `Cargo.toml`; date the `CHANGELOG.md` entry and open a fresh `[Unreleased]` skeleton above it | implementer |
| 2 | Run the full gate set locally; capture into `.git-exclude/evidence/release-<version>.log` | tester |
| 3 | Submit a review request | implementer |
| 4 | Audit; issue a release-readiness report with a recommendation | reviewer |
| 5 | Confirm CI green **at the exact commit to be tagged** | reviewer |
| 6 | Merge, tag — signed, bare version, no `v` — and push the tag | **owner** |
| 7 | **Verify the attached asset**, §"Verifying the artifact" below | tester |
| 8 | Post-release evaluation and roadmap disposition review | reviewer |

Steps 6 is the owner's and no one else's: `tag.gpgsign` is set and every
tag in this repository is signed with the owner's key. A signed tag is an
assertion of authorship.

## Hotfixes: when the release does not come from `main`

Added 2026-10-25, from `0.32.2`. **This procedure assumed every release
is cut from `main`, and a security fix sometimes must not be.**

`0.32.2` fixed RUSTSEC-2026-0385 in `Cargo.lock` while `main` carried
the unreleased `noye provision` (subject 37), whose Gateway was not yet
reachable (G-48). Releasing from `main` would have been a **minor**
shipping an incomplete feature; the fix should neither wait for it nor
drag it along. So:

1. Branch from the **tag**, not `main`: `git checkout -b hotfix/<version> <previous tag>`
2. Apply only the fix. **Nothing else** — a hotfix's reviewability is
   its minimalism. `0.32.2` was one dependency version, a version bump,
   and a changelog section
3. Bump, date the changelog, dry-run the notes — steps 1–3 unchanged
4. **Step 5 needs `workflow_dispatch`.** `ci.yml` triggers on pushes to
   `main` and PRs targeting `main`, so **pushing a hotfix branch runs no
   CI at all** — "CI green at the exact commit to be tagged" is
   otherwise unsatisfiable. Push the branch, then:

   ```bash
   gh workflow run ci.yml --ref hotfix/<version>
   ```

5. Tag on the hotfix branch and push; `release.yml` triggers on the tag
   regardless of branch
6. **Back-port the changelog section to `main`** — the section only,
   never the version bump, since `main` is heading for a different
   number. Without this, `main`'s history silently omits a release that
   exists

**Do not merge a hotfix branch into `main`.** Its version bump conflicts
with `main`'s own next version, and the fix itself should be applied to
`main` separately so `main`'s CI goes green on its own commit.

**Dry-run the release notes before step 6, not after.** `release.yml`
now sources the published notes from the tag's own dated `CHANGELOG.md`
section (subject 04a, G-35) and fails the job — no release created — if
that section is missing or empty. Run `bash
scripts/changelog-section.sh <version>` before the tag is pushed: it
prints exactly what the release will publish, and this is the only
point at which the notes are still cheap to fix. The changelog section
is now a release **gate**, not a courtesy — a version bumped without a
corresponding dated section will fail step 6 outright, not merely ship
with thin notes.

---

## Gate set

All with `--locked` where the tool accepts it:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo check --workspace --locked
cargo test --workspace --lib --bins --locked
cargo test -p noye-shared -p noye-gateway --target wasm32-unknown-unknown --lib --locked
cargo check -p noye-gateway --target wasm32-unknown-unknown --locked
cargo check -p noye-core    --target wasm32-unknown-unknown --locked
cargo audit
bash scripts/check-migrations.sh
```

`cargo audit` takes **no** `--locked` flag — passing it exits 2 before
scanning, which is gap G-32 and is how the scan sat inert for a month.

The wasm **test** line (not merely `cargo check` against the wasm
target) is standing rule 8: a compile-only check would have passed
straight through M2b's regression, which only a live `wasm-bindgen-test`
run against real Web Crypto / Rust-JS boundary code catches. Added to
this gate set at release `0.32.0`, the first release since the rule was
adopted.

Capture real output, never an asserted exit code. See
`.git-exclude/evidence/README.md`.

---

## Verifying the artifact

**Pushing the tag runs `release.yml`, which builds and attaches the
archive. A green run is not the verification.** Inspect the asset.

```bash
gh release download <version> --dir /tmp/rc
tar tzf /tmp/rc/noye-project-v<version>.tar.gz | grep -v '/$' | sed 's|^\./||' | sort > /tmp/rc/in-archive
git ls-tree -r --name-only <version> | sort > /tmp/rc/at-tag
diff /tmp/rc/in-archive /tmp/rc/at-tag && echo "archive == tag"
```

Then confirm:

- `diff` is empty — the archive is exactly the tag's tracked content (PRQ-14)
- No path under `.git-exclude/`, `.claude/`, `target/`, `docs/book/`
- `.cargo/config.toml` and `.vscode/settings.json` **are** present — they
  are tracked, and their absence would mean someone untracked repository
  content to satisfy a test (subject 03d, T-171a)
- `Cargo.lock` is present (DEC-019)
- It unpacks flat — no `noye/` or `noye-<version>/` prefix (PRQ-08)

> **Why inspect rather than trust the run.** Four M0 gaps — G-21, G-32,
> G-33, G-34 — were mechanisms that appeared configured and had never been
> observed producing correct output. Confirming that `release.yml`
> concluded `success` would repeat exactly that. The asset is the output;
> check the output.

---

## Release notes

Draw from the dated `CHANGELOG.md` entry — this is now what
`release.yml` publishes verbatim, not a courtesy summary of it (subject
04a, G-35). State explicitly, in the entry itself:

- Anything that can break a running deployment or local setup
- Migration steps an operator must take
- Known issues carried forward, with the subject that closes each
- Rollback: what reverting restores, including any defect it reinstates

An entry that omits these is a defect in the changelog, not merely
brevity — from `0.29.0` onward, whatever is missing here is missing
from the published release.

---

## Do not

- **Do not run `package.sh` by hand to produce a distributable archive.**
  It refuses an untagged or dirty tree, so it will not silently misbehave
  — but the artifact of record is the one CI attached to the release,
  because that one is observable
- **Do not re-tag or overwrite a release.** Supersede with a new patch
  version. `0.28.0` is permanently archive-less for exactly this reason
- **Do not mark a gate green without captured output**

## Escalate

| Situation | Do |
|---|---|
| A gate fails at step 2 | Report. Do not bump the version over a red gate |
| The attached asset differs from `git ls-tree` at the tag | **Report immediately.** PRQ-14 is violated and the release should not be announced |
| `release.yml` did not trigger on the tag push | Report — check the tag form is a bare version; the trigger matches that pattern |
| A published archive is found containing `.git-exclude/` | **Report immediately.** A disclosure question, not a packaging one |
| The release job fails at "Extract release notes from CHANGELOG.md" | Correct: the tag's dated section is missing or empty. Fix the changelog on a new commit — do not edit a released tag's history; re-tag is forbidden regardless |
| A published release's notes do not match its `CHANGELOG.md` section | **Report immediately.** `scripts/changelog-section.sh` extracted something other than what was reviewed, or the workflow was edited without re-verifying T-178 |
