use super::*;
use std::cell::RefCell;
use std::rc::Rc;

fn test_plan() -> Plan {
    Plan::new(names::Names::derive("noye"), false)
}

// ── Fakes used only by the runner tests below. Never touch a
// filesystem or a subprocess -- that's the point: these tests prove
// properties of `run_plan`/`run_apply` themselves, independent of what
// any real production step does.

struct Recording {
    name: &'static str,
    applied: Rc<RefCell<Vec<&'static str>>>,
    fail: bool,
}

impl Step for Recording {
    fn name(&self) -> &'static str {
        self.name
    }
    fn describe(&self, _plan: &Plan) -> String {
        format!("would run: {}", self.name)
    }
    fn apply(&self, _plan: &mut Plan) -> Result<()> {
        self.applied.borrow_mut().push(self.name);
        if self.fail {
            return Err(anyhow::anyhow!("{} deliberately failed", self.name));
        }
        Ok(())
    }
}

/// A step whose `apply` panics if ever called -- used by the T-229
/// tests, which must never reach it.
struct PanicsIfApplied;
impl Step for PanicsIfApplied {
    fn name(&self) -> &'static str {
        "must_not_apply"
    }
    fn describe(&self, _plan: &Plan) -> String {
        "described, never applied".to_string()
    }
    fn apply(&self, _plan: &mut Plan) -> Result<()> {
        panic!("run_plan must never call apply() on any step");
    }
}

// ── T-229 (must fail first): `--plan` changes nothing -- no file
// written, no subprocess spawned. Modeled here as "no step's apply() is
// ever called", which is the property `run_plan` must guarantee
// regardless of what a step's apply() would have done.

#[test]
fn t229_run_plan_never_calls_apply_on_any_step() {
    let steps: Vec<Box<dyn Step>> = vec![Box::new(PanicsIfApplied)];
    let plan = test_plan();
    run_plan(&steps, &plan); // would panic if this property broke
}

#[test]
fn t229_run_plan_leaves_the_plans_completed_list_empty() {
    let applied = Rc::new(RefCell::new(Vec::new()));
    let steps: Vec<Box<dyn Step>> = vec![Box::new(Recording {
        name: "a",
        applied: applied.clone(),
        fail: false,
    })];
    let plan = test_plan();
    run_plan(&steps, &plan);
    assert!(plan.completed.is_empty());
    assert!(
        applied.borrow().is_empty(),
        "apply() must never run under --plan"
    );
}

// ── T-234 (must fail first): a step failing mid-run reports which
// resources exist and which do not.

#[test]
fn t234_a_step_failing_midrun_stops_before_later_steps_run() {
    let applied = Rc::new(RefCell::new(Vec::new()));
    let steps: Vec<Box<dyn Step>> = vec![
        Box::new(Recording {
            name: "first",
            applied: applied.clone(),
            fail: false,
        }),
        Box::new(Recording {
            name: "second",
            applied: applied.clone(),
            fail: true,
        }),
        Box::new(Recording {
            name: "third",
            applied: applied.clone(),
            fail: false,
        }),
    ];
    let mut plan = test_plan();
    let result = run_apply(&steps, &mut plan);

    assert!(result.is_err());
    assert_eq!(
        *applied.borrow(),
        vec!["first", "second"],
        "third must never be attempted once second fails"
    );
}

#[test]
fn t234_completed_reflects_exactly_the_steps_that_succeeded() {
    let applied = Rc::new(RefCell::new(Vec::new()));
    let steps: Vec<Box<dyn Step>> = vec![
        Box::new(Recording {
            name: "first",
            applied: applied.clone(),
            fail: false,
        }),
        Box::new(Recording {
            name: "second",
            applied: applied.clone(),
            fail: true,
        }),
    ];
    let mut plan = test_plan();
    let _ = run_apply(&steps, &mut plan);

    assert_eq!(plan.completed, vec!["first"]);
}

#[test]
fn t234_resumable_state_report_names_what_exists_and_what_does_not() {
    let steps: Vec<Box<dyn Step>> = vec![
        Box::new(Recording {
            name: "first",
            applied: Rc::new(RefCell::new(Vec::new())),
            fail: false,
        }),
        Box::new(Recording {
            name: "second",
            applied: Rc::new(RefCell::new(Vec::new())),
            fail: false,
        }),
        Box::new(Recording {
            name: "third",
            applied: Rc::new(RefCell::new(Vec::new())),
            fail: false,
        }),
    ];
    let mut plan = test_plan();
    plan.completed.push("first");

    let report = resumable_state_report(&steps, &plan);
    assert!(report.contains("first"));
    let exists_section = report.split("do NOT exist").next().unwrap();
    assert!(exists_section.contains("first"));
    assert!(!exists_section.contains("second"));

    assert!(report.contains("second"));
    assert!(report.contains("third"));
}

