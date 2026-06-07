//! Conformance tests validate that bridge spec artifacts (bridge.yml, schema.json)
//! have correct structure per the bridge spec. These tests check field presence
//! and types only -- no command execution.

use std::path::Path;

/// Returns the workspace root (two levels up from CARGO_MANIFEST_DIR).
fn workspace_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

/// Returns the path to the bridge examples directory.
fn examples_dir() -> std::path::PathBuf {
    workspace_root().join(".planning/specs/bridge/examples")
}

/// Returns the path to the stub bridge directory.
fn stub_dir() -> std::path::PathBuf {
    examples_dir().join("stub")
}

/// Returns the path to a profile's bridge directory.
fn profile_bridge_dir(profile: &str, bridge: &str) -> std::path::PathBuf {
    workspace_root().join(format!("profiles/{}/bridges/{}", profile, bridge))
}

/// Reads and parses a YAML file into a serde_yml::Value.
fn read_yaml(path: &Path) -> serde_yml::Value {
    let contents = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {}", path.display(), e));
    serde_yml::from_str(&contents)
        .unwrap_or_else(|e| panic!("Failed to parse YAML {}: {}", path.display(), e))
}

/// Reads and parses a JSON file into a serde_json::Value.
fn read_json(path: &Path) -> serde_json::Value {
    let contents = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {}", path.display(), e));
    serde_json::from_str(&contents)
        .unwrap_or_else(|e| panic!("Failed to parse JSON {}: {}", path.display(), e))
}

// ── bridge.yml structure (3 tests) ───────────────────────────────────

#[test]
fn stub_bridge_yml_has_required_fields() {
    let path = stub_dir().join("bridge.yml");
    let val = read_yaml(&path);

    // apiVersion MUST be botminter.dev/v1alpha1
    assert_eq!(
        val["apiVersion"].as_str(),
        Some("botminter.dev/v1alpha1"),
        "apiVersion must be 'botminter.dev/v1alpha1', got: {:?}",
        val["apiVersion"]
    );

    // kind MUST be Bridge
    assert_eq!(
        val["kind"].as_str(),
        Some("Bridge"),
        "kind must be 'Bridge', got: {:?}",
        val["kind"]
    );

    // metadata.name MUST be non-empty string
    let name = val["metadata"]["name"].as_str();
    assert!(
        name.is_some() && !name.unwrap().is_empty(),
        "metadata.name must be a non-empty string, got: {:?}",
        val["metadata"]["name"]
    );

    // spec.type MUST be local or external
    let spec_type = val["spec"]["type"].as_str();
    assert!(
        spec_type == Some("local") || spec_type == Some("external"),
        "spec.type must be 'local' or 'external', got: {:?}",
        val["spec"]["type"]
    );

    // spec.configSchema MUST be a string
    assert!(
        val["spec"]["configSchema"].is_string(),
        "spec.configSchema must be a string, got: {:?}",
        val["spec"]["configSchema"]
    );

    // spec.configDir MUST be a string
    assert!(
        val["spec"]["configDir"].is_string(),
        "spec.configDir must be a string, got: {:?}",
        val["spec"]["configDir"]
    );
}

#[test]
fn local_bridge_has_lifecycle_commands() {
    // Test against both the stub and the reference local bridge
    for bridge_name in ["stub/bridge.yml", "bridge.yml"] {
        let path = if bridge_name.starts_with("stub") {
            stub_dir().join("bridge.yml")
        } else {
            examples_dir().join(bridge_name)
        };
        let val = read_yaml(&path);

        assert_eq!(
            val["spec"]["type"].as_str(),
            Some("local"),
            "{}: spec.type must be 'local'",
            bridge_name
        );

        // lifecycle.start MUST be a string
        assert!(
            val["spec"]["lifecycle"]["start"].is_string(),
            "{}: spec.lifecycle.start must be a string, got: {:?}",
            bridge_name,
            val["spec"]["lifecycle"]["start"]
        );

        // lifecycle.stop MUST be a string
        assert!(
            val["spec"]["lifecycle"]["stop"].is_string(),
            "{}: spec.lifecycle.stop must be a string, got: {:?}",
            bridge_name,
            val["spec"]["lifecycle"]["stop"]
        );

        // lifecycle.health MUST be a string
        assert!(
            val["spec"]["lifecycle"]["health"].is_string(),
            "{}: spec.lifecycle.health must be a string, got: {:?}",
            bridge_name,
            val["spec"]["lifecycle"]["health"]
        );
    }
}

