use worker::*;

use crate::db;
use noye_shared::{CheckResult, Target};

/// Whether this scheduled invocation should run the retention pass,
/// decided from the invocation's own nominal schedule -- never the wall
/// clock. Subject 07g (G-43): `chrono::Utc::now()` reads wall-clock time,
/// but Cloudflare's cron triggers are best-effort, not exact -- an
/// invocation nominally scheduled for 00:00 that actually starts at
/// 00:01 read wall-clock "01" and silently skipped the hour, with
/// nothing logged either way.
///
/// Pure, and deliberately so: `wrangler dev --local`'s scheduled-event
/// simulation does not propagate a controllable nominal time through
/// `event.schedule()` (confirmed against real `workerd`; see
/// `.git-exclude/reviewed/058-subject-07g-escalation-ruling.md`), so
/// this is the only way T-223/T-224 can exercise the decision without a
/// live Worker runtime -- the same reason `decide_transition`,
/// `compute_cutoff` and `eligibility_where_clause` are pure.
#[derive(Debug, Clone, PartialEq, Eq)]
enum RetentionTrigger {
    Run,
    Skip { minute: String },
    UnreadableSchedule,
}

fn retention_trigger(scheduled_ms: f64) -> RetentionTrigger {
    // NaN/infinite must not reach `as i64`: `f64::NAN as i64` saturates
    // to 0 (1970-01-01T00:00:00Z, minute "00"), which would silently
    // run retention on an unreadable schedule -- the same shape subject
    // 07b's ruling required `bool_from_d1`'s `visit_f64` to guard
    // against (`NaN != 0.0` reading as `true`). Identical defect,
    // caught the same way: reject before the cast, don't let it land
    // on a value that happens to look valid.
    if !scheduled_ms.is_finite() {
        return RetentionTrigger::UnreadableSchedule;
    }
    match chrono::DateTime::from_timestamp_millis(scheduled_ms as i64) {
        Some(t) => {
            let minute = t.format("%M").to_string();
            if minute == "00" {
                RetentionTrigger::Run
            } else {
                RetentionTrigger::Skip { minute }
            }
        }
        None => RetentionTrigger::UnreadableSchedule,
    }
}

/// Top-level scheduler invoked from the Cron Trigger
///
/// Requirement 2-4: a single scheduler batches every target whose next-check time has arrived
pub async fn run_scheduled_checks(env: &Env, event: &ScheduledEvent) -> Result<()> {
    let db_conn = env.d1("DB")?;
    let now = chrono::Utc::now();
    let now_str = now.format("%Y-%m-%dT%H:%M:%SZ").to_string();

    // 1. Pull every active target whose next-check time has arrived
    let targets = db_conn
        .prepare(format!(
            "SELECT {} FROM targets
             WHERE is_disabled = 0 AND next_check_at <= ?1
             ORDER BY next_check_at ASC
             LIMIT 50",
            crate::db::targets::TARGET_COLUMNS
        ))
        .bind(&[now_str.clone().into()])?
        .all()
        .await?
        .results::<Target>()?;

    if targets.is_empty() {
        return Ok(());
    }

    console_log!("Processing {} targets for scheduled check", targets.len());

    // 2. Run the health check for each target
    for target in &targets {
        let outcome = execute_check(env, target).await;

        // 3. Record the check result in D1
        let result_id = uuid::Uuid::new_v4().to_string();
        let check_result = CheckResult {
            id: result_id,
            target_id: target.id.clone(),
            checked_at: now_str.clone(),
            is_success: outcome.is_success,
            status_code: outcome.status_code,
            response_time_ms: Some(outcome.response_time_ms),
            error_message: outcome.error_message.clone(),
            tls_expiry_date: outcome.tls_expiry_date.clone(),
            tls_days_left: outcome.tls_days_left,
            details: outcome.details.clone(),
        };
        if let Err(e) = db::results::insert(&db_conn, &check_result).await {
            console_error!("Failed to insert check result for {}: {:?}", target.id, e);
        }

        // 4. Update target_states (evaluate consecutive successes/failures and transition state)
        match db::states::update_after_check(
            &db_conn,
            &target.id,
            target.success_threshold,
            target.failure_threshold,
            outcome.is_success,
        )
        .await
        {
            Ok(transition) => {
                if transition.changed {
                    handle_state_transition(env, target, &transition, &outcome).await;
                }
            }
            Err(e) => {
                console_error!("Failed to update state for {}: {:?}", target.id, e);
            }
        }

        // 5. Update next_check_at to the next scheduled execution time
        let next_check = now + chrono::Duration::minutes(target.interval_minutes);
        let next_str = next_check.format("%Y-%m-%dT%H:%M:%SZ").to_string();
        if let Ok(stmt) = db_conn
            .prepare("UPDATE targets SET next_check_at = ?1 WHERE id = ?2")
            .bind(&[next_str.into(), target.id.clone().into()])
        {
            let _ = stmt.run().await;
        }
    }

    // 6. Data lifecycle: periodic cleanup (runs at minute 0 of every
    // hour, decided from the invocation's own nominal schedule -- see
    // `retention_trigger` above, subject 07g / G-43). This step is the
    // only one of the six that answers "is this the invocation that
    // should do X?"; the other five stamp *when a check actually ran*,
    // which is correctly `now` and must stay that way (T-225) -- a
    // check result stamped with its scheduled time rather than its
    // actual time would misreport when a probe happened.
    match retention_trigger(event.schedule()) {
        RetentionTrigger::Run => {
            if let Err(e) = db::retention::run_cleanup(env).await {
                console_error!("Retention cleanup error: {:?}", e);
            }
        }
        RetentionTrigger::Skip { minute } => {
            console_log!(
                "Retention skipped this invocation: nominal schedule was minute {}, not 00",
                minute
            );
        }
        RetentionTrigger::UnreadableSchedule => {
            console_error!(
                "Retention skipped: could not interpret scheduled event time {} as a timestamp",
                event.schedule()
            );
        }
    }

    Ok(())
}

