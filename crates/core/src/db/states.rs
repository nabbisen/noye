use noye_shared::{TargetState, i64_to_d1};
use worker::*;

/// Result of a state transition
#[derive(Debug, Clone)]
pub struct TransitionResult {
    #[allow(dead_code)]
    pub target_id: String,
    pub previous_status: String,
    pub new_status: String,
    pub changed: bool,
}

/// Inputs to the pure state-transition decision.
///
/// All of the data required to decide whether a target should change state
/// after a single check, with no I/O dependencies, so the decision is unit-
/// testable in isolation from D1.
#[derive(Debug, Clone)]
pub struct TransitionInputs<'a> {
    pub previous_status: &'a str,
    pub consecutive_successes: i64,
    pub consecutive_failures: i64,
    pub success_threshold: i64,
    pub failure_threshold: i64,
}

/// Outputs of the pure state-transition decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionDecision {
    pub new_status: String,
    pub new_consecutive_successes: i64,
    pub new_consecutive_failures: i64,
    pub changed: bool,
}

/// Decide the next state of a target after a single check, given the previous
/// state and counters. This function is pure and side-effect free.
///
/// Rules (per requirement 2-4):
///
/// - On success: increment `consecutive_successes`; reset failures to zero.
/// - On failure: increment `consecutive_failures`; reset successes to zero.
/// - Transition `* -> down` when `consecutive_failures >= failure_threshold`
///   and the previous state was not already `down`.
/// - Transition `down -> up` when `consecutive_successes >= success_threshold`.
/// - Transition `unknown -> up` on the first success (so a brand-new target
///   with no prior history can show as healthy without waiting for the full
///   success threshold).
/// - Otherwise the state is unchanged.
pub fn decide_transition(inputs: TransitionInputs<'_>, is_success: bool) -> TransitionDecision {
    let (new_successes, new_failures) = if is_success {
        (inputs.consecutive_successes + 1, 0_i64)
    } else {
        (0_i64, inputs.consecutive_failures + 1)
    };

    let new_status = if new_failures >= inputs.failure_threshold && inputs.previous_status != "down"
    {
        "down".to_string()
    } else if (new_successes >= inputs.success_threshold && inputs.previous_status == "down")
        || (is_success && inputs.previous_status == "unknown")
    {
        "up".to_string()
    } else {
        inputs.previous_status.to_string()
    };

    let changed = new_status != inputs.previous_status;

    TransitionDecision {
        new_status,
        new_consecutive_successes: new_successes,
        new_consecutive_failures: new_failures,
        changed,
    }
}

pub async fn list_all(db: &D1Database) -> Result<Vec<TargetState>> {
    let results = db
        .prepare("SELECT * FROM target_states")
        .bind(&[])?
        .all()
        .await?;
    results.results::<TargetState>()
}

pub async fn get_by_target(db: &D1Database, target_id: &str) -> Result<TargetState> {
    db.prepare("SELECT * FROM target_states WHERE target_id = ?1")
        .bind(&[target_id.into()])?
        .first::<TargetState>(None)
        .await?
        .ok_or_else(|| Error::RustError(format!("State not found for target: {}", target_id)))
}

/// Update state based on the check result.
/// Increments the consecutive success/failure counter and transitions state once the threshold is met.
pub async fn update_after_check(
    db: &D1Database,
    target_id: &str,
    is_success: bool,
) -> Result<TransitionResult> {
    let state = get_by_target(db, target_id).await?;
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    let inputs = TransitionInputs {
        previous_status: &state.current_status,
        consecutive_successes: state.consecutive_successes,
        consecutive_failures: state.consecutive_failures,
        success_threshold: state.success_threshold,
        failure_threshold: state.failure_threshold,
    };
    let decision = decide_transition(inputs, is_success);

    let status_change_at = if decision.changed {
        now.clone()
    } else {
        state.last_status_change_at.unwrap_or_else(|| now.clone())
    };

    db.prepare(
        "UPDATE target_states SET current_status = ?1, consecutive_successes = ?2,
         consecutive_failures = ?3, last_checked_at = ?4, last_status_change_at = ?5
         WHERE target_id = ?6",
    )
    .bind(&[
        decision.new_status.clone().into(),
        i64_to_d1(decision.new_consecutive_successes).map_err(Error::RustError)?,
        i64_to_d1(decision.new_consecutive_failures).map_err(Error::RustError)?,
        now.into(),
        status_change_at.into(),
        target_id.into(),
    ])?
    .run()
    .await?;

    Ok(TransitionResult {
        target_id: target_id.to_string(),
        previous_status: state.current_status,
        new_status: decision.new_status,
        changed: decision.changed,
    })
}