#[test]
fn t234_resumable_state_report_on_a_fresh_plan_says_none_exist_yet() {
    let steps: Vec<Box<dyn Step>> = vec![Box::new(Recording {
        name: "only",
        applied: Rc::new(RefCell::new(Vec::new())),
        fail: false,
    })];
    let plan = test_plan();
    let report = resumable_state_report(&steps, &plan);
    assert!(report.contains("(none)"));
}

// ── T-235: the owner's condition on first-run-only scope is that
// idempotent re-run stays addable as a structural addition, not a
// rewrite. That means `run_apply` -- which takes `&[Box<dyn Step>]` --
// must itself be able to skip a satisfied step. A prior version of
// this test used a `StepWithState: Step` *supertrait* instead of a
// defaulted method on `Step`; it compiled and looked like it proved
// the claim, but a `dyn Step` cannot reach a supertrait's methods, so
// the runner could never actually have consulted it -- caught in
// review (ruling `.git-exclude/reviewed/075-subject-37-ruling.md` §1,
// proved with a minimal two-trait `rustc` probe), not by inspection.
// `Step::state` is a defaulted method on `Step` itself instead, which
// is what makes both halves below true at once.

#[test]
fn t235_run_apply_skips_a_step_reporting_satisfied_yes() {
    let applied = Rc::new(RefCell::new(false));

    struct AlreadySatisfied {
        applied: Rc<RefCell<bool>>,
    }
    impl Step for AlreadySatisfied {
        fn name(&self) -> &'static str {
            "already_satisfied"
        }
        fn describe(&self, _plan: &Plan) -> String {
            "already satisfied".to_string()
        }
        fn apply(&self, _plan: &mut Plan) -> Result<()> {
            *self.applied.borrow_mut() = true;
            Ok(())
        }
        fn state(&self, _plan: &Plan) -> Satisfied {
            Satisfied::Yes
        }
    }

    let steps: Vec<Box<dyn Step>> = vec![Box::new(AlreadySatisfied {
        applied: applied.clone(),
    })];
    let mut plan = test_plan();
    run_apply(&steps, &mut plan).expect("a satisfied step must not fail the run");

    assert!(
        !*applied.borrow(),
        "run_apply must skip apply() when state() reports Satisfied::Yes"
    );
    assert_eq!(
        plan.completed,
        vec!["already_satisfied"],
        "a skipped step still counts as present for resumable-state reporting"
    );
}

#[test]
fn t235_run_apply_runs_a_step_reporting_satisfied_no() {
    // `No` (drift found -- run it) must behave exactly like `Unknown`
    // (nothing checked -- run it): `run_apply` only skips on `Yes`.
    let applied = Rc::new(RefCell::new(false));

    struct NeedsRunning {
        applied: Rc<RefCell<bool>>,
    }
    impl Step for NeedsRunning {
        fn name(&self) -> &'static str {
            "needs_running"
        }
        fn describe(&self, _plan: &Plan) -> String {
            "needs running".to_string()
        }
        fn apply(&self, _plan: &mut Plan) -> Result<()> {
            *self.applied.borrow_mut() = true;
            Ok(())
        }
        fn state(&self, _plan: &Plan) -> Satisfied {
            Satisfied::No
        }
    }

    let steps: Vec<Box<dyn Step>> = vec![Box::new(NeedsRunning {
        applied: applied.clone(),
    })];
    let mut plan = test_plan();
    run_apply(&steps, &mut plan).unwrap();

    assert!(
        *applied.borrow(),
        "run_apply must not skip apply() when state() reports Satisfied::No"
    );
}

#[test]
fn t235_no_production_step_needed_to_change_to_gain_state() {
    // "Nothing above changes": every one of the thirteen real
    // production steps compiles and reports the untouched default
    // without editing a single one of them. Calling `state()` (not
    // `apply()`) is pure and safe here -- none of them override it.
    let plan = test_plan();
    for step in steps::default_steps() {
        assert_eq!(
            step.state(&plan),
            Satisfied::Unknown,
            "step '{}' must not need to override state() for this subject's scope",
            step.name()
        );
    }
}

#[test]
fn generate_shared_secret_is_64_hex_characters() {
    let secret = generate_shared_secret();
    assert_eq!(secret.len(), 64);
    assert!(secret.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn generate_shared_secret_is_not_deterministic() {
    assert_ne!(generate_shared_secret(), generate_shared_secret());
}

#[test]
fn human_prerequisites_banner_names_all_three_items() {
    let banner = human_prerequisites_banner();
    assert!(banner.contains("wrangler login"));
    assert!(banner.to_lowercase().contains("oauth"));
    assert!(banner.to_lowercase().contains("issuer"));
}
