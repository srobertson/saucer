use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use syn::{parse_file, token::RArrow, Block, Expr, File, Item, Stmt, UseTree};
use toml::Value;

use crate::build::ast::{ManagerInfo, PortDirection, PortSpec, TemplateInfo};
use crate::build::hygiene::{crate_to_module_name, dependency_infos, read_crate_metadata, DepInfo};

#[derive(Debug, Clone, PartialEq, Eq)]
struct UsePath {
    segments: Vec<String>,
    is_glob: bool,
}

const RUNTIME_INCLUDE_MARKER: &str = "include!(concat!(env!(\"OUT_DIR\"), \"/runtime.rs\"))";

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    if !dir.exists() {
        return;
    }
    let Ok(read_dir) = fs::read_dir(dir) else {
        return;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().map(|ext| ext == "rs").unwrap_or(false) {
            out.push(path);
        }
    }
}

fn candidate_runtime_files(manifest_dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let src_dir = manifest_dir.join("src");
    files.push(src_dir.join("main.rs"));
    files.push(src_dir.join("lib.rs"));
    collect_rs_files(&src_dir.join("bin"), &mut files);
    collect_rs_files(&manifest_dir.join("tests"), &mut files);
    collect_rs_files(&manifest_dir.join("examples"), &mut files);
    files.into_iter().filter(|p| p.exists()).collect()
}

pub fn find_runtime_host_file(manifest_dir: &Path) -> (PathBuf, String, File) {
    let mut matches = Vec::new();
    for path in candidate_runtime_files(manifest_dir) {
        let Ok(source) = fs::read_to_string(&path) else {
            continue;
        };
        if source.contains(RUNTIME_INCLUDE_MARKER) {
            matches.push((path, source));
        }
    }

    if matches.is_empty() {
        panic!(
            "Failed to find {} in any of: src/main.rs, src/lib.rs, src/bin/*.rs, tests/**/*.rs, examples/*.rs",
            RUNTIME_INCLUDE_MARKER
        );
    }
    if matches.len() > 1 {
        let files: Vec<String> = matches.iter().map(|m| m.0.display().to_string()).collect();
        panic!(
            "Multiple runtime includes found ({}). There must be exactly one include!(...runtime.rs) per crate.",
            files.join(", ")
        );
    }

    let (path, source) = matches.remove(0);
    let ast = parse_file(&source).expect("Failed to parse runtime host file");
    (path, source, ast)
}

fn template_path(base: &Path, segments: &[String]) -> std::path::PathBuf {
    let mut path = base.to_path_buf();
    if !segments.is_empty() {
        for seg in &segments[..segments.len() - 1] {
            path.push(seg);
        }
        path.push(format!("{}.tea.rs", segments.last().unwrap()));
    }
    path
}

fn push_dep_template(dep: &DepInfo, segments: &[String], templates: &mut Vec<TemplateInfo>) {
    if segments.is_empty() {
        return;
    }

    let module_name = segments.last().unwrap();
    if !module_name
        .chars()
        .next()
        .map(|c| c.is_lowercase())
        .unwrap_or(false)
    {
        return;
    }

    let tea_path = template_path(&dep.path.join("src"), segments);
    if !tea_path.exists() {
        panic!(
            "Template `{}` declared via `use {}::{};` was not found at {}.",
            module_name,
            dep.module_name,
            segments.join("::"),
            tea_path.display()
        );
    }

    let rs_path = dep.path.join("src").join(format!("{}.rs", module_name));
    if rs_path.exists() {
        panic!(
            "Both {}.rs and {}.tea.rs exist in dependency `{}`. Remove the concrete .rs when providing a template.",
            module_name, module_name, dep.crate_name
        );
    }

    if !dep.has_templates {
        panic!(
            "Template {}.tea.rs found in dependency `{}` but it lacks `[package.metadata.saucer] has_templates = true`.",
            module_name, dep.crate_name
        );
    }

    if templates.iter().any(|t| t.path == tea_path) {
        return;
    }

    templates.push(TemplateInfo {
        module_name: module_name.clone(),
        crate_name: dep.crate_name.clone(),
        path: tea_path,
        is_root: false,
        used_helpers: Vec::new(),
        ports: Vec::new(),
    });
}