/// Run the protocol-specific health check for the target
async fn execute_check(env: &Env, target: &Target) -> super::CheckOutcome {
    let start = js_sys::Date::now() as i64;

    // Retrying check
    let mut last_outcome = None;
    for attempt in 0..=target.retry_count {
        if attempt > 0 {
            console_log!(
                "Retry {}/{} for target {}",
                attempt,
                target.retry_count,
                target.id
            );
        }

        let outcome = match target.target_type.as_str() {
            "http" | "https" => super::http::check(env, target).await,
            "tcp" => super::tcp::check(env, target).await,
            "smtp" => super::smtp::check(env, target).await,
            "tls" => super::tls::check_certificate(env, target).await,
            other => super::CheckOutcome::failure(format!("Unsupported protocol: {}", other), 0),
        };

        if outcome.is_success {
            return outcome;
        }
        last_outcome = Some(outcome);
    }

    last_outcome.unwrap_or_else(|| {
        let elapsed = (js_sys::Date::now() as i64) - start;
        super::CheckOutcome::failure("All retries exhausted".to_string(), elapsed)
    })
}

/// Behavior when a state transition occurs (down/recovery)
async fn handle_state_transition(
    env: &Env,
    target: &Target,
    transition: &db::states::TransitionResult,
    outcome: &super::CheckOutcome,
) {
    let db_conn = match env.d1("DB") {
        Ok(db) => db,
        Err(_) => return,
    };

    console_log!(
        "State transition for {}: {} -> {}",
        target.id,
        transition.previous_status,
        transition.new_status
    );

    // Maintenance-window check (used to suppress notifications)
    let under_maintenance = db::maintenance::is_under_maintenance(&db_conn, &target.id)
        .await
        .unwrap_or(false);

    match transition.new_status.as_str() {
        "down" => {
            // Create the incident
            let cause = outcome
                .error_message
                .clone()
                .unwrap_or_else(|| "Unknown failure".to_string());
            if let Err(e) = db::incidents::open(&db_conn, &target.id, &cause).await {
                console_error!("Failed to open incident: {:?}", e);
            }

            // Notification (suppressed during maintenance windows)
            if !under_maintenance {
                crate::notify::dispatch_down(env, target, outcome).await;
                let _ = db::states::mark_notified(&db_conn, &target.id).await;
            }

            // audit log
            db::audit::log_system_or_report(
                &db_conn,
                "target",
                &target.id,
                "status_down",
                outcome.error_message.as_deref(),
            )
            .await;
        }
        "up" if transition.previous_status == "down" => {
            // Auto-resolve the incident
            if let Err(e) = db::incidents::auto_resolve(&db_conn, &target.id).await {
                console_error!("Failed to auto-resolve incident: {:?}", e);
            }

            // recovery notification
            if !under_maintenance {
                crate::notify::dispatch_up(env, target).await;
                let _ = db::states::mark_notified(&db_conn, &target.id).await;
            }

            db::audit::log_system_or_report(
                &db_conn,
                "target",
                &target.id,
                "status_up",
                Some("Auto-recovered"),
            )
            .await;
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests;
