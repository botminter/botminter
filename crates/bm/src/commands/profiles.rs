use anyhow::Result;
use comfy_table::{Table, presets::UTF8_FULL_CONDENSED, modifiers::UTF8_ROUND_CORNERS};

use crate::profile;

/// Handles `bm profiles list` — displays a table of all embedded profiles.
pub fn list() -> Result<()> {
    let names = profile::list_profiles();

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec!["Profile", "Version", "Schema", "Description"]);

    for name in &names {
        let manifest = profile::read_manifest(name)?;
        table.add_row(vec![
            &manifest.name,
            &manifest.version,
            &manifest.schema_version,
            &manifest.description,
        ]);
    }

    println!("{table}");
    Ok(())
}

/// Handles `bm profiles describe <profile>` — shows full profile details.
pub fn describe(name: &str) -> Result<()> {
    let manifest = profile::read_manifest(name)?;

    println!("Profile: {}", manifest.name);
    println!("Display Name: {}", manifest.display_name);
    println!("Version: {}", manifest.version);
    println!("Schema: {}", manifest.schema_version);
    println!("Description: {}", manifest.description);

    println!();
    println!("Available Roles:");
    let roles = profile::list_roles(name)?;
    // Build a lookup from manifest roles for descriptions
    let role_descriptions: std::collections::HashMap<&str, &str> = manifest
        .roles
        .iter()
        .map(|r| (r.name.as_str(), r.description.as_str()))
        .collect();

    for role in &roles {
        let desc = role_descriptions
            .get(role.as_str())
            .unwrap_or(&"");
        println!("  {:<20} {}", role, desc);
    }

    println!();
    println!("Labels ({}):", manifest.labels.len());
    for label in &manifest.labels {
        println!("  {:<30} {}", label.name, label.description);
    }

    Ok(())
}