#[test]
fn all_bridges_have_identity_commands() {
    // All bridge types must have identity commands
    for bridge_file in ["stub/bridge.yml", "bridge.yml", "bridge-external.yml"] {
        let path = if bridge_file.starts_with("stub") {
            stub_dir().join("bridge.yml")
        } else {
            examples_dir().join(bridge_file)
        };
        let val = read_yaml(&path);

        // identity.onboard MUST be a string
        assert!(
            val["spec"]["identity"]["onboard"].is_string(),
            "{}: spec.identity.onboard must be a string, got: {:?}",
            bridge_file,
            val["spec"]["identity"]["onboard"]
        );

        // identity.rotate-credentials MUST be a string
        assert!(
            val["spec"]["identity"]["rotate-credentials"].is_string(),
            "{}: spec.identity.rotate-credentials must be a string, got: {:?}",
            bridge_file,
            val["spec"]["identity"]["rotate-credentials"]
        );

        // identity.remove MUST be a string
        assert!(
            val["spec"]["identity"]["remove"].is_string(),
            "{}: spec.identity.remove must be a string, got: {:?}",
            bridge_file,
            val["spec"]["identity"]["remove"]
        );
    }
}

// ── schema.json structure (1 test) ───────────────────────────────────

#[test]
fn stub_schema_json_is_valid_json_schema() {
    let path = stub_dir().join("schema.json");
    let val = read_json(&path);

    // Must have $schema string field
    assert!(
        val["$schema"].is_string(),
        "$schema must be a string, got: {:?}",
        val["$schema"]
    );

    // Must have type field with value "object"
    assert_eq!(
        val["type"].as_str(),
        Some("object"),
        "type must be 'object', got: {:?}",
        val["type"]
    );

    // Must have properties as an object
    assert!(
        val["properties"].is_object(),
        "properties must be an object, got: {:?}",
        val["properties"]
    );
}

// ── external bridge (2 tests) ────────────────────────────────────────

#[test]
fn external_bridge_has_no_lifecycle() {
    let path = examples_dir().join("bridge-external.yml");
    let val = read_yaml(&path);

    // spec.type MUST be external
    assert_eq!(
        val["spec"]["type"].as_str(),
        Some("external"),
        "spec.type must be 'external', got: {:?}",
        val["spec"]["type"]
    );

    // spec.lifecycle MUST NOT be present
    assert!(
        val["spec"]["lifecycle"].is_null(),
        "external bridge must not have spec.lifecycle, got: {:?}",
        val["spec"]["lifecycle"]
    );
}

#[test]
fn external_bridge_has_identity_commands() {
    let path = examples_dir().join("bridge-external.yml");
    let val = read_yaml(&path);

    assert!(
        val["spec"]["identity"]["onboard"].is_string(),
        "spec.identity.onboard must be a string, got: {:?}",
        val["spec"]["identity"]["onboard"]
    );

    assert!(
        val["spec"]["identity"]["rotate-credentials"].is_string(),
        "spec.identity.rotate-credentials must be a string, got: {:?}",
        val["spec"]["identity"]["rotate-credentials"]
    );

    assert!(
        val["spec"]["identity"]["remove"].is_string(),
        "spec.identity.remove must be a string, got: {:?}",
        val["spec"]["identity"]["remove"]
    );
}

// ── directory structure (1 test) ─────────────────────────────────────

#[test]
fn stub_bridge_has_required_files() {
    let stub = stub_dir();

    assert!(
        stub.join("bridge.yml").exists(),
        "stub/bridge.yml must exist"
    );
    assert!(
        stub.join("schema.json").exists(),
        "stub/schema.json must exist"
    );
    assert!(
        stub.join("Justfile").exists(),
        "stub/Justfile must exist"
    );
}

// ── Telegram bridge conformance (profile bridges) ───────────────────