fn process_template_use_paths(
    paths: &[UsePath],
    deps: &HashMap<String, DepInfo>,
    templates: &mut Vec<TemplateInfo>,
) {
    for path in paths {
        if path.segments.is_empty() {
            continue;
        }

        let crate_name = &path.segments[0];
        let Some(dep) = deps.get(crate_name) else {
            // Not a dependency crate; treat as a normal import.
            continue;
        };

        if !dep.has_templates {
            // Dependency doesn't declare templates; treat as a normal import.
            continue;
        }

        if path.is_glob {
            panic!(
                "Glob imports are not allowed for template discovery. Import templates from `{}` explicitly (e.g., `use {}::some::template;`).",
                dep.crate_name,
                dep.module_name
            );
        }

        let relative_segments = &path.segments[1..];
        if relative_segments.is_empty() {
            panic!(
                "Template import `use {}::...` must include a template module path (e.g., `use {}::foo;`).",
                dep.crate_name,
                dep.module_name
            );
        }

        push_dep_template(dep, relative_segments, templates);
    }
}

fn collect_use_paths(tree: &UseTree, prefix: &mut Vec<String>, out: &mut Vec<UsePath>) {
    match tree {
        UseTree::Path(path) => {
            prefix.push(path.ident.to_string());
            collect_use_paths(&path.tree, prefix, out);
            prefix.pop();
        }
        UseTree::Name(name) => {
            let mut p = prefix.clone();
            p.push(name.ident.to_string());
            out.push(UsePath {
                segments: p,
                is_glob: false,
            });
        }
        UseTree::Rename(rename) => {
            let mut p = prefix.clone();
            p.push(rename.ident.to_string());
            out.push(UsePath {
                segments: p,
                is_glob: false,
            });
        }
        UseTree::Group(group) => {
            for item in &group.items {
                collect_use_paths(item, prefix, out);
            }
        }
        UseTree::Glob(_) => {
            out.push(UsePath {
                segments: prefix.clone(),
                is_glob: true,
            });
        }
    }
}

pub fn discover_templates_from_template(
    template: &TemplateInfo,
    deps: &HashMap<String, DepInfo>,
    templates: &mut Vec<TemplateInfo>,
) {
    let source = std::fs::read_to_string(&template.path)
        .unwrap_or_else(|e| panic!("Failed to read template {:?}: {}", template.path, e));
    let ast = parse_file(&source).expect("Failed to parse template for dependency discovery");

    for item in ast.items {
        if let Item::Use(use_item) = item {
            let mut paths = Vec::new();
            collect_use_paths(&use_item.tree, &mut Vec::new(), &mut paths);
            process_template_use_paths(&paths, deps, templates);
        }
    }
}

fn collect_template_paths(tree: &UseTree, prefix: &mut Vec<String>, out: &mut Vec<Vec<String>>) {
    match tree {
        UseTree::Name(name) => {
            let mut path = prefix.clone();
            path.push(name.ident.to_string());
            out.push(path);
        }
        UseTree::Rename(rename) => {
            let mut path = prefix.clone();
            path.push(rename.ident.to_string());
            out.push(path);
        }
        UseTree::Group(group) => {
            for item in &group.items {
                collect_template_paths(item, prefix, out);
            }
        }
        UseTree::Path(path) => {
            prefix.push(path.ident.to_string());
            collect_template_paths(&path.tree, prefix, out);
            prefix.pop();
        }
        UseTree::Glob(_) => {}
    }
}

