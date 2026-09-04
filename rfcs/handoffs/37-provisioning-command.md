# 37 — `noye provision`: deployment in minutes, not hours

**Milestone** M2.1 · **Closes** G-45, G-46 · **Satisfies** PRQ-08, CON-09
**Branch** `fix/37-provisioning` · **Depends on** nothing
**Governing artifact** — Gaps **G-45**, **G-46** (§11) ·
`.git-exclude/reviewed/072-provisioning-is-the-defect.md`

## The defect

Deploying Noye is **ten manual steps**, of which **seven are pure
mechanism**: two `cp`s, three resource creations, a migrations apply, a
secret generation registered twice, an `INSERT`, and two ordered deploys.

The project owner attempted it on 2026-09-04 and **stopped, exhausted,
before step 1** — which also answers subject 07a's item 6 ("is
`docs/src/setup.md` sufficient on its own?") in the negative, from the
most informed deployer this project will ever have.

Two specific failures inside that:

- **Step 4 requires hand-copying generated ids** — `database_id` and the
  KV `id` — into `YOUR_DATABASE_ID_HERE`/`YOUR_KV_ID_HERE` in two
  `.toml` files. A wrong paste deploys successfully and fails at runtime.
- **Step 9 tells the operator to hand-write SQL** with a hardcoded
  primary key (`'admin-001'`), which collides on a second run — while
  **`noye admin create` already exists**, is idempotent
  (`INSERT OR IGNORE`), and is documented in `crates/cli/src/main.rs` as
  existing for precisely this chore.

And **G-46**: resource *and worker* names are fixed literals
(`noye-core`, `noye-gateway`, `noye_db`, `noye-logs`), so one Cloudflare
account cannot hold a staging and a production Noye without hand-editing
four literals across two files.

## Build

**`noye provision`, a subcommand of the existing CLI crate.** Not a shell
script — `crates/cli/src/main.rs`'s own "Why a binary crate, not a shell
script" section already settled that, and its "subcommands grouped under
resource names… a structural addition rather than a refactor" is exactly
the shape this needs.

### The scope decision, and the condition attached to it

**First-run only.** Owner's decision 2026-09-04, *"accepted only if it is
safe in the future as well as now, on maintenance, extensibility and
release stability."*

So the constraint is not "make it work"; it is **make idempotent re-run a
later structural addition rather than a rewrite.** That is a design
requirement, and §"The design that earns first-run-only" below is how it
is met. A `provision` that works but has to be torn up to become
idempotent has failed this subject.

### The design that earns first-run-only

**1. Plan and apply are separate.**

```
noye provision --plan     # prints every action, resolved, changes nothing
noye provision --apply    # executes
```

`--plan` is the whole reason this scope is safe. Each step declares its
intent *separately from performing it*, so idempotency later adds a
state query to an existing structure instead of restructuring.

**2. One `Step` trait, one list.**

```rust
trait Step {
    fn describe(&self, ctx: &Plan) -> String;   // for --plan
    fn apply(&self, ctx: &mut Plan) -> Result<()>;
}
```

Idempotency later = one added method (`fn state(&self, …) -> Satisfied |
Missing | Drifted`) and a filter in the runner. **Nothing above changes.**
Say so in a module doc, so the next person knows the extension point is
deliberate.

**3. Names derive from one input — this closes G-46.**

`--env-name <name>`, default `noye`:

| Derived | Default | Staging example |
|---|---|---|
| Core worker | `noye-core` | `noye-staging-core` |
| Gateway worker | `noye-gateway` | `noye-staging-gateway` |
| D1 database | `noye_db` | `noye_staging_db` |
| R2 bucket | `noye-logs` | `noye-staging-logs` |

Binding names (`DB`, `CACHE_KV`, `LOG_BUCKET`, `CORE`) are **not**
derived — they are per-worker identifiers, not account-scoped resources.
Changing them would break code for no benefit.

**4. Secrets never reach disk, logs, or the terminal.**

`setup.md` step 7 currently does `echo "Generated: $SHARED_TOKEN"` — a
secret into scrollback and shell history. `provision` generates it, pipes
it to both `wrangler secret put` invocations, and **never prints it**.
No secret in `--plan` output, in any error message, or in any file this
command writes.

**5. Id resolution is isolated, and never guesses.**

Only two ids need resolving. R2 is referenced by *name* and needs none.

| Resource | Method | Verified |
|---|---|---|
| D1 `database_id` | `wrangler d1 info <name> --json` | ✅ `--json` documented and present |
| KV `id` | `wrangler kv namespace list` | ⚠️ **no `--json` flag is documented** — see Escalate |

