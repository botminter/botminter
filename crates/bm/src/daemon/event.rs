use std::process::Command;

use anyhow::{bail, Context, Result};
use serde::Deserialize;

use crate::config;

use super::config::PollState;

/// GitHub event types that trigger member launches.
const RELEVANT_EVENTS: &[&str] = &["issues", "issue_comment", "pull_request"];

/// A GitHub event from the events API.
#[derive(Debug, Deserialize)]
pub struct GitHubEvent {
    pub id: String,
    #[serde(rename = "type")]
    pub event_type: String,
}

/// Checks if an event type is relevant for triggering member launches.
pub fn is_relevant_event(event_type: &str) -> bool {
    // The events API uses PascalCase type names, webhook headers use snake_case
    let normalized = event_type.to_lowercase();
    RELEVANT_EVENTS.iter().any(|&re| {
        normalized == re
            || normalized == re.replace('_', "")
            // Events API format: IssuesEvent, IssueCommentEvent, PullRequestEvent
            || normalized == format!("{}event", re.replace('_', ""))
    })
}

/// Polls the GitHub events API for new events.
pub fn poll_github_events(
    github_repo: &str,
    poll_state: &PollState,
) -> Result<Vec<GitHubEvent>> {
    tracing::debug!(
        repo = %github_repo,
        last_event_id = poll_state.last_event_id.as_deref().unwrap_or("none"),
        "Polling GitHub events"
    );

    let output = Command::new("gh")
        .args([
            "api",
            &format!("repos/{}/events", github_repo),
            "--paginate",
            "--jq",
            "[.[] | {id: .id, type: .type}]",
        ])
        .output()
        .context("Failed to run gh api command")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("gh api failed: {}", stderr.trim());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.trim().is_empty() {
        tracing::debug!("GitHub events API returned empty response");
        return Ok(Vec::new());
    }

    let events: Vec<GitHubEvent> =
        serde_json::from_str(&stdout).context("Failed to parse GitHub events response")?;

    // Filter to events newer than last_event_id
    let result = if let Some(ref last_id) = poll_state.last_event_id {
        let new_events: Vec<GitHubEvent> = events
            .into_iter()
            .take_while(|e| &e.id != last_id)
            .collect();
        new_events
    } else {
        events
    };

    tracing::debug!(total = result.len(), "GitHub events fetched");
    for event in &result {
        tracing::trace!(id = %event.id, event_type = %event.event_type, "Event");
    }

    Ok(result)
}

/// Resolves the GitHub repo (owner/name) for a team.
pub fn resolve_github_repo(team_name: &str) -> Result<String> {
    let cfg = config::load()?;
    let team = config::resolve_team(&cfg, Some(team_name))?;
    if team.github_repo.is_empty() {
        bail!("No GitHub repo configured for team '{}'", team_name);
    }
    tracing::debug!(team = %team_name, repo = %team.github_repo, "Resolved GitHub repo");
    Ok(team.github_repo.clone())
}

/// Validates a GitHub webhook signature using HMAC-SHA256.
pub fn validate_webhook_signature(
    secret: &str,
    body: &str,
    signature_header: Option<&str>,
) -> bool {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    let sig = match signature_header {
        Some(s) => s,
        None => return false,
    };

    // GitHub sends "sha256=<hex>"
    let hex_sig = match sig.strip_prefix("sha256=") {
        Some(h) => h,
        None => return false,
    };

    let expected = match hex::decode(hex_sig) {
        Ok(b) => b,
        Err(_) => return false,
    };

    let mut mac = match Hmac::<Sha256>::new_from_slice(secret.as_bytes()) {
        Ok(m) => m,
        Err(_) => return false,
    };
    mac.update(body.as_bytes());

    mac.verify_slice(&expected).is_ok()
}

/// Loads the webhook secret from the team's credentials.
pub fn load_webhook_secret(team_name: &str) -> Option<String> {
    let cfg = config::load().ok()?;
    let team = config::resolve_team(&cfg, Some(team_name)).ok()?;
    let secret = team.credentials.webhook_secret.clone();
    tracing::debug!(team = %team_name, found = secret.is_some(), "Webhook secret lookup");
    secret
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relevant_event_types_webhook_format() {
        assert!(is_relevant_event("issues"));
        assert!(is_relevant_event("issue_comment"));
        assert!(is_relevant_event("pull_request"));
    }

    #[test]
    fn relevant_event_types_api_format() {
        assert!(is_relevant_event("IssuesEvent"));
        assert!(is_relevant_event("IssueCommentEvent"));
        assert!(is_relevant_event("PullRequestEvent"));
    }

    #[test]
    fn irrelevant_event_types() {
        assert!(!is_relevant_event("push"));
        assert!(!is_relevant_event("PushEvent"));
        assert!(!is_relevant_event("create"));
        assert!(!is_relevant_event("delete"));
        assert!(!is_relevant_event("fork"));
        assert!(!is_relevant_event("watch"));
        assert!(!is_relevant_event("star"));
    }

    #[test]
    fn webhook_signature_valid() {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;

        let secret = "mysecret";
        let body = r#"{"action":"opened"}"#;

        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body.as_bytes());
        let result = mac.finalize();
        let hex_sig = hex::encode(result.into_bytes());
        let header = format!("sha256={}", hex_sig);

        assert!(validate_webhook_signature(secret, body, Some(&header)));
    }

    #[test]
    fn webhook_signature_invalid() {
        let secret = "mysecret";
        let body = r#"{"action":"opened"}"#;
        let bad_sig =
            "sha256=0000000000000000000000000000000000000000000000000000000000000000";

        assert!(!validate_webhook_signature(secret, body, Some(bad_sig)));
    }

    #[test]
    fn webhook_signature_missing_header() {
        assert!(!validate_webhook_signature("secret", "body", None));
    }

    #[test]
    fn webhook_signature_wrong_prefix() {
        assert!(!validate_webhook_signature(
            "secret",
            "body",
            Some("sha1=abcd")
        ));
    }

    #[test]
    fn webhook_signature_invalid_hex() {
        assert!(!validate_webhook_signature(
            "secret",
            "body",
            Some("sha256=not-hex!!")
        ));
    }

    #[test]
    fn github_event_deser() {
        let json =
            r#"[{"id":"12345","type":"IssuesEvent"},{"id":"12346","type":"PushEvent"}]"#;
        let events: Vec<GitHubEvent> = serde_json::from_str(json).unwrap();

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].id, "12345");
        assert_eq!(events[0].event_type, "IssuesEvent");
        assert_eq!(events[1].id, "12346");
        assert_eq!(events[1].event_type, "PushEvent");
    }
}