fn push_template_for_path(
    path: &[String],
    src_dir: &Path,
    package_name: &str,
    has_templates_local: bool,
    deps: &[DepInfo],
    templates: &mut Vec<TemplateInfo>,
) {
    if path.is_empty() {
        return;
    }

    let module_name = path.last().unwrap().clone();
    if !module_name
        .chars()
        .next()
        .map(|c| c.is_lowercase())
        .unwrap_or(false)
    {
        return;
    }

    if path.len() < 2 {
        panic!(
            "Template imports must include a crate namespace: use runtime::<crate>::<template>. Got `runtime::{}`.",
            path.join("::")
        );
    }

    let mut crate_seg = path[0].clone();
    let mut module_segments: &[String] = &path[1..];

    // Allow paths like runtime::sync::mock_app::app by unwrapping the backend namespace.
    if crate_seg == "sync" || crate_seg == "async" {
        if module_segments.is_empty() {
            return;
        }
        crate_seg = module_segments[0].clone();
        module_segments = &module_segments[1..];
    }
    let package_module = crate_to_module_name(package_name);

    // Local crate namespace
    if crate_seg == package_module {
        let local_path = template_path(src_dir, module_segments);
        if !local_path.exists() {
            panic!(
                "Template {}.tea.rs not found locally in `{}` at {}.",
                module_segments.last().unwrap(),
                package_name,
                local_path.display()
            );
        }
        if !has_templates_local {
            panic!(
                "Template {}.tea.rs found in crate `{}` but `[package.metadata.saucer] has_templates = true` is missing.",
                module_segments.last().unwrap(), package_name
            );
        }
        let local_rs = src_dir.join(format!("{}.rs", module_segments.last().unwrap()));
        if local_rs.exists() {
            panic!(
                "Both {}.rs and {}.tea.rs exist in crate `{}`. Remove the concrete .rs when providing a template.",
                module_segments.last().unwrap(), module_segments.last().unwrap(), package_name
            );
        }
        if templates.iter().any(|t| t.path == local_path) {
            return;
        }
        templates.push(TemplateInfo {
            module_name: module_segments.last().unwrap().clone(),
            crate_name: package_name.to_string(),
            path: local_path,
            is_root: true,
            used_helpers: Vec::new(),
            ports: Vec::new(),
        });
        return;
    }

    // Dependency crate namespace (explicit crate required)
    let mut candidates: Vec<(&DepInfo, std::path::PathBuf)> = Vec::new();
    for dep in deps {
        if dep.module_name != crate_seg {
            continue;
        }

        let dep_template_path = template_path(&dep.path.join("src"), module_segments);
        if dep_template_path.exists() {
            candidates.push((dep, dep_template_path));
        }
    }

    match candidates.len() {
        0 => {
            panic!(
                "Template {}.tea.rs not found in dependency `{}` (or locally).",
                module_segments.last().unwrap(),
                crate_seg
            );
        }
        1 => {
            let (dep, dep_path) = candidates.into_iter().next().unwrap();
            if !dep.has_templates {
                panic!(
                    "Template {}.tea.rs found in dependency `{}` but it lacks `[package.metadata.saucer] has_templates = true`.",
                    module_segments.last().unwrap(), dep.crate_name
                );
            }
            let rs_path = dep
                .path
                .join("src")
                .join(format!("{}.rs", module_segments.last().unwrap()));
            if rs_path.exists() {
                panic!(
                    "Both {}.rs and {}.tea.rs exist in dependency `{}`. Remove the concrete .rs when providing a template.",
                    module_segments.last().unwrap(), module_segments.last().unwrap(), dep.crate_name
                );
            }
            if templates.iter().any(|t| t.path == dep_path) {
                return;
            }
            templates.push(TemplateInfo {
                module_name: module_segments.last().unwrap().clone(),
                crate_name: dep.crate_name.clone(),
                path: dep_path,
                is_root: true,
                used_helpers: Vec::new(),
                ports: Vec::new(),
            });
        }
        _ => {
            let names: Vec<String> = candidates
                .iter()
                .map(|(d, _)| d.crate_name.clone())
                .collect();
            panic!(
                "Template {}.tea.rs found in multiple dependencies: {}. Use an explicit crate hint (e.g., `use runtime::crate_name::{};`) to disambiguate.",
                module_segments.last().unwrap(),
                names.join(", "),
                module_segments.last().unwrap()
            );
        }
    }
}

