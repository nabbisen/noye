# 28a — The floor is AA; the audience needs more

**Milestone** M4 · **Closes** G-49, G-50, G-51, G-52
**Satisfies** NFR-A11Y-14…18 · **Implements** DEC-024
**Branch** `feat/28a-age-related-usability` · **Depends on** nothing for G-49; subjects 25–27 for the rest
**Governing artifacts** — **DEC-024**, gaps **G-49**…**G-52**,
`.git-exclude/reviewed/078-age-related-usability-assessment.md`

## Why this is separate from subject 28

**Subject 28 asks "does the surface meet the standard".** It sweeps
sixteen screens for landmarks, tab order, reduced motion, no-CSS
operation — all thirteen `NFR-A11Y-01…13`, all already `Implemented`.

**This subject asks a different question: "is the standard enough for
who uses it?"** DEC-024 answers no. WCAG 2.1 AA sets **no minimum font
size, no target size at AA, and nothing about information density** — so
a product can hold every AA requirement and still render its form help
text at 12px. This one does.

Keeping them separate matters because 28 is a conformance sweep that
either passes or names a route; this is design work with trade-offs.

## Build

### Step 1 — G-49, and do it first and alone

```css
/* crates/gateway/src/ui/layout/style.rs:166 */
html { font-size: 100%; }   /* was: 16px */
```

**One line, and it changes the rendered size of the entire surface** for
anyone whose browser default is not 16px — which is the point. Nineteen
`rem` values already sit on this root; they have been scaling from a
base that discarded the user's preference.

**Land this as its own commit before anything else**, so that if any
later step's screenshots or spacing look wrong, you can tell whether the
cause is this change or yours.

### Step 2 — G-50, the type floor

`--fs-xs: 0.75rem` (12px) carries eleven declarations. Raise the floor
so no informative text renders below 14px, and body and help text reach
16px. At minimum:

| Site | Now | Must reach |
|---|---|---|
| `.field-help` (`:550`) | 12px | **16px** — it is help text |
| `.btn-sm` (`:436`) | 12px | 14px, and see step 3 |
| `.nav-group-label` (`:233`) | 12px + uppercase | 14px |
| `.summary-item .label` (`:359`) | 12px + uppercase | 14px |

**Drop `text-transform: uppercase` on both labels that carry it.**
Uppercase removes ascender/descender word shapes, and that cue matters
more as acuity declines — so today the smallest text is also the
hardest-shaped. Keep the letter-spacing if you like the look; lose the
uppercase.

**Anything you keep below 14px must go on an enumerated exemption list
with its reason** (NFR-A11Y-15). "It looked cramped otherwise" is a
reason — an honest list beats a quiet exception.

### Step 3 — G-51, target size

Computed today: `.btn` ≈ **38px**, `.btn-sm` ≈ **27px**. The floor is
**44 × 44px**.

`.btn-sm` is the hard case: it exists to be dense, and 44px is not
dense. **Do not simply delete it** — find where it is actually used
first. If it is only in table rows, that is exactly the "dense tabular
control" NFR-A11Y-16 permits at 24px, and the right answer is to
enumerate it rather than inflate every table.

### Step 4 — G-52, AAA contrast

`contrast.rs:97` is the whole change:

```rust
let threshold = if body { 7.0 } else { 4.5 };   /* was 4.5 / 3.0 */
```

**Then fix whatever tokens that breaks.** The gate is already pinned to
the token set and already breaks the build (NFR-A11Y-02), so this step
tells you its own scope — flip the constant and read the failures.

**One thing the ratio cannot tell you.** Lens yellowing reduces
short-wavelength discrimination, so **blue-against-grey separates less
well with age than its computed luminance ratio implies.** The formula
models luminance, not that. Look at every pair where blue or violet
carries meaning, with your own eyes, and say in the report what you
decided. A passing number is not the same as a legible pair.

### Step 5 — NFR-A11Y-18, "less is more"

**Not gate-able, deliberately.** For each screen in
`docs/src/external-design.md` §4, state **one primary question** the
screen exists to answer. Then anything on it that does not help answer
that question is collapsed behind a disclosure, or removed.

**FR-UI-08 is the pattern already in the codebase** — the dashboard
omits all-zero breakdown sections. That instinct, applied once, becomes
the rule.

**Where a screen cannot be reduced to one question, say so and say
why.** That is a finding about the screen, and more useful than a forced
answer.

## Verify

| # | Test | Type |
|---|---|---|
| T-243 | No absolute `font-size` on `html`; a doubled root size doubles rendered text | **must fail first** |
| T-244 | Every `font-size` token resolves ≥14px, or is on the exemption list with a reason | **must fail first** |
| T-245 | `.field-help` resolves ≥16px | **must fail first** |
| T-246 | No `text-transform: uppercase` on `.nav-group-label` or `.summary-item .label` | **must fail first** |
| T-247 | Every interactive class computes ≥44px, or is on the exemption list | **must fail first** |
| T-248 | The contrast gate rejects a 4.5:1 body pair that AA accepted | **must fail first** |
| T-249 | Every token pair passes AAA, in both themes | **guard — critical** |
| T-250 | Every §4 screen names exactly one primary question | guard |

**T-243 through T-247 belong beside `contrast.rs`'s own checks** — a
pinned assertion over the token file, host-testable, no browser. That
pattern already exists in this codebase and is why G-52 is a
one-constant change; point it at type and target size too.

**T-248 is the one that proves the bar moved.** Asserting that AAA
passes proves nothing on its own — the tokens might already have
passed. Assert that a pair which *used to* pass now fails.

## Done

- All eight tests pass; six baseline failures captured
- `cargo test -p noye-shared -p noye-gateway --target wasm32-unknown-unknown --lib --locked` (standing rule 8)
- `docs/src/requirements.md`: NFR-A11Y-14…18 → `Implemented`; G-49…G-52 struck
- `docs/src/external-design.md` §4: one primary question per screen
- `docs/src/accessibility.md`: the exemption lists, with reasons
- The blue/grey judgement from step 4 recorded in the review request

## Escalate

**If AAA cannot be met without losing a distinction the interface needs**
— two statuses that become indistinguishable at 7:1, say — **report it
with the measured ratios before changing the palette.** NFR-A11Y-17
permits a listed exception with a reason; it does not permit a silently
lowered threshold, and NFR-A11Y-03 already means no status depends on
colour alone, so the fallback exists.

**If step 1 visibly breaks layout** at a non-16px root, that is a finding
about the layout, not a reason to re-pin the root. Report it.