#[test]
fn telegram_bridge_yml_has_required_fields() {
    // Validate both agentic-sdlc-minimal and scrum profile copies
    for profile in ["agentic-sdlc-minimal", "scrum"] {
        let path = profile_bridge_dir(profile, "telegram").join("bridge.yml");
        let val = read_yaml(&path);

        assert_eq!(
            val["apiVersion"].as_str(),
            Some("botminter.dev/v1alpha1"),
            "{}/telegram: apiVersion must be 'botminter.dev/v1alpha1'",
            profile
        );

        assert_eq!(
            val["kind"].as_str(),
            Some("Bridge"),
            "{}/telegram: kind must be 'Bridge'",
            profile
        );

        let name = val["metadata"]["name"].as_str();
        assert_eq!(
            name,
            Some("telegram"),
            "{}/telegram: metadata.name must be 'telegram'",
            profile
        );

        assert_eq!(
            val["spec"]["type"].as_str(),
            Some("external"),
            "{}/telegram: spec.type must be 'external'",
            profile
        );

        assert!(
            val["spec"]["configSchema"].is_string(),
            "{}/telegram: spec.configSchema must be a string",
            profile
        );

        assert!(
            val["spec"]["configDir"].is_string(),
            "{}/telegram: spec.configDir must be a string",
            profile
        );
    }
}

#[test]
fn telegram_bridge_has_no_lifecycle() {
    for profile in ["agentic-sdlc-minimal", "scrum"] {
        let path = profile_bridge_dir(profile, "telegram").join("bridge.yml");
        let val = read_yaml(&path);

        // External bridge MUST NOT have lifecycle section
        assert!(
            val["spec"]["lifecycle"].is_null(),
            "{}/telegram: external bridge must not have spec.lifecycle",
            profile
        );
    }
}

#[test]
fn telegram_bridge_has_identity_commands() {
    for profile in ["agentic-sdlc-minimal", "scrum"] {
        let path = profile_bridge_dir(profile, "telegram").join("bridge.yml");
        let val = read_yaml(&path);

        assert!(
            val["spec"]["identity"]["onboard"].is_string(),
            "{}/telegram: spec.identity.onboard must be a string",
            profile
        );

        assert!(
            val["spec"]["identity"]["rotate-credentials"].is_string(),
            "{}/telegram: spec.identity.rotate-credentials must be a string",
            profile
        );

        assert!(
            val["spec"]["identity"]["remove"].is_string(),
            "{}/telegram: spec.identity.remove must be a string",
            profile
        );
    }
}

#[test]
fn telegram_schema_json_has_bot_token() {
    for profile in ["agentic-sdlc-minimal", "scrum"] {
        let path = profile_bridge_dir(profile, "telegram").join("schema.json");
        let val = read_json(&path);

        assert!(
            val["$schema"].is_string(),
            "{}/telegram: $schema must be a string",
            profile
        );

        assert_eq!(
            val["type"].as_str(),
            Some("object"),
            "{}/telegram: type must be 'object'",
            profile
        );

        assert!(
            val["properties"].is_object(),
            "{}/telegram: properties must be an object",
            profile
        );

        // Telegram-specific: must have bot_token property
        assert!(
            val["properties"]["bot_token"].is_object(),
            "{}/telegram: properties.bot_token must be defined",
            profile
        );

        // bot_token must be required
        let required = val["required"].as_array();
        assert!(
            required.is_some(),
            "{}/telegram: required array must be present",
            profile
        );
        let required = required.unwrap();
        assert!(
            required.iter().any(|v| v.as_str() == Some("bot_token")),
            "{}/telegram: bot_token must be in required array",
            profile
        );
    }
}

#[test]
fn telegram_bridge_has_required_files() {
    for profile in ["agentic-sdlc-minimal", "scrum"] {
        let dir = profile_bridge_dir(profile, "telegram");

        assert!(
            dir.join("bridge.yml").exists(),
            "{}/telegram/bridge.yml must exist",
            profile
        );
        assert!(
            dir.join("schema.json").exists(),
            "{}/telegram/schema.json must exist",
            profile
        );
        assert!(
            dir.join("Justfile").exists(),
            "{}/telegram/Justfile must exist",
            profile
        );
    }
}

// ── Tuwunel bridge conformance ──────────────────────────────────────