fn extract_modules_from_tree(
    tree: &UseTree,
    src_dir: &Path,
    package_name: &str,
    has_templates_local: bool,
    deps: &[DepInfo],
    templates: &mut Vec<TemplateInfo>,
) {
    let mut paths: Vec<Vec<String>> = Vec::new();
    collect_template_paths(tree, &mut Vec::new(), &mut paths);

    for path in paths {
        push_template_for_path(
            &path,
            src_dir,
            package_name,
            has_templates_local,
            deps,
            templates,
        );
    }
}

fn extract_runtime_modules(
    tree: &UseTree,
    src_dir: &Path,
    package_name: &str,
    has_templates_local: bool,
    deps: &[DepInfo],
    templates: &mut Vec<TemplateInfo>,
) {
    match tree {
        UseTree::Path(path) => {
            if path.ident == "runtime" {
                // Found `use runtime::xxx` - extract module names
                extract_modules_from_tree(
                    &path.tree,
                    src_dir,
                    package_name,
                    has_templates_local,
                    deps,
                    templates,
                );
            } else {
                // Recurse to handle paths like crate::runtime::app
                extract_runtime_modules(
                    &path.tree,
                    src_dir,
                    package_name,
                    has_templates_local,
                    deps,
                    templates,
                );
            }
        }
        UseTree::Group(group) => {
            for tree in &group.items {
                extract_runtime_modules(
                    tree,
                    src_dir,
                    package_name,
                    has_templates_local,
                    deps,
                    templates,
                );
            }
        }
        _ => {}
    }
}

/// Discover templates from the file that includes the generated runtime by looking for
/// `use runtime::xxx` statements (local or dependency crates)
pub fn discover_templates(
    runtime_host_path: &Path,
    runtime_host_source: &str,
) -> Vec<TemplateInfo> {
    let ast = parse_file(runtime_host_source).expect("Failed to parse runtime host file");
    let mut templates = Vec::new();

    let src_dir = runtime_host_path
        .parent()
        .expect("runtime host should have parent dir");
    let cargo_toml = src_dir.parent().unwrap().join("Cargo.toml");
    let deps = dependency_infos(&cargo_toml);
    let has_templates_local = read_crate_metadata(&cargo_toml);
    let package_name = {
        let content = std::fs::read_to_string(&cargo_toml)
            .unwrap_or_else(|e| panic!("Failed to read Cargo.toml at {:?}: {}", cargo_toml, e));
        let toml: Value = content.parse().expect("Failed to parse Cargo.toml");
        toml.get("package")
            .and_then(|p| p.get("name"))
            .and_then(|n| n.as_str())
            .unwrap_or("app")
            .to_string()
    };

    for item in ast.items {
        if let Item::Use(use_item) = item {
            extract_runtime_modules(
                &use_item.tree,
                src_dir,
                &package_name,
                has_templates_local,
                &deps,
                &mut templates,
            );
        }
    }

    templates
}

/// Inspect main.rs to find which effect manager the reconciler references.
pub fn find_reconciler_manager_module(
    manifest_dir: &Path,
    managers: &[ManagerInfo],
) -> Option<String> {
    for path in candidate_runtime_files(manifest_dir) {
        let Ok(source) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(ast) = parse_file(&source) else {
            continue;
        };
        let use_aliases = collect_use_aliases(&ast);
        if let Some(runtime_call) = find_runtime_run_call_any(&ast) {
            let reconciler_arg = runtime_call.args.iter().nth(3)?;
            let segments = extract_signature_path(reconciler_arg)?;
            let crate_name = resolve_crate_from_path(&segments, &use_aliases)?;
            if let Some(m) = managers.iter().find(|m| m.module_name == crate_name) {
                return Some(m.module_name.clone());
            }
        }
    }
    None
}

fn collect_use_aliases(ast: &File) -> HashMap<String, Vec<String>> {
    let mut aliases = HashMap::new();
    for item in &ast.items {
        if let Item::Use(use_item) = item {
            collect_alias_from_tree(&use_item.tree, &mut Vec::new(), &mut aliases);
        }
    }
    aliases
}

