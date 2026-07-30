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

---

## Gate set

All with `--locked` where the tool accepts it:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo check --workspace --locked
cargo test --workspace --lib --bins --locked
cargo check -p noye-gateway --target wasm32-unknown-unknown --locked
cargo check -p noye-core    --target wasm32-unknown-unknown --locked
cargo audit
bash scripts/check-migrations.sh
```

`cargo audit` takes **no** `--locked` flag — passing it exits 2 before
scanning, which is gap G-32 and is how the scan sat inert for a month.

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

Draw from the dated `CHANGELOG.md` entry. State explicitly:

- Anything that can break a running deployment or local setup
- Migration steps an operator must take
- Known issues carried forward, with the subject that closes each
- Rollback: what reverting restores, including any defect it reinstates

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
