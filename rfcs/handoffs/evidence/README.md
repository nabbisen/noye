# Gate evidence

Captured output from quality gates and test runs. Committed, because
evidence that lives only on the machine that produced it is not
evidence.

## The rule

**Record what a command actually printed. Never assert what it would
have printed.**

The v0.27.2 handoff bundle shipped a `cargo-fmt.log` that stated the
command was not runnable in the capture environment, gave instructions
to regenerate it, and then asserted `# exit code: 0`. A file that
asserts the outcome of a command it did not run is not evidence — it is
a claim wearing evidence's clothes, and it is worse than no file at all
because it stops anyone from looking.

If you cannot run something, write `NOT RUN` and the reason. That is a
useful, honest artifact. An invented exit code is not.

## Format

Each file records, per command:

```
$ <the exact command>
<toolchain version>
<commit SHA>

<real captured output — stdout and stderr, not a summary>

# exit code: <the actual observed code>
```

## Files

Named per subject, not per phase — phase-era names (`baseline-p0-p1.log`,
`phase-0-tests.log`) predate the `rfcs/handoffs/` reorganization and
are not used going forward; subject 36 assembles the full must-fail-first
register at release time, and per-subject names keep it navigable.

| File | Produced by | Contains |
|---|---|---|
| `baseline-<NN>.log` | tester, before implementation starts | The must-fail-first tests failing against the unfixed tree (NFR-QA-09), for subject `NN` |
| `subject-<NN>-tests.log` | tester, once the implementer's fix lands | Same tests passing, plus the regression guards, for subject `NN` |
| `release-<version>.log` | implementer, at each release | fmt, clippy, check, host tests, both WASM target builds, cargo-audit |

## Release gate set

All run with `--locked`:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo check --workspace --locked
cargo test --workspace --lib --bins --locked
cargo check -p noye-gateway --target wasm32-unknown-unknown --locked
cargo check -p noye-core    --target wasm32-unknown-unknown --locked
cargo audit
```

Plus, from v0.28.0: the migration-apply gate (DR-MIG-05) — every file in
`sql/` applied in filename order to an empty database.
