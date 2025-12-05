use std::path::Path;

use toml::Value;

#[derive(Debug, Clone)]
pub struct DepInfo {
    pub crate_name: String,
    pub module_name: String,
    pub path: std::path::PathBuf,
    pub has_templates: bool,
    pub exclude_ok: bool,
}

pub fn crate_to_module_name(crate_name: &str) -> String {
    crate_name.replace('-', "_")
}

pub fn read_crate_metadata(cargo_toml_path: &Path) -> bool {
    let content = std::fs::read_to_string(cargo_toml_path)
        .unwrap_or_else(|e| panic!("Failed to read Cargo.toml at {:?}: {}", cargo_toml_path, e));
    let toml: Value = content.parse().expect("Failed to parse Cargo.toml");
    toml.get("package")
        .and_then(|p| p.get("metadata"))
        .and_then(|m| m.get("saucer"))
        .and_then(|v| v.get("has_templates"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

pub fn package_exclude_has_tea_glob(cargo_toml_path: &Path) -> bool {
    let content = std::fs::read_to_string(cargo_toml_path)
        .unwrap_or_else(|e| panic!("Failed to read Cargo.toml at {:?}: {}", cargo_toml_path, e));
    let toml: Value = content.parse().expect("Failed to parse Cargo.toml");
    let excludes = toml
        .get("package")
        .and_then(|p| p.get("exclude"))
        .and_then(|e| e.as_array())
        .cloned()
        .unwrap_or_default();
    excludes
        .into_iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .any(|s| s.contains(".tea.rs"))
}

pub fn dependency_infos(cargo_toml_path: &Path) -> Vec<DepInfo> {
    let content = std::fs::read_to_string(cargo_toml_path)
        .unwrap_or_else(|e| panic!("Failed to read Cargo.toml at {:?}: {}", cargo_toml_path, e));
    let toml: Value = content.parse().expect("Failed to parse Cargo.toml");
    let deps = toml
        .get("dependencies")
        .and_then(|d| d.as_table())
        .cloned()
        .unwrap_or_default();
    let mut out = Vec::new();
    let cargo_dir = cargo_toml_path.parent().unwrap();
    for (dep_name, dep_value) in deps {
        let dep_path = match dep_value {
            Value::Table(t) => t
                .get("path")
                .and_then(|p| p.as_str())
                .map(|p| cargo_dir.join(p)),
            _ => None,
        };
        if let Some(path) = dep_path {
            let crate_name = dep_name.clone();
            let module_name = crate_to_module_name(&dep_name);
            let cargo = path.join("Cargo.toml");
            let has_templates = read_crate_metadata(&cargo);
            let exclude_ok = package_exclude_has_tea_glob(&cargo);
            out.push(DepInfo {
                crate_name,
                module_name,
                path,
                has_templates,
                exclude_ok,
            });
        }
    }
    out
}

pub fn run_hygiene_checks(cargo_toml_path: &Path) {
    // Local crate must exclude .tea.rs from Cargo packaging to avoid accidental compilation.
    if !package_exclude_has_tea_glob(cargo_toml_path) {
        panic!(
            "Hygiene check: crate at {} must include `exclude = [\"src/*.tea.rs\"]` in Cargo.toml to keep templates out of normal compilation.",
            cargo_toml_path.display()
        );
    }

    // Dependencies with templates must also exclude .tea.rs
    for dep in dependency_infos(cargo_toml_path) {
        if dep.has_templates && !dep.exclude_ok {
            panic!(
                "Hygiene check: dependency `{}` must include `exclude = [\"src/*.tea.rs\"]` in its Cargo.toml to keep templates out of normal compilation.",
                dep.crate_name
            );
        }
    }
}