Put both behind one small module with one function per resource. **If the
output does not parse into the expected shape, stop and prompt the
operator to paste the id** — never scrape loosely, never write a value
you are not sure of. A wrangler version change must degrade to a prompt,
not to a corrupted config.

**6. Config writing is atomic and refuses to clobber.**

Write to a temp file, parse it back as TOML to prove it is valid, then
rename into place. **Refuse to overwrite an existing `wrangler.toml`**
unless `--force` is given. Someone's filled-in production config is not
yours to replace because a flag was mistyped.

**7. Reuse `admin create`.** Do not re-implement the `INSERT`. Delete
`setup.md`'s raw-SQL step 9 and point at the command.

**8. Fail closed, and report resumable state.** If step 6 of 9 fails,
print exactly which resources now exist and which do not. The operator
must never have to guess what a half-run left behind — and the future
idempotent run has the same information available.

**9. Enforce deploy order.** Core before Gateway; the Service Binding
requires it. If ever bypassed, the error says why.

### What stays human — print it up front

Before doing anything, `provision` prints the three things it cannot do,
so nobody discovers them halfway:

1. `wrangler login`
2. Registering the OAuth client at the provider
3. Supplying: issuer URL, client id, client secret, admin email

**Solve the ordering problem while you are there.** Step 6 of `setup.md`
needs the Gateway's URL for the redirect URI, but the doc puts it before
the deploy that creates it. `provision` knows the derived worker name —
**print the redirect URI the operator will need, before prompting for
OIDC values.**

### Documentation

`docs/src/setup.md` becomes: prerequisites, `wrangler login`, register
the OAuth client, `noye provision --plan`, `noye provision --apply`,
smoke test. The ten-step walkthrough moves to an appendix for anyone who
wants to know what the command does — **not deleted**, since a reader who
must diverge needs it.

## Verify

| # | Test | Type |
|---|---|---|
| T-228 | `--plan` with `--env-name staging` derives all four names correctly and prints them | **must fail first** |
| T-229 | `--plan` changes nothing — no file written, no `wrangler` subprocess spawned | **must fail first** |
| T-230 | The generated shared secret appears in no plan output, no error message, and no written file | **guard — critical** |
| T-231 | D1 id parsed from a captured `d1 info --json` fixture | **must fail first** |
| T-232 | An unparseable id response prompts rather than writing a guessed value | **guard — critical** |
| T-233 | Config writing refuses an existing `wrangler.toml` without `--force`, and writes atomically with `--force` | **must fail first** |
| T-234 | A step failing mid-run reports which resources exist and which do not | **must fail first** |
| T-235 | Adding a `state()` method to `Step` requires no change to the runner's existing call sites | guard — the extensibility contract, asserted |

**T-230 and T-232 are the critical pair.** One is a secret leak, the
other is a silently wrong config that deploys clean and fails at runtime.

**T-235 is unusual and deliberate.** The owner's condition on
first-run-only is that idempotency stays a structural addition. Assert it
rather than promising it — a compile-time test that the trait extends
without touching the runner.

### How this is tested without touching real infrastructure

**Standing rule 7 still applies.** `--plan` is fully testable: name
derivation, ordering, output, and the refusal paths are pure logic. Id
parsing is testable against **captured JSON fixtures**, committed. Config
writing is testable against temp directories.

**`--apply` against a real account is not the dev team's to run.** It is
verified by the owner, during the deployment session — which is item 6's
new and better form: *"does `noye provision` produce a working
deployment?"* answered in minutes, repeatably, instead of a ten-step
walkthrough graded by hand.

## Done

- All eight tests pass; six baseline failures captured
- `cargo test -p noye-shared -p noye-gateway --target wasm32-unknown-unknown --lib --locked` (standing rule 8)
- `docs/src/setup.md` rewritten; ten-step walkthrough preserved as an appendix
- `crates/*/wrangler.toml.example` carry derived-name placeholders and say which values `provision` fills
- `docs/src/requirements.md`: G-45, G-46 struck

## Escalate

**KV id resolution has no documented `--json` flag.** `wrangler kv
namespace list --help` shows no such option, unlike `d1 list`/`d1 info`
which both document one. It may emit JSON anyway, and confirming that
needs an account — which you do not have.

**Do not guess, and do not scrape the human-readable table.** Build the
manual-prompt fallback (design point 5) *first*, so the command is
correct whichever way this lands, then report what you find. The owner
can confirm the actual shape in one command during the deployment
session, and the JSON path can be added behind the same interface
afterwards.

This is the same escalation shape as 06a's Build step 3: a step that
cannot be satisfied inside the access boundary is a defect in the
subject, not in the developer.
