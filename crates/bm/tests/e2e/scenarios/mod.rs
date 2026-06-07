pub mod manifest_journey;
pub mod operator_journey;
pub mod rc_operator_journey;
pub mod session_lifecycle_journey;
pub mod tg_operator_journey;

use super::helpers::{E2eConfig, ProgressiveMode};
use libtest_mimic::Trial;

const ALL_SUITES: &[&str] = &[
    "scenario_manifest_journey",
    "scenario_operator_journey",
    "scenario_rc_operator_journey",
    "scenario_tg_operator_journey",
    "scenario_session_lifecycle_journey",
];

pub fn tests(config: &E2eConfig) -> Vec<Trial> {
    match &config.progressive {
        None => {
            vec![
                manifest_journey::scenario(config),
                operator_journey::scenario(config),
                rc_operator_journey::scenario(config),
                tg_operator_journey::scenario(config),
                session_lifecycle_journey::scenario(config),
            ]
        }
        Some(ProgressiveMode::Step(suite_filter)) => {
            let suites = targeted_suites(suite_filter);
            suites
                .into_iter()
                .filter_map(|name| build_progressive_suite(name, config))
                .collect()
        }
        Some(ProgressiveMode::Reset(_)) => Vec::new(),
    }
}

fn targeted_suites(filter: &Option<String>) -> Vec<&'static str> {
    match filter {
        Some(name) => {
            match ALL_SUITES.iter().find(|s| **s == name.as_str()) {
                Some(s) => vec![s],
                None => {
                    eprintln!("Unknown suite '{}'. Available: {}", name, ALL_SUITES.join(", "));
                    std::process::exit(1);
                }
            }
        }
        None => ALL_SUITES.to_vec(),
    }
}

fn build_progressive_suite(name: &str, config: &E2eConfig) -> Option<Trial> {
    if config
        .progressive
        .as_ref()
        .is_some_and(|p| matches!(p, ProgressiveMode::Step(None)))
    {
        if let Some(state) = super::helpers::ProgressState::load(name) {
            if state.next_case >= state.total_cases {
                eprintln!("  [{}] already complete, skipping", name);
                return None;
            }
        }
    }

    let trial = match name {
        "scenario_manifest_journey" => manifest_journey::scenario_progressive(config),
        "scenario_operator_journey" => operator_journey::scenario_progressive(config),
        "scenario_rc_operator_journey" => rc_operator_journey::scenario_progressive(config),
        "scenario_tg_operator_journey" => tg_operator_journey::scenario_progressive(config),
        "scenario_session_lifecycle_journey" => {
            session_lifecycle_journey::scenario_progressive(config)
        }
        _ => unreachable!(),
    };
    Some(trial)
}
