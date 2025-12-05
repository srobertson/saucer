//! Build-time code generation for runtime.rs
//!
//! This module provides utilities for build.rs scripts to generate
//! the unified runtime module with Request enum, Cmd type, command helpers,
//! and Runtime struct.

mod ast;
mod emit;
mod hygiene;
mod managers;
mod ports;
mod request;
mod templates;

use crate::build::ast::{ManagerInfo, PortSpec, RuntimeSpec};
use crate::build::hygiene::{crate_to_module_name, dependency_infos, run_hygiene_checks};
use crate::build::templates::{
    discover_templates, discover_templates_from_template, find_reconciler_manager_module,
    find_runtime_host_file, package_name, transform_template,
};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use std::collections::HashMap;
use std::path::Path;

pub use emit::generate_runtime_source;

/// Main entry point: generate runtime.rs
pub fn generate_runtime() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .expect("CARGO_MANIFEST_DIR not set - must be called from build.rs");

    let cargo_toml_path = Path::new(&manifest_dir).join("Cargo.toml");
    let package_name = package_name(&cargo_toml_path);

    // Hygiene checks on participating crates
    run_hygiene_checks(&cargo_toml_path);

    // Discover managers from dependencies
    let managers = managers::discover_managers(&cargo_toml_path);

    // Locate the runtime host file (the one that includes runtime.rs)
    let (runtime_host_path, runtime_host_source, _runtime_host_ast) =
        find_runtime_host_file(Path::new(&manifest_dir));

    // Discover templates from runtime host file
    let reconciler_manager_module =
        find_reconciler_manager_module(Path::new(&manifest_dir), &managers).expect(
            "Failed to find Runtime::run reconciler usage pattern. Ensure `Runtime::run` or `Runtime::new(...).run()` is invoked with the app functions and a reconciler from an effect manager.",
        );
    let reconciler_manager = managers
        .iter()
        .find(|m| &m.module_name == &reconciler_manager_module)
        .cloned()
        .unwrap_or_else(|| panic!("Reconciler references `{}` but no registered effect manager provides that module. Check the manager's Cargo.toml `[package.metadata.saucer] effect_manager = true` block.", reconciler_manager_module));

    let effect_managers: Vec<ManagerInfo> = managers
        .iter()
        .filter(|m| m.crate_name != "saucer-core")
        .cloned()
        .collect();

    // Discover templates from runtime host (after fast failure above)
    let mut templates = discover_templates(&runtime_host_path, &runtime_host_source);

    // Discover additional templates referenced from within templates via crate-qualified uses.
    let dep_map: HashMap<String, hygiene::DepInfo> = dependency_infos(&cargo_toml_path)
        .into_iter()
        .map(|d| (d.module_name.clone(), d))
        .collect();
    let mut idx = 0;
    while idx < templates.len() {
        let t = templates[idx].clone();
        discover_templates_from_template(&t, &dep_map, &mut templates);
        idx += 1;
    }

    // Parse ports on root templates; reject ports in transitive templates.
    for t in templates.iter_mut() {
        let source = std::fs::read_to_string(&t.path)
            .unwrap_or_else(|e| panic!("Failed to read template {:?}: {}", t.path, e));
        let parsed_ports = ports::parse_ports(&source);
        if !parsed_ports.is_empty() {
            if !t.is_root {
                panic!("Ports may only be declared in templates imported directly from main.rs. `{}` is a transitive template but contains #[port] functions.", t.path.display());
            }
            t.ports = parsed_ports;
        }
    }

    // Transform templates and collect which helpers were actually used
    let dep_template_catalog: Vec<(String, String)> = templates
        .iter()
        .map(|t| (crate_to_module_name(&t.crate_name), t.module_name.clone()))
        .collect();

    let mut all_used_helpers: Vec<(String, String)> = Vec::new();
    let transformed_templates: Vec<_> = templates
        .iter_mut()
        .map(|t| {
            let transformed =
                transform_template(&t.path, &managers, &dep_template_catalog, &t.ports);
            t.used_helpers = transformed.used_helpers.clone();
            all_used_helpers.extend(transformed.used_helpers.into_iter());
            (t.module_name.clone(), transformed.code)
        })
        .collect();

    let all_ports: Vec<PortSpec> = templates.iter().flat_map(|t| t.ports.clone()).collect();

    let has_outgoing_ports = all_ports.iter().any(|p| matches!(p, PortSpec::Outgoing(_)));

    // Choose the primary app template (first root from the current crate) for Msg pathing.
    let app_template = templates
        .iter()
        .find(|t| {
            t.is_root && crate_to_module_name(&t.crate_name) == crate_to_module_name(&package_name)
        })
        .unwrap_or_else(|| templates.first().expect("No templates discovered"));
    let app_crate_ident = format_ident!("{}", crate_to_module_name(&app_template.crate_name));
    let app_module_ident = format_ident!("{}", app_template.module_name);
    let app_msg_path: TokenStream = quote! { #app_crate_ident::#app_module_ident::Msg };

    let ports_with_paths: Vec<(PortSpec, proc_macro2::Ident, proc_macro2::Ident)> = templates
        .iter()
        .filter(|t| t.is_root)
        .flat_map(|t| {
            let crate_ident = format_ident!("{}", crate_to_module_name(&t.crate_name));
            let module_ident = format_ident!("{}", t.module_name);
            t.ports
                .iter()
                .cloned()
                .map(move |p| (p, crate_ident.clone(), module_ident.clone()))
        })
        .collect();

    let spec = RuntimeSpec {
        package_name,
        managers,
        effect_managers,
        reconciler_manager,
        templates,
        transformed_templates,
        used_helpers: all_used_helpers,
        all_ports,
        ports_with_paths,
        app_msg_path,
        has_outgoing_ports,
    };

    let output = emit::generate_runtime_source(&spec);

    // Write to OUT_DIR
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set - must be called from build.rs");
    let out_path = Path::new(&out_dir).join("runtime.rs");

    std::fs::write(&out_path, output)
        .unwrap_or_else(|e| panic!("Failed to write runtime.rs: {}", e));

    // Tell cargo to rerun if relevant files change
    println!("cargo:rerun-if-changed={}", cargo_toml_path.display());
    println!("cargo:rerun-if-changed={}", runtime_host_path.display());
    for template in &spec.templates {
        println!("cargo:rerun-if-changed={}", template.path.display());
    }
}
