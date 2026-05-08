use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Profile manifest parsed from botminter.yml
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProfileManifest {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub version: String,
    pub schema_version: String,
    #[serde(default)]
    pub roles: Vec<RoleDef>,
    #[serde(default)]
    pub labels: Vec<LabelDef>,
    #[serde(default)]
    pub statuses: Vec<StatusDef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub projects: Vec<ProjectDef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub views: Vec<ViewDef>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub coding_agents: HashMap<String, CodingAgentDef>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub default_coding_agent: String,
    /// Bridges supported by this profile.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bridges: Vec<BridgeDef>,
    /// The selected bridge for this team (set during `bm init`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bridge: Option<String>,
    /// Operator identity (set during `bm init --bridge` for local bridges).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operator: Option<OperatorDef>,
    /// Meetings declared by this profile. Each meeting is a named shortcut
    /// for launching an agent with custom instructions — available via
    /// `bm meetings <name>`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub meetings: Vec<Meeting>,
}

/// Operator identity configuration.
///
/// Defines who the human operator is in bridge contexts (e.g., the admin user
/// that created the bridge server). Set during `bm init --bridge` for local bridges.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct OperatorDef {
    pub bridge_username: String,
}

/// A bridge declaration in the profile manifest.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BridgeDef {
    pub name: String,
    pub display_name: String,
    pub description: String,
    #[serde(rename = "type")]
    pub bridge_type: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RoleDef {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LabelDef {
    pub name: String,
    pub color: String,
    pub description: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StatusDef {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProjectDef {
    pub name: String,
    pub fork_url: String,
}

/// Describes a coding agent's file conventions and binary name.
/// Used by the extraction pipeline to determine context file names,
/// agent directories, and which binary to launch.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CodingAgentDef {
    pub name: String,
    pub display_name: String,
    pub context_file: String,
    pub agent_dir: String,
    pub binary: String,
    /// CLI flag to append a system prompt file (e.g. "--append-system-prompt-file")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt_flag: Option<String>,
    /// CLI flag to skip permission prompts (e.g. "--dangerously-skip-permissions")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skip_permissions_flag: Option<String>,
}

/// A named shortcut for launching an agent with custom instructions and an
/// initial prompt, available via `bm meetings <name>`.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Meeting {
    pub name: String,
    pub description: String,
    pub member: String,
    /// The meeting's system prompt. Written to a temp file and passed via
    /// `--append-system-prompt-file`. This replaces the full meta-prompt
    /// pipeline that `bm chat` uses.
    pub instructions: String,
    /// Initial message prepended to user-provided trailing args.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
}

/// Defines a role-based view for the GitHub Project board.
/// Each view maps to a subset of statuses via prefix matching.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ViewDef {
    pub name: String,
    /// Status name prefixes to include (e.g., ["po"] matches "po:triage", "po:backlog", etc.)
    pub prefixes: Vec<String>,
    /// Extra statuses always included regardless of prefix (e.g., ["done", "error"])
    #[serde(default)]
    pub also_include: Vec<String>,
}

impl ViewDef {
    /// Expands prefixes against the full status list, returning matching status names
    /// plus any `also_include` entries.
    pub fn resolve_statuses(&self, all_statuses: &[StatusDef]) -> Vec<String> {
        let mut result: Vec<String> = all_statuses
            .iter()
            .filter(|s| {
                self.prefixes
                    .iter()
                    .any(|p| s.name.starts_with(&format!("{}:", p)))
            })
            .map(|s| s.name.clone())
            .collect();
        for extra in &self.also_include {
            if !result.contains(extra) {
                result.push(extra.clone());
            }
        }
        result
    }