#[test]
fn tuwunel_bridge_yml_has_required_fields() {
    for profile in ["agentic-sdlc-minimal", "scrum"] {
        let path = profile_bridge_dir(profile, "tuwunel").join("bridge.yml");
        let val = read_yaml(&path);

        assert_eq!(
            val["apiVersion"].as_str(),
            Some("botminter.dev/v1alpha1"),
            "{}/tuwunel: apiVersion must be 'botminter.dev/v1alpha1'",
            profile
        );

        assert_eq!(
            val["kind"].as_str(),
            Some("Bridge"),
            "{}/tuwunel: kind must be 'Bridge'",
            profile
        );

        assert_eq!(
            val["metadata"]["name"].as_str(),
            Some("tuwunel"),
            "{}/tuwunel: metadata.name must be 'tuwunel'",
            profile
        );

        assert_eq!(
            val["spec"]["type"].as_str(),
            Some("local"),
            "{}/tuwunel: spec.type must be 'local'",
            profile
        );

        assert!(
            val["spec"]["configSchema"].is_string(),
            "{}/tuwunel: spec.configSchema must be a string",
            profile
        );

        assert!(
            val["spec"]["configDir"].is_string(),
            "{}/tuwunel: spec.configDir must be a string",
            profile
        );
    }
}

#[test]
fn tuwunel_bridge_has_lifecycle_commands() {
    for profile in ["agentic-sdlc-minimal", "scrum"] {
        let path = profile_bridge_dir(profile, "tuwunel").join("bridge.yml");
        let val = read_yaml(&path);

        assert!(
            val["spec"]["lifecycle"]["start"].is_string(),
            "{}/tuwunel: lifecycle.start must be a string",
            profile
        );
        assert!(
            val["spec"]["lifecycle"]["stop"].is_string(),
            "{}/tuwunel: lifecycle.stop must be a string",
            profile
        );
        assert!(
            val["spec"]["lifecycle"]["health"].is_string(),
            "{}/tuwunel: lifecycle.health must be a string",
            profile
        );
    }
}

#[test]
fn tuwunel_bridge_has_identity_commands() {
    for profile in ["agentic-sdlc-minimal", "scrum"] {
        let path = profile_bridge_dir(profile, "tuwunel").join("bridge.yml");
        let val = read_yaml(&path);

        assert!(
            val["spec"]["identity"]["onboard"].is_string(),
            "{}/tuwunel: identity.onboard must be a string",
            profile
        );
        assert!(
            val["spec"]["identity"]["rotate-credentials"].is_string(),
            "{}/tuwunel: identity.rotate-credentials must be a string",
            profile
        );
        assert!(
            val["spec"]["identity"]["remove"].is_string(),
            "{}/tuwunel: identity.remove must be a string",
            profile
        );
    }
}

#[test]
fn tuwunel_bridge_has_room_commands() {
    for profile in ["agentic-sdlc-minimal", "scrum"] {
        let path = profile_bridge_dir(profile, "tuwunel").join("bridge.yml");
        let val = read_yaml(&path);

        assert!(
            val["spec"]["room"]["create"].is_string(),
            "{}/tuwunel: room.create must be a string",
            profile
        );
        assert!(
            val["spec"]["room"]["list"].is_string(),
            "{}/tuwunel: room.list must be a string",
            profile
        );
    }
}

#[test]
fn tuwunel_schema_json_has_host() {
    for profile in ["agentic-sdlc-minimal", "scrum"] {
        let path = profile_bridge_dir(profile, "tuwunel").join("schema.json");
        let contents = std::fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("Could not read {}", path.display()));
        let val: serde_json::Value = serde_json::from_str(&contents)
            .unwrap_or_else(|_| panic!("Invalid JSON in {}", path.display()));

        assert!(
            val["$schema"].is_string(),
            "{}/tuwunel: schema.json must have $schema",
            profile
        );
        assert_eq!(
            val["type"].as_str(),
            Some("object"),
            "{}/tuwunel: schema.json type must be 'object'",
            profile
        );
        assert!(
            val["properties"]["host"].is_object(),
            "{}/tuwunel: schema.json must have properties.host",
            profile
        );
        let required = val["required"].as_array()
            .unwrap_or_else(|| panic!("{}/tuwunel: schema.json must have required array", profile));
        assert!(
            required.iter().any(|v| v.as_str() == Some("host")),
            "{}/tuwunel: schema.json required must include 'host'",
            profile
        );
    }
}

#[test]
fn tuwunel_bridge_has_required_files() {
    for profile in ["agentic-sdlc-minimal", "scrum"] {
        let dir = profile_bridge_dir(profile, "tuwunel");

        assert!(
            dir.join("bridge.yml").exists(),
            "{}/tuwunel/bridge.yml must exist",
            profile
        );
        assert!(
            dir.join("schema.json").exists(),
            "{}/tuwunel/schema.json must exist",
            profile
        );
        assert!(
            dir.join("Justfile").exists(),
            "{}/tuwunel/Justfile must exist",
            profile
        );
    }
}