fn collect_alias_from_tree(
    tree: &UseTree,
    prefix: &mut Vec<String>,
    aliases: &mut HashMap<String, Vec<String>>,
) {
    match tree {
        UseTree::Path(path) => {
            prefix.push(path.ident.to_string());
            collect_alias_from_tree(&path.tree, prefix, aliases);
            prefix.pop();
        }
        UseTree::Name(name) => {
            aliases.insert(name.ident.to_string(), prefix.clone());
        }
        UseTree::Rename(rename) => {
            aliases.insert(rename.rename.to_string(), prefix.clone());
        }
        UseTree::Group(group) => {
            for item in &group.items {
                collect_alias_from_tree(item, prefix, aliases);
            }
        }
        UseTree::Glob(_) => {}
    }
}

fn find_runtime_run_call_any(ast: &File) -> Option<&syn::ExprCall> {
    for item in &ast.items {
        if let Item::Fn(func) = item {
            if let Some(call) = find_runtime_call_in_block(&func.block) {
                return Some(call);
            }
        }
    }
    None
}

fn find_runtime_call_in_block(block: &Block) -> Option<&syn::ExprCall> {
    for stmt in &block.stmts {
        if let Some(call) = find_runtime_call_in_stmt(stmt) {
            return Some(call);
        }
    }
    None
}

fn find_runtime_call_in_stmt(stmt: &Stmt) -> Option<&syn::ExprCall> {
    match stmt {
        Stmt::Expr(expr, _) => find_runtime_call_in_expr(expr),
        Stmt::Local(local) => local
            .init
            .as_ref()
            .and_then(|init| find_runtime_call_in_expr(&init.expr)),
        _ => None,
    }
}

fn find_runtime_call_in_expr(expr: &Expr) -> Option<&syn::ExprCall> {
    if let Expr::Call(call) = expr {
        if is_runtime_ctor_or_run(&call.func) {
            return Some(call);
        }
    }

    match expr {
        Expr::Block(block) => find_runtime_call_in_block(&block.block),
        Expr::Paren(paren) => find_runtime_call_in_expr(&paren.expr),
        Expr::Await(await_expr) => find_runtime_call_in_expr(&await_expr.base),
        Expr::MethodCall(mc) => {
            if mc.method == "run" && mc.args.is_empty() {
                // Method call: receiver.run()
                return None; // reconciler not obtainable here
            }
            for arg in &mc.args {
                if let Some(call) = find_runtime_call_in_expr(arg) {
                    return Some(call);
                }
            }
            find_runtime_call_in_expr(&mc.receiver)
        }
        Expr::If(e) => {
            if let Some(call) = find_runtime_call_in_expr(&e.cond) {
                return Some(call);
            }
            if let Some(call) = find_runtime_call_in_block(&e.then_branch) {
                return Some(call);
            }
            if let Some((_, else_expr)) = &e.else_branch {
                return find_runtime_call_in_expr(else_expr);
            }
            None
        }
        Expr::Match(m) => {
            if let Some(call) = find_runtime_call_in_expr(&m.expr) {
                return Some(call);
            }
            for arm in &m.arms {
                if let Some(call) = find_runtime_call_in_expr(&arm.body) {
                    return Some(call);
                }
            }
            None
        }
        Expr::Let(le) => find_runtime_call_in_expr(&le.expr),
        Expr::Tuple(t) => t.elems.iter().find_map(find_runtime_call_in_expr),
        Expr::Array(a) => a.elems.iter().find_map(find_runtime_call_in_expr),
        _ => None,
    }
}

fn is_runtime_ctor_or_run(func: &Expr) -> bool {
    if let Expr::Path(path) = func {
        let segments = path
            .path
            .segments
            .iter()
            .map(|seg| seg.ident.to_string())
            .collect::<Vec<_>>();
        if segments.len() >= 2 && matches!(segments.last().map(String::as_str), Some("run" | "new"))
        {
            if segments[segments.len() - 2] == "Runtime" {
                return true;
            }
        }
    }
    false
}

fn extract_signature_path(expr: &Expr) -> Option<Vec<String>> {
    match expr {
        Expr::Call(call) => extract_signature_path(&call.func),
        Expr::Path(path) => Some(
            path.path
                .segments
                .iter()
                .map(|seg| seg.ident.to_string())
                .collect(),
        ),
        Expr::Paren(paren) => extract_signature_path(&paren.expr),
        _ => None,
    }
}

