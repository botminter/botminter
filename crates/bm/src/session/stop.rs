use anyhow::Result;

use crate::session::finalization::deactivation;
use crate::session::registry::SessionRegistry;
use crate::session::types::{SessionId, SessionState, SessionType};

#[cfg(unix)]
fn send_signal(pid: u32, force: bool) {
    let sig = if force { libc::SIGKILL } else { libc::SIGTERM };
    unsafe { libc::kill(pid as libc::pid_t, sig) };
}

/// How sessions are selected for stopping.
#[derive(Debug)]
pub enum StopMode {
    AllForMember(String),
    SpecificSession(SessionId),
    AutonomousOnly,
}

/// Options for the stop operation.
#[derive(Debug)]
pub struct StopOptions {
    pub mode: StopMode,
    pub force: bool,
}

/// Summary of a stop operation.
#[derive(Debug, Default)]
pub struct StopSummary {
    pub deactivated: usize,
    pub killed: usize,
    pub skipped_interactive: usize,
    pub errors: Vec<String>,
}

/// Execute a stop operation against sessions in the registry.
pub fn stop_sessions(
    registry: &mut SessionRegistry,
    options: &StopOptions,
) -> StopSummary {
    let mut summary = StopSummary::default();

    let snapshot: Vec<_> = registry
        .list()
        .into_iter()
        .map(|r| {
            (
                r.session_id.clone(),
                r.member_name.clone(),
                r.session_type.clone(),
                r.current_state.clone(),
                r.agent_pid,
                r.finalization_agent_pid,
            )
        })
        .collect();

    for (id, member_name, session_type, current_state, agent_pid, finalization_agent_pid) in &snapshot {
        let should_process = match &options.mode {
            StopMode::AllForMember(member) => member_name == member,
            StopMode::SpecificSession(target_id) => id == target_id,
            StopMode::AutonomousOnly => {
                if *session_type == SessionType::Interactive {
                    if *current_state == SessionState::Active {
                        summary.skipped_interactive += 1;
                    }
                    false
                } else {
                    true
                }
            }
        };

        if !should_process {
            continue;
        }

        if options.force {
            match current_state {
                SessionState::Active => {
                    if let Some(pid) = agent_pid {
                        #[cfg(unix)]
                        send_signal(*pid, true);
                    }
                    if registry.update_state(id, SessionState::Killed).is_ok() {
                        summary.killed += 1;
                    }
                }
                SessionState::Finalizing => {
                    // Update state BEFORE sending SIGKILL so the deactivation watcher always
                    // sees Retained when it detects the dead process (signal arrives before
                    // state update in the opposite order, creating a race).
                    if registry.update_state(id, SessionState::Killed).is_ok() {
                        let _ = registry.update_state(id, SessionState::Retained);
                        summary.killed += 1;
                    }
                    if let Some(pid) = agent_pid {
                        #[cfg(unix)]
                        send_signal(*pid, true);
                    }
                    // Kill the finalization subagent. The brain PID above is typically already
                    // dead and may be recycled; this is the live process consuming resources.
                    if let Some(fin_pid) = finalization_agent_pid {
                        #[cfg(unix)]
                        send_signal(*fin_pid, true);
                    }
                }
                _ => {}
            }
        } else if *current_state == SessionState::Active {
            if let Some(pid) = agent_pid {
                #[cfg(unix)]
                send_signal(*pid, false);
            }
            if registry.update_state(id, SessionState::Finalizing).is_ok() {
                summary.deactivated += 1;
            }
        }
    }

    summary
}