pub async fn mark_notified(db: &D1Database, target_id: &str) -> Result<()> {
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    db.prepare("UPDATE target_states SET last_notification_at = ?1 WHERE target_id = ?2")
        .bind(&[now.into(), target_id.into()])?
        .run()
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_inputs() -> TransitionInputs<'static> {
        TransitionInputs {
            previous_status: "up",
            consecutive_successes: 0,
            consecutive_failures: 0,
            success_threshold: 3,
            failure_threshold: 3,
        }
    }

    #[test]
    fn first_success_from_unknown_transitions_to_up() {
        let inputs = TransitionInputs {
            previous_status: "unknown",
            ..base_inputs()
        };
        let decision = decide_transition(inputs, true);
        assert_eq!(decision.new_status, "up");
        assert_eq!(decision.new_consecutive_successes, 1);
        assert_eq!(decision.new_consecutive_failures, 0);
        assert!(decision.changed);
    }

    #[test]
    fn single_failure_below_threshold_stays_up() {
        let inputs = TransitionInputs {
            previous_status: "up",
            consecutive_failures: 0,
            failure_threshold: 3,
            ..base_inputs()
        };
        let decision = decide_transition(inputs, false);
        assert_eq!(decision.new_status, "up");
        assert_eq!(decision.new_consecutive_failures, 1);
        assert!(!decision.changed);
    }

    #[test]
    fn failures_at_threshold_transition_to_down() {
        let inputs = TransitionInputs {
            previous_status: "up",
            consecutive_failures: 2, // about to become 3
            failure_threshold: 3,
            ..base_inputs()
        };
        let decision = decide_transition(inputs, false);
        assert_eq!(decision.new_status, "down");
        assert_eq!(decision.new_consecutive_failures, 3);
        assert!(decision.changed);
    }

    #[test]
    fn failures_already_down_stay_down() {
        let inputs = TransitionInputs {
            previous_status: "down",
            consecutive_failures: 5,
            failure_threshold: 3,
            ..base_inputs()
        };
        let decision = decide_transition(inputs, false);
        assert_eq!(decision.new_status, "down");
        assert_eq!(decision.new_consecutive_failures, 6);
        assert!(!decision.changed);
    }

    #[test]
    fn success_resets_failure_counter() {
        let inputs = TransitionInputs {
            previous_status: "up",
            consecutive_failures: 2,
            ..base_inputs()
        };
        let decision = decide_transition(inputs, true);
        assert_eq!(decision.new_consecutive_failures, 0);
        assert_eq!(decision.new_consecutive_successes, 1);
    }

    #[test]
    fn failure_resets_success_counter() {
        let inputs = TransitionInputs {
            previous_status: "up",
            consecutive_successes: 5,
            ..base_inputs()
        };
        let decision = decide_transition(inputs, false);
        assert_eq!(decision.new_consecutive_successes, 0);
        assert_eq!(decision.new_consecutive_failures, 1);
    }

    #[test]
    fn down_recovers_only_after_success_threshold() {
        let mut state = TransitionInputs {
            previous_status: "down",
            consecutive_successes: 0,
            consecutive_failures: 5,
            success_threshold: 3,
            failure_threshold: 3,
        };

        // First success: still down (1 < 3)
        let decision = decide_transition(state.clone(), true);
        assert_eq!(decision.new_status, "down");
        state.consecutive_successes = decision.new_consecutive_successes;
        state.consecutive_failures = decision.new_consecutive_failures;

        // Second success: still down (2 < 3)
        let decision = decide_transition(state.clone(), true);
        assert_eq!(decision.new_status, "down");
        state.consecutive_successes = decision.new_consecutive_successes;
        state.consecutive_failures = decision.new_consecutive_failures;

        // Third success: transition to up (3 >= 3)
        let decision = decide_transition(state, true);
        assert_eq!(decision.new_status, "up");
        assert!(decision.changed);
    }

    #[test]
    fn down_with_one_failure_in_between_resets_recovery_progress() {
        let inputs = TransitionInputs {
            previous_status: "down",
            consecutive_successes: 2, // close to recovery
            consecutive_failures: 0,
            success_threshold: 3,
            failure_threshold: 3,
        };
        // A new failure resets the success counter and pins us at down.
        let decision = decide_transition(inputs, false);
        assert_eq!(decision.new_status, "down");
        assert_eq!(decision.new_consecutive_successes, 0);
        assert_eq!(decision.new_consecutive_failures, 1);
        assert!(!decision.changed);
    }

    #[test]
    fn unknown_to_down_takes_full_failure_threshold() {
        let mut state = TransitionInputs {
            previous_status: "unknown",
            consecutive_successes: 0,
            consecutive_failures: 0,
            success_threshold: 3,
            failure_threshold: 3,
        };
        // Two failures: still unknown
        for _ in 0..2 {
            let decision = decide_transition(state.clone(), false);
            assert_eq!(decision.new_status, "unknown");
            state.consecutive_failures = decision.new_consecutive_failures;
        }
        // Third failure: transition to down
        let decision = decide_transition(state, false);
        assert_eq!(decision.new_status, "down");
        assert!(decision.changed);
    }

    #[test]
    fn aggressive_threshold_of_one_triggers_immediately() {
        // Threshold = 1 means "fail-fast"
        let inputs = TransitionInputs {
            previous_status: "up",
            consecutive_successes: 0,
            consecutive_failures: 0,
            success_threshold: 1,
            failure_threshold: 1,
        };
        let decision = decide_transition(inputs, false);
        assert_eq!(decision.new_status, "down");
        assert!(decision.changed);
    }
}