fn resolve_crate_from_path(
    segments: &[String],
    aliases: &HashMap<String, Vec<String>>,
) -> Option<String> {
    if segments.is_empty() {
        return None;
    }

    if segments.len() == 1 {
        aliases
            .get(&segments[0])
            .and_then(|path| path.first().cloned())
    } else {
        let first = &segments[0];
        if let Some(path) = aliases.get(first) {
            path.first().cloned()
        } else {
            Some(first.clone())
        }
    }
}

/// Transform a .tea.rs template by rewriting fictional imports
///
/// Uses line-by-line text processing to avoid needing span locations.
pub struct TemplateTransform {
    pub code: String,
    pub used_helpers: Vec<(String, String)>,
}

fn is_manager_request_import(line: &str, managers: &[ManagerInfo]) -> bool {
    if !line.starts_with("use ") || !line.contains("::") {
        return false;
    }
    let without_use = line.trim_start_matches("use ").trim_end_matches(';');
    // use mock_chat::ChatRequest
    let mut parts = without_use.split("::");
    let crate_seg = match parts.next() {
        Some(s) => s,
        None => return false,
    };
    let type_seg = match parts.last() {
        Some(s) => s,
        None => return false,
    };
    managers
        .iter()
        .any(|m| m.module_name == crate_seg && m.request_type == type_seg)
}

pub fn transform_template(
    template_path: &Path,
    managers: &[ManagerInfo],
    dep_templates: &[(String, String)],
    template_ports: &[PortSpec],
) -> TemplateTransform {
    let source = std::fs::read_to_string(template_path)
        .unwrap_or_else(|e| panic!("Failed to read template {:?}: {}", template_path, e));

    // First pass: drop outgoing #[port] functions (so generated helpers win) and strip the attribute from incoming ones.
    let file_ast: syn::File = syn::parse_str(&source)
        .unwrap_or_else(|e| panic!("Failed to parse template {:?}: {}", template_path, e));
    let mut cleaned_items = Vec::new();
    for item in file_ast.items {
        if let Item::Fn(func) = &item {
            let has_port_attr = func.attrs.iter().any(|a| a.path().is_ident("port"));
            if has_port_attr {
                let name = func.sig.ident.to_string();
                let direction = template_ports
                    .iter()
                    .find_map(|p| match p {
                        PortSpec::Incoming(p) if p.name == name => Some(PortDirection::Incoming),
                        PortSpec::Outgoing(p) if p.name == name => Some(PortDirection::Outgoing),
                        _ => None,
                    })
                    .unwrap_or_else(|| panic!("Port `{}` not tracked during parsing", name));
                if matches!(direction, PortDirection::Outgoing) {
                    continue; // remove outgoing declaration
                } else {
                    let mut func = func.clone();
                    func.attrs = func
                        .attrs
                        .iter()
                        .filter(|a| !a.path().is_ident("port"))
                        .cloned()
                        .collect();
                    // Incoming: force return type to Msg so Sub is stripped from generated code.
                    let msg_ty: syn::Type = syn::parse_str("Msg").unwrap();
                    func.sig.output = syn::ReturnType::Type(RArrow::default(), Box::new(msg_ty));
                    cleaned_items.push(Item::Fn(func));
                    continue;
                }
            }
        }
        cleaned_items.push(item);
    }

    let cleaned_file = syn::File {
        shebang: None,
        attrs: Vec::new(),
        items: cleaned_items,
    };
    let cleaned_source = prettyplease::unparse(&cleaned_file);

    let mut result = String::new();
    let mut used_helpers = Vec::new();

    for line in cleaned_source.lines() {
        let trimmed = line.trim();

        // Hygiene: block direct imports of core plumbing that would bypass helper permissions.
        if trimmed.starts_with("use saucer_core::CoreCmd")
            || trimmed.starts_with("use saucer_core::CoreRequest")
        {
            panic!(
                "Template {:?} imports saucer_core::CoreCmd/CoreRequest directly. Use fictional imports (saucer_core::Cmd, <manager>::command::helper) so tooling and permissions stay enforced.",
                template_path
            );
        }

        if trimmed.starts_with("use runtime::Request")
            || trimmed.starts_with("use super::Request")
            || trimmed.starts_with("use super::super::Request")
        {
            panic!(
                "Template {:?} imports the generated Request type directly. Construct commands via the generated helpers instead of Request/Request::map.",
                template_path
            );
        }

        // Optional hygiene: block direct imports of manager Request types to force helper usage.
        if is_manager_request_import(trimmed, managers) {
            panic!(
                "Template {:?} imports an effect manager Request type directly. Use fictional ::command:: helpers instead so permissions and tooling apply.",
                template_path
            );
        }

        // Ignore template-only port helper imports
        if trimmed.starts_with("use saucer_core::port") {
            continue;
        }

        // Check for fictional imports and transform them
        if let Some((transformed, helper_use)) = transform_line(trimmed, managers, dep_templates) {
            // Preserve leading whitespace
            let leading_ws: String = line.chars().take_while(|c| c.is_whitespace()).collect();
            result.push_str(&leading_ws);
            result.push_str(&transformed);
            if let Some((manager, helper)) = helper_use {
                used_helpers.push((manager, helper));
            }
        } else {
            if trimmed.starts_with("use ") && trimmed.contains("::command::") {
                panic!(
                    "Unrecognized fictional import in template {:?}: `{}`\n\
                     Ensure the crate is registered as an effect manager and exposes the helper in its requests module.",
                    template_path, trimmed
                );
            }
            result.push_str(line);
        }
        result.push('\n');
    }

    TemplateTransform {
        code: result,
        used_helpers,
    }
}

