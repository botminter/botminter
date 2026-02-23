//! GitHub test helpers — TempRepo with RAII cleanup, label/issue listing.

use std::process::Command;

/// A temporary private GitHub repository that is deleted on drop.
pub struct TempRepo {
    /// Full name in `owner/repo` format.
    pub full_name: String,
}

impl TempRepo {
    /// Creates a new private GitHub repository under a specific organization.
    ///
    /// Returns `Err` if the gh token lacks permission to create repos in the org.
    pub fn new_in_org(prefix: &str, org: &str) -> Result<Self, String> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let repo_name = format!("{}-{}", prefix, timestamp);
        let full_name = format!("{}/{}", org, repo_name);

        let output = Command::new("gh")
            .args(["repo", "create", &full_name, "--private"])
            .output()
            .map_err(|e| format!("failed to run gh repo create: {}", e))?;
        if !output.status.success() {
            return Err(format!(
                "gh repo create '{}' failed: {}",
                full_name,
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        eprintln!("TempRepo created: {}", full_name);
        Ok(TempRepo { full_name })
    }

    /// Creates a new private GitHub repository with a unique timestamped name.
    ///
    /// Returns `Err` if the gh token lacks `CreateRepository` permission or
    /// any other creation failure occurs.
    pub fn new(prefix: &str) -> Result<Self, String> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let repo_name = format!("{}-{}", prefix, timestamp);

        // Get the authenticated user's login
        let whoami = Command::new("gh")
            .args(["api", "user", "--jq", ".login"])
            .output()
            .map_err(|e| format!("failed to get gh user login: {}", e))?;
        if !whoami.status.success() {
            return Err(format!(
                "gh api user failed: {}",
                String::from_utf8_lossy(&whoami.stderr)
            ));
        }
        let user = String::from_utf8_lossy(&whoami.stdout)
            .trim()
            .to_string();

        // Create the private repo
        let output = Command::new("gh")
            .args(["repo", "create", &repo_name, "--private"])
            .output()
            .map_err(|e| format!("failed to run gh repo create: {}", e))?;
        if !output.status.success() {
            return Err(format!(
                "gh repo create '{}' failed: {}",
                repo_name,
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        let full_name = format!("{}/{}", user, repo_name);
        eprintln!("TempRepo created: {}", full_name);

        Ok(TempRepo { full_name })
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        eprintln!("TempRepo dropping: {}", self.full_name);
        let output = Command::new("gh")
            .args(["repo", "delete", &self.full_name, "--yes"])
            .output();
        match output {
            Ok(o) if o.status.success() => {
                eprintln!("TempRepo deleted: {}", self.full_name);
            }
            Ok(o) => {
                eprintln!(
                    "WARNING: failed to delete repo {}: {}",
                    self.full_name,
                    String::from_utf8_lossy(&o.stderr)
                );
            }
            Err(e) => {
                eprintln!(
                    "WARNING: failed to run gh repo delete for {}: {}",
                    self.full_name, e
                );
            }
        }
    }
}

/// Returns `true` if `gh auth status` succeeds.
pub fn gh_auth_ok() -> bool {
    Command::new("gh")
        .args(["auth", "status"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Lists label names on a GitHub repository.
pub fn list_labels(repo: &str) -> Vec<String> {
    let output = Command::new("gh")
        .args([
            "label", "list", "-R", repo, "--json", "name", "--jq", ".[].name",
        ])
        .output()
        .expect("failed to run gh label list");

    if !output.status.success() {
        return Vec::new();
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| l.to_string())
        .collect()
}

/// Lists labels on a GitHub repository with name and color.
///
/// Returns `(name, color)` pairs parsed from `gh label list --json`.
pub fn list_labels_json(repo: &str) -> Vec<(String, String)> {
    let output = Command::new("gh")
        .args([
            "label", "list", "-R", repo, "--json", "name,color", "--limit", "200",
        ])
        .output()
        .expect("failed to run gh label list --json");

    if !output.status.success() {
        return Vec::new();
    }

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).unwrap_or(serde_json::Value::Array(vec![]));

    json.as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|v| {
            let name = v.get("name")?.as_str()?.to_string();
            let color = v.get("color")?.as_str()?.to_string();
            Some((name, color))
        })
        .collect()
}

/// A temporary GitHub Project that is deleted on drop.
pub struct TempProject {
    pub owner: String,
    pub number: u64,
}

impl TempProject {
    /// Creates a new GitHub Project under the given owner.
    pub fn new(owner: &str, title: &str) -> Result<Self, String> {
        let output = Command::new("gh")
            .args([
                "project", "create", "--owner", owner, "--title", title, "--format", "json",
            ])
            .output()
            .map_err(|e| format!("failed to run gh project create: {}", e))?;
        if !output.status.success() {
            return Err(format!(
                "gh project create failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let json: serde_json::Value = serde_json::from_str(stdout.trim())
            .map_err(|e| format!("failed to parse project JSON: {}", e))?;
        let number = json["number"]
            .as_u64()
            .ok_or("missing 'number' in project create output")?;

        eprintln!("TempProject created: {}/project#{}", owner, number);
        Ok(TempProject {
            owner: owner.to_string(),
            number,
        })
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        eprintln!("TempProject dropping: {}/project#{}", self.owner, self.number);
        let output = Command::new("gh")
            .args([
                "project",
                "delete",
                "--owner",
                &self.owner,
                &self.number.to_string(),
                "--format",
                "json",
            ])
            .output();
        match output {
            Ok(o) if o.status.success() => {
                eprintln!("TempProject deleted: {}/project#{}", self.owner, self.number);
            }
            Ok(o) => {
                eprintln!(
                    "WARNING: failed to delete project {}/#{}: {}",
                    self.owner,
                    self.number,
                    String::from_utf8_lossy(&o.stderr)
                );
            }
            Err(e) => {
                eprintln!(
                    "WARNING: failed to run gh project delete for {}/#{}: {}",
                    self.owner, self.number, e
                );
            }
        }
    }
}

/// Lists Status field options on a GitHub Project.
/// Returns a list of option names.
pub fn list_project_status_options(owner: &str, project_number: u64) -> Vec<String> {
    let output = Command::new("gh")
        .args([
            "project",
            "field-list",
            &project_number.to_string(),
            "--owner",
            owner,
            "--format",
            "json",
        ])
        .output()
        .expect("failed to run gh project field-list");

    if !output.status.success() {
        return Vec::new();
    }

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).unwrap_or(serde_json::Value::Null);

    json["fields"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .find(|f| f["name"].as_str() == Some("Status"))
        .and_then(|f| f["options"].as_array())
        .map(|opts| {
            opts.iter()
                .filter_map(|o| o["name"].as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// Lists issue titles on a GitHub repository.
pub fn list_issues(repo: &str) -> Vec<String> {
    let output = Command::new("gh")
        .args([
            "issue", "list", "-R", repo, "--json", "title", "--jq", ".[].title",
        ])
        .output()
        .expect("failed to run gh issue list");

    if !output.status.success() {
        return Vec::new();
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| l.to_string())
        .collect()
}