/// Re-trigger finalization on a retained session.
///
/// Returns the spawned child on success so callers can attach a watcher.
/// Spawn failures are logged and returned as `None` — the session is already
/// in `Finalizing` state when this returns, so the caller can still respond OK.
pub fn retrigger_session_finalization(
    registry: &mut SessionRegistry,
    session_id: &SessionId,
) -> Result<(StopSummary, Option<std::process::Child>)> {
    let workspace_path = registry
        .get(session_id)
        .and_then(|r| r.workspace_path.clone());
    registry.update_state(session_id, SessionState::Finalizing)?;
    let child = if let Some(ref wp) = workspace_path {
        match deactivation::retrigger_finalization(session_id, wp) {
            Ok(child) => Some(child),
            Err(e) => {
                tracing::error!(
                    session_id = session_id.as_str(),
                    "failed to spawn finalization subagent: {}",
                    e
                );
                None
            }
        }
    } else {
        None
    };
    Ok((StopSummary::default(), child))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use crate::session::registry::SessionRegistry;
    use crate::session::types::{SessionId, SessionRecord, SessionState, SessionType};

    fn new_registry() -> SessionRegistry {
        let tmp = tempfile::tempdir().unwrap();
        SessionRegistry::new(tmp.path().join("registry.json"))
    }

    fn register_active(
        registry: &mut SessionRegistry,
        member: &str,
        session_type: SessionType,
    ) -> SessionId {
        let id = SessionId::new();
        let now = chrono::Utc::now();
        let record = SessionRecord {
            session_id: id.clone(),
            member_name: member.to_string(),
            session_type,
            current_state: SessionState::Creating,
            created_at: now,
            state_transitioned_at: now,
            agent_pid: None,
            workspace_path: Some(PathBuf::from("/tmp/ws")),
            finalization_result: None,
                finalization_agent_pid: None,
        };
        registry.register(record).unwrap();
        registry
            .update_state(&id, SessionState::Active)
            .unwrap();
        id
    }

    fn register_finalizing(
        registry: &mut SessionRegistry,
        member: &str,
        session_type: SessionType,
    ) -> SessionId {
        let id = register_active(registry, member, session_type);
        registry
            .update_state(&id, SessionState::Finalizing)
            .unwrap();
        id
    }

    fn register_retained(
        registry: &mut SessionRegistry,
        member: &str,
        session_type: SessionType,
    ) -> SessionId {
        let id = register_active(registry, member, session_type);
        registry
            .update_state(&id, SessionState::Killed)
            .unwrap();
        registry
            .update_state(&id, SessionState::Retained)
            .unwrap();
        id
    }

    // ---
    // AC-1: Member stop with finalization → CLI summary
    // ---

    #[test]
    fn member_stop_deactivates_all_member_sessions() {
        let mut reg = new_registry();
        let _id1 = register_active(&mut reg, "alice", SessionType::Loop);
        let _id2 = register_active(&mut reg, "alice", SessionType::Brain);

        let summary = stop_sessions(
            &mut reg,
            &StopOptions {
                mode: StopMode::AllForMember("alice".to_string()),
                force: false,
            },
        );

        assert!(
            summary.deactivated >= 2,
            "member stop must deactivate all sessions for the member, got {}",
            summary.deactivated,
        );
    }

    #[test]
    fn member_stop_transitions_sessions_out_of_active() {
        let mut reg = new_registry();
        let id1 = register_active(&mut reg, "alice", SessionType::Loop);
        let id2 = register_active(&mut reg, "alice", SessionType::Brain);

        stop_sessions(
            &mut reg,
            &StopOptions {
                mode: StopMode::AllForMember("alice".to_string()),
                force: false,
            },
        );

        let state1 = &reg.get(&id1).unwrap().current_state;
        let state2 = &reg.get(&id2).unwrap().current_state;
        assert_ne!(
            *state1,
            SessionState::Active,
            "session must not remain Active after member stop"
        );
        assert_ne!(
            *state2,
            SessionState::Active,
            "session must not remain Active after member stop"
        );
    }

    #[test]
    fn member_stop_does_not_affect_other_members() {
        let mut reg = new_registry();
        let _alice = register_active(&mut reg, "alice", SessionType::Loop);
        let bob_id = register_active(&mut reg, "bob", SessionType::Loop);

        stop_sessions(
            &mut reg,
            &StopOptions {
                mode: StopMode::AllForMember("alice".to_string()),
                force: false,
            },
        );

        let bob_state = &reg.get(&bob_id).unwrap().current_state;
        assert_eq!(
            *bob_state,
            SessionState::Active,
            "other members' sessions must not be affected"
        );
    }

    // ---
    // AC-2: Session-specific stop
    // ---

    #[test]
    fn session_specific_stop_deactivates_target() {
        let mut reg = new_registry();
        let target = register_active(&mut reg, "alice", SessionType::Loop);

        let summary = stop_sessions(
            &mut reg,
            &StopOptions {
                mode: StopMode::SpecificSession(target.clone()),
                force: false,
            },
        );

        let state = &reg.get(&target).unwrap().current_state;
        assert_ne!(
            *state,
            SessionState::Active,
            "target session must be deactivated"
        );
        assert!(
            summary.deactivated >= 1,
            "summary must report at least 1 deactivated session"
        );
    }

    #[test]
    fn session_specific_stop_leaves_others_active() {
        let mut reg = new_registry();
        let target = register_active(&mut reg, "alice", SessionType::Loop);
        let other = register_active(&mut reg, "alice", SessionType::Brain);

        stop_sessions(
            &mut reg,
            &StopOptions {
                mode: StopMode::SpecificSession(target.clone()),
                force: false,
            },
        );

        let other_state = &reg.get(&other).unwrap().current_state;
        assert_eq!(
            *other_state,
            SessionState::Active,
            "non-targeted session must remain Active"
        );
    }

    // ---
    // AC-3: Bare stop skips interactive with explanation
    // ---

    #[test]
    fn bare_stop_deactivates_loop_sessions() {
        let mut reg = new_registry();
        let loop_id = register_active(&mut reg, "alice", SessionType::Loop);

        stop_sessions(
            &mut reg,
            &StopOptions {
                mode: StopMode::AutonomousOnly,
                force: false,
            },
        );

        let state = &reg.get(&loop_id).unwrap().current_state;
        assert_ne!(
            *state,
            SessionState::Active,
            "Loop session must be deactivated by bare stop"
        );
    }

    #[test]
    fn bare_stop_deactivates_brain_sessions() {
        let mut reg = new_registry();
        let brain_id = register_active(&mut reg, "alice", SessionType::Brain);

        stop_sessions(
            &mut reg,
            &StopOptions {
                mode: StopMode::AutonomousOnly,
                force: false,
            },
        );

        let state = &reg.get(&brain_id).unwrap().current_state;
        assert_ne!(
            *state,
            SessionState::Active,
            "Brain session must be deactivated by bare stop"
        );
    }

    #[test]
    fn bare_stop_skips_interactive_sessions() {
        let mut reg = new_registry();
        let interactive_id = register_active(&mut reg, "alice", SessionType::Interactive);
        let _loop_id = register_active(&mut reg, "alice", SessionType::Loop);

        stop_sessions(
            &mut reg,
            &StopOptions {
                mode: StopMode::AutonomousOnly,
                force: false,
            },
        );

        let state = &reg.get(&interactive_id).unwrap().current_state;
        assert_eq!(
            *state,
            SessionState::Active,
            "Interactive session must remain Active during bare stop"
        );
    }

    #[test]
    fn bare_stop_reports_skipped_interactive_count() {
        let mut reg = new_registry();
        let _interactive1 = register_active(&mut reg, "alice", SessionType::Interactive);
        let _interactive2 = register_active(&mut reg, "bob", SessionType::Interactive);
        let _loop1 = register_active(&mut reg, "alice", SessionType::Loop);

        let summary = stop_sessions(
            &mut reg,
            &StopOptions {
                mode: StopMode::AutonomousOnly,
                force: false,
            },
        );

        assert_eq!(
            summary.skipped_interactive, 2,
            "bare stop must report 2 skipped interactive sessions, got {}",
            summary.skipped_interactive,
        );
    }

    // ---
    // AC-4: Force stop active → Killed, no finalization
    // ---

    #[test]
    fn force_stop_active_transitions_to_killed() {
        let mut reg = new_registry();
        let id = register_active(&mut reg, "alice", SessionType::Loop);

        let summary = stop_sessions(
            &mut reg,
            &StopOptions {
                mode: StopMode::AllForMember("alice".to_string()),
                force: true,
            },
        );

        let state = &reg.get(&id).unwrap().current_state;
        assert_eq!(
            *state,
            SessionState::Killed,
            "force stop on Active must transition to Killed, got {state}"
        );
        assert!(
            summary.killed >= 1,
            "summary must report at least 1 killed session"
        );
    }

    #[test]
    fn force_stop_active_does_not_enter_finalizing() {
        let mut reg = new_registry();
        let id = register_active(&mut reg, "alice", SessionType::Loop);

        stop_sessions(
            &mut reg,
            &StopOptions {
                mode: StopMode::AllForMember("alice".to_string()),
                force: true,
            },
        );

        let state = &reg.get(&id).unwrap().current_state;
        assert_ne!(
            *state,
            SessionState::Finalizing,
            "force stop must skip finalization — session must not be in Finalizing state"
        );
    }

    // ---
    // AC-5: Force stop finalizing → Killed, retained
    // ---

    #[test]
    fn force_stop_finalizing_transitions_to_killed() {
        let mut reg = new_registry();
        let id = register_finalizing(&mut reg, "alice", SessionType::Loop);

        let _summary = stop_sessions(
            &mut reg,
            &StopOptions {
                mode: StopMode::AllForMember("alice".to_string()),
                force: true,
            },
        );

        let state = &reg.get(&id).unwrap().current_state;
        assert_eq!(
            *state,
            SessionState::Retained,
            "force stop on Finalizing must transition to Retained (enabling retrigger), got {state}"
        );
    }

    #[test]
    fn force_stop_mixed_states_handles_each_independently() {
        let mut reg = new_registry();
        let active_id = register_active(&mut reg, "alice", SessionType::Loop);
        let finalizing_id = register_finalizing(&mut reg, "alice", SessionType::Brain);

        let summary = stop_sessions(
            &mut reg,
            &StopOptions {
                mode: StopMode::AllForMember("alice".to_string()),
                force: true,
            },
        );

        let active_state = &reg.get(&active_id).unwrap().current_state;
        let finalizing_state = &reg.get(&finalizing_id).unwrap().current_state;

        assert_eq!(
            *active_state,
            SessionState::Killed,
            "Active session must be Killed, got {active_state}"
        );
        assert_eq!(
            *finalizing_state,
            SessionState::Retained,
            "Finalizing session must be Retained after force stop, got {finalizing_state}"
        );
        assert!(
            summary.killed >= 2,
            "summary must report 2 killed sessions, got {}",
            summary.killed,
        );
    }

    // ---
    // AC-6: Re-trigger after force stop → fresh subagent
    // ---

    #[test]
    fn retrigger_on_retained_session_succeeds() {
        let mut reg = new_registry();
        let id = register_retained(&mut reg, "alice", SessionType::Loop);

        let result = retrigger_session_finalization(&mut reg, &id);

        assert!(
            result.is_ok(),
            "retrigger on retained session must succeed, got: {:?}",
            result.err(),
        );
    }

    #[test]
    fn retrigger_transitions_retained_to_finalizing() {
        let mut reg = new_registry();
        let id = register_retained(&mut reg, "alice", SessionType::Loop);

        let _result = retrigger_session_finalization(&mut reg, &id);

        let state = &reg.get(&id).unwrap().current_state;
        assert_eq!(
            *state,
            SessionState::Finalizing,
            "retrigger must transition Retained → Finalizing, got {state}"
        );
    }

    // ---
    // AC-5 (bugfix): Force stop Finalizing → Killed → Retained (not terminal Killed)
    // ---

    #[test]
    fn force_stop_finalizing_then_retrigger_is_possible() {
        let mut reg = new_registry();
        let id = register_finalizing(&mut reg, "alice", SessionType::Loop);
        stop_sessions(
            &mut reg,
            &StopOptions {
                mode: StopMode::AllForMember("alice".to_string()),
                force: true,
            },
        );
        // After force stop on Finalizing, session must be Retained (not terminal Killed)
        let state = &reg.get(&id).unwrap().current_state;
        assert_eq!(
            *state,
            SessionState::Retained,
            "force stop on Finalizing must go to Retained for retrigger to be possible, got {state}"
        );
        // Verify retrigger is valid (Retained → Finalizing)
        let retrigger_result = retrigger_session_finalization(&mut reg, &id);
        assert!(
            retrigger_result.is_ok(),
            "retrigger must succeed after force stop: {:?}",
            retrigger_result.err()
        );
        assert_eq!(
            reg.get(&id).unwrap().current_state,
            SessionState::Finalizing,
            "retrigger must transition Retained → Finalizing"
        );
    }

    #[test]
    fn force_stop_finalizing_produces_retained_state_not_killed() {
        let mut reg = new_registry();
        let id = register_finalizing(&mut reg, "bob", SessionType::Loop);
        let summary = stop_sessions(
            &mut reg,
            &StopOptions {
                mode: StopMode::AllForMember("bob".to_string()),
                force: true,
            },
        );
        assert!(
            summary.killed >= 1,
            "force stop must report killed, got {}",
            summary.killed
        );
        // State must be Retained, not Killed (Killed is terminal, Retained allows retrigger)
        assert_eq!(
            reg.get(&id).unwrap().current_state,
            SessionState::Retained,
            "force stop on Finalizing must result in Retained state (Killed is terminal, prevents retrigger)"
        );
    }

    #[test]
    #[cfg(unix)]
    fn force_stop_finalizing_kills_finalization_agent_process() {
        let mut reg = new_registry();
        let id = register_finalizing(&mut reg, "alice", SessionType::Loop);

        // Spawn a real process as the simulated finalization agent.
        let mut child = std::process::Command::new("sleep")
            .arg("9999")
            .spawn()
            .expect("failed to spawn sleep");
        let fin_pid = child.id();

        // Record the finalization agent PID — this is what force-stop must kill.
        reg.set_finalization_agent_pid(&id, fin_pid).unwrap();

        stop_sessions(
            &mut reg,
            &StopOptions {
                mode: StopMode::AllForMember("alice".to_string()),
                force: true,
            },
        );

        // Give the OS a moment to deliver SIGKILL.
        std::thread::sleep(std::time::Duration::from_millis(100));

        let status = child.try_wait().expect("try_wait failed");
        assert!(
            status.is_some(),
            "finalization agent (PID {fin_pid}) must be dead after force-stop on Finalizing"
        );
    }
}