fn transform_line(
    line: &str,
    managers: &[ManagerInfo],
    dep_templates: &[(String, String)],
) -> Option<(String, Option<(String, String)>)> {
    // Skip if not a use statement
    if !line.starts_with("use ") {
        return None;
    }

    // Check for saucer_core::Cmd
    if line.starts_with("use saucer_core::Cmd") {
        return Some(("use super::super::Cmd;".to_string(), None));
    }
    if line.starts_with("use saucer_core::{Cmd, Sub}")
        || line.starts_with("use saucer_core::{Cmd,Sub}")
    {
        return Some(("use super::super::Cmd;".to_string(), None));
    }

    // Check for saucer_core::command::xxx
    if line.starts_with("use saucer_core::command::") {
        let rest = line.strip_prefix("use saucer_core::command::")?;
        let name = rest.trim_end_matches(';').trim();
        return Some((
            format!("use super::super::core::{};", name),
            Some(("saucer_core".to_string(), name.to_string())),
        ));
    }

    // Check for manager crate ::command:: patterns
    for manager in managers {
        let prefix = format!("use {}::command::", manager.module_name);
        if line.starts_with(&prefix) {
            let rest = line.strip_prefix(&prefix)?;
            let name = rest.trim_end_matches(';').trim();
            return Some((
                format!("use super::super::{}::{};", manager.module_name, name),
                Some((manager.module_name.clone(), name.to_string())),
            ));
        }
    }

    // Check for template crate imports (cross-crate templates)
    if line.starts_with("use ") && line.contains("::") {
        let without_use = line.trim_start_matches("use ").trim_end_matches(';');
        let parts = without_use.split("::").collect::<Vec<_>>();
        if parts.len() >= 2 {
            let crate_name = parts[0];
            let module_path = parts[1..].join("::");
            let module = *parts.last().unwrap();
            if dep_templates
                .iter()
                .any(|(c, m)| c == crate_name && m == module)
            {
                return Some((
                    format!("use super::super::{}::{};", crate_name, module_path),
                    None,
                ));
            }
        }
    }

    None
}

// ===== Helper utilities used outside the module =====

pub fn package_name(cargo_toml_path: &Path) -> String {
    let content = std::fs::read_to_string(cargo_toml_path)
        .unwrap_or_else(|e| panic!("Failed to read Cargo.toml at {:?}: {}", cargo_toml_path, e));
    let toml: Value = content.parse().expect("Failed to parse Cargo.toml");
    toml.get("package")
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or("app")
        .to_string()
}