    /// Builds a GitHub Projects filter string for this view.
    /// Example: `status:po:triage,po:backlog,po:ready,done,error`
    pub fn filter_string(&self, all_statuses: &[StatusDef]) -> String {
        let statuses = self.resolve_statuses(all_statuses);
        format!("status:{}", statuses.join(","))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── ViewDef tests ────────────────────────────────────────

    fn sample_statuses() -> Vec<StatusDef> {
        vec![
            StatusDef { name: "po:triage".into(), description: "".into() },
            StatusDef { name: "po:backlog".into(), description: "".into() },
            StatusDef { name: "arch:design".into(), description: "".into() },
            StatusDef { name: "arch:plan".into(), description: "".into() },
            StatusDef { name: "dev:implement".into(), description: "".into() },
            StatusDef { name: "done".into(), description: "".into() },
            StatusDef { name: "error".into(), description: "".into() },
        ]
    }

    #[test]
    fn view_resolve_single_prefix() {
        let view = ViewDef {
            name: "PO".into(),
            prefixes: vec!["po".into()],
            also_include: vec!["done".into(), "error".into()],
        };
        let resolved = view.resolve_statuses(&sample_statuses());
        assert_eq!(resolved, vec!["po:triage", "po:backlog", "done", "error"]);
    }

    #[test]
    fn view_resolve_multiple_prefixes() {
        let view = ViewDef {
            name: "Mixed".into(),
            prefixes: vec!["po".into(), "arch".into()],
            also_include: vec![],
        };
        let resolved = view.resolve_statuses(&sample_statuses());
        assert_eq!(resolved, vec!["po:triage", "po:backlog", "arch:design", "arch:plan"]);
    }

    #[test]
    fn view_resolve_no_duplicates_in_also_include() {
        let view = ViewDef {
            name: "Dev".into(),
            prefixes: vec!["dev".into()],
            also_include: vec!["done".into(), "dev:implement".into()],
        };
        let resolved = view.resolve_statuses(&sample_statuses());
        assert_eq!(resolved, vec!["dev:implement", "done"]);
    }

    #[test]
    fn view_resolve_empty_prefixes_returns_only_also_include() {
        let view = ViewDef {
            name: "Bare".into(),
            prefixes: vec![],
            also_include: vec!["done".into()],
        };
        let resolved = view.resolve_statuses(&sample_statuses());
        assert_eq!(resolved, vec!["done"]);
    }

    #[test]
    fn view_resolve_no_match_returns_only_also_include() {
        let view = ViewDef {
            name: "NoMatch".into(),
            prefixes: vec!["nonexistent".into()],
            also_include: vec!["error".into()],
        };
        let resolved = view.resolve_statuses(&sample_statuses());
        assert_eq!(resolved, vec!["error"]);
    }

    #[test]
    fn view_filter_string_format() {
        let view = ViewDef {
            name: "Arch".into(),
            prefixes: vec!["arch".into()],
            also_include: vec!["done".into()],
        };
        let filter = view.filter_string(&sample_statuses());
        assert_eq!(filter, "status:arch:design,arch:plan,done");
    }

    // ── BridgeDef / ProfileManifest.bridges tests ───────────────

    #[test]
    fn manifest_with_bridges_deserializes() {
        let yaml = r#"
name: test
display_name: "Test Profile"
description: "Test"
version: "1.0.0"
schema_version: "1.0"
bridges:
  - name: telegram
    display_name: "Telegram"
    description: "Telegram bot"
    type: external
"#;
        let manifest: ProfileManifest = serde_yml::from_str(yaml).unwrap();
        assert_eq!(manifest.bridges.len(), 1);
        assert_eq!(manifest.bridges[0].name, "telegram");
        assert_eq!(manifest.bridges[0].display_name, "Telegram");
        assert_eq!(manifest.bridges[0].description, "Telegram bot");
        assert_eq!(manifest.bridges[0].bridge_type, "external");
    }

    #[test]
    fn manifest_no_bridges_deserializes() {
        let yaml = r#"
name: test
display_name: "Test Profile"
description: "Test"
version: "1.0.0"
schema_version: "1.0"
"#;
        let manifest: ProfileManifest = serde_yml::from_str(yaml).unwrap();
        assert!(manifest.bridges.is_empty());
    }

    #[test]
    fn manifest_empty_bridges_deserializes() {
        let yaml = r#"
name: test
display_name: "Test Profile"
description: "Test"
version: "1.0.0"
schema_version: "1.0"
bridges: []
"#;
        let manifest: ProfileManifest = serde_yml::from_str(yaml).unwrap();
        assert!(manifest.bridges.is_empty());
    }

    #[test]
    fn bridge_def_has_expected_fields() {
        let bridge = BridgeDef {
            name: "telegram".into(),
            display_name: "Telegram".into(),
            description: "Bot API".into(),
            bridge_type: "external".into(),
        };
        assert_eq!(bridge.name, "telegram");
        assert_eq!(bridge.display_name, "Telegram");
        assert_eq!(bridge.description, "Bot API");
        assert_eq!(bridge.bridge_type, "external");
    }

    // ── Meeting tests ──────────────────────────────────────────

    #[test]
    fn manifest_with_meetings_deserializes() {
        let yaml = r#"
name: test
display_name: "Test Profile"
description: "Test"
version: "1.0.0"
schema_version: "1.0"
meetings:
  - name: planning
    description: "Collaborative planning session"
    member: engineer
    instructions: |
      You are an engineer in a planning meeting.
      Help the user plan their work.
    prompt: start
  - name: verification
    description: "Verify acceptance criteria"
    member: engineer
    instructions: |
      You are an engineer in a verification meeting.
"#;
        let manifest: ProfileManifest = serde_yml::from_str(yaml).unwrap();
        assert_eq!(manifest.meetings.len(), 2);

        let planning = &manifest.meetings[0];
        assert_eq!(planning.name, "planning");
        assert_eq!(planning.member, "engineer");
        assert!(planning.instructions.contains("planning meeting"));
        assert_eq!(planning.prompt, Some("start".into()));

        let verification = &manifest.meetings[1];
        assert_eq!(verification.name, "verification");
        assert!(verification.instructions.contains("verification meeting"));
        assert!(verification.prompt.is_none());
    }

    #[test]
    fn manifest_no_meetings_deserializes() {
        let yaml = r#"
name: test
display_name: "Test Profile"
description: "Test"
version: "1.0.0"
schema_version: "1.0"
"#;
        let manifest: ProfileManifest = serde_yml::from_str(yaml).unwrap();
        assert!(manifest.meetings.is_empty());
    }

    #[test]
    fn manifest_empty_meetings_deserializes() {
        let yaml = r#"
name: test
display_name: "Test Profile"
description: "Test"
version: "1.0.0"
schema_version: "1.0"
meetings: []
"#;
        let manifest: ProfileManifest = serde_yml::from_str(yaml).unwrap();
        assert!(manifest.meetings.is_empty());
    }

    #[test]
    fn meeting_no_prompt_deserializes() {
        let yaml = r#"
name: test
display_name: "Test Profile"
description: "Test"
version: "1.0.0"
schema_version: "1.0"
meetings:
  - name: planning
    description: "Test"
    member: engineer
    instructions: "Do the thing."
"#;
        let manifest: ProfileManifest = serde_yml::from_str(yaml).unwrap();
        assert_eq!(manifest.meetings[0].instructions, "Do the thing.");
        assert!(manifest.meetings[0].prompt.is_none());
    }

    #[test]
    fn meeting_multiline_instructions_roundtrip() {
        let yaml = r#"
name: test
display_name: "Test Profile"
description: "Test"
version: "1.0.0"
schema_version: "1.0"
meetings:
  - name: planning
    description: "Test"
    member: engineer
    instructions: |
      Line one.
      Line two.
      Line three.
    prompt: start
"#;
        let manifest: ProfileManifest = serde_yml::from_str(yaml).unwrap();
        let meeting = &manifest.meetings[0];
        assert!(meeting.instructions.contains("Line one."));
        assert!(meeting.instructions.contains("Line two."));
        assert!(meeting.instructions.contains("Line three."));
        // YAML | block includes trailing newline
        assert!(meeting.instructions.ends_with('\n'));
    }
}
