use std::path::Path;

use toml::Value;

use crate::build::ast::ManagerInfo;

/// Discover effect managers from Cargo.toml dependencies
pub fn discover_managers(cargo_toml_path: &Path) -> Vec<ManagerInfo> {
    let cargo_content = std::fs::read_to_string(cargo_toml_path)
        .unwrap_or_else(|e| panic!("Failed to read Cargo.toml at {:?}: {}", cargo_toml_path, e));

    let cargo_toml: Value = cargo_content.parse().expect("Failed to parse Cargo.toml");

    let deps = match cargo_toml.get("dependencies") {
        Some(Value::Table(deps)) => deps,
        _ => return Vec::new(),
    };

    let cargo_dir = cargo_toml_path.parent().unwrap();
    let mut managers = Vec::new();

    for (dep_name, dep_value) in deps {
        // Get path to dependency's Cargo.toml
        let dep_cargo_path = match dep_value {
            Value::Table(table) => {
                if let Some(Value::String(path)) = table.get("path") {
                    cargo_dir.join(path).join("Cargo.toml")
                } else {
                    continue; // Skip non-path dependencies
                }
            }
            Value::String(_) => continue, // Skip version-only dependencies
            _ => continue,
        };

        if !dep_cargo_path.exists() {
            continue;
        }

        // Read dependency's Cargo.toml
        let dep_content = match std::fs::read_to_string(&dep_cargo_path) {
            Ok(content) => content,
            Err(_) => continue,
        };

        let dep_toml: Value = match dep_content.parse() {
            Ok(value) => value,
            Err(_) => continue,
        };

        // Check for [package.metadata.saucer] with effect_manager = true
        let saucer_meta = match dep_toml
            .get("package")
            .and_then(|p| p.get("metadata"))
            .and_then(|m| m.get("saucer"))
        {
            Some(meta) => meta,
            None => continue,
        };

        let is_effect_manager = saucer_meta
            .get("effect_manager")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if !is_effect_manager {
            continue;
        }

        let request_type = saucer_meta
            .get("request_type")
            .and_then(|v| v.as_str())
            .unwrap_or("Request")
            .to_string();

        let manager_type = saucer_meta
            .get("manager_type")
            .and_then(|v| v.as_str())
            .unwrap_or("Manager")
            .to_string();

        let self_msg_type = saucer_meta
            .get("self_msg_type")
            .and_then(|v| v.as_str())
            .unwrap_or("()")
            .to_string();

        // Derive variant name from manager type (e.g., "TimeManager" -> "Time")
        let variant = manager_type
            .strip_suffix("Manager")
            .unwrap_or(&manager_type)
            .to_string();

        let module_name = dep_name.replace('-', "_");
        let crate_dir = dep_cargo_path.parent().unwrap();
        let lib_path = crate_dir.join("src").join("lib.rs");

        assert_command_stub(crate_dir, &dep_cargo_path);

        managers.push(ManagerInfo {
            crate_name: dep_name.clone(),
            module_name,
            variant,
            request_type,
            manager_type,
            self_msg_type,
            lib_path,
        });

        // Tell cargo to rerun if dependency's Cargo.toml changes
        println!("cargo:rerun-if-changed={}", dep_cargo_path.display());
    }

    managers
}

fn assert_command_stub(crate_dir: &Path, cargo_path: &Path) {
    let command_path = crate_dir.join("src").join("command.rs");
    if !command_path.exists() {
        return;
    }

    let content = std::fs::read_to_string(&command_path).unwrap_or_else(|e| {
        panic!(
            "Failed to read command.rs in {:?} (declared in {:?}): {}",
            crate_dir, cargo_path, e
        )
    });

    let trimmed = content.trim();
    if trimmed.is_empty() {
        return;
    }

    let has_breadcrumb = trimmed
        .lines()
        .any(|l| l.contains("commands are generated") || l.contains("requests.rs"));

    let all_comments = trimmed
        .lines()
        .all(|l| l.trim().is_empty() || l.trim_start().starts_with("//"));

    if !all_comments || !has_breadcrumb {
        panic!(
            "Effect manager at {:?} has a real src/command.rs; leave it empty except for a breadcrumb comment pointing to src/requests.rs (seen in {:?}).",
            crate_dir, command_path
        );
    }
}
