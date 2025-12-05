use proc_macro2::TokenStream;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortDirection {
    Incoming, // host -> app (Sub<Msg>)
    Outgoing, // app -> host (Cmd<Msg>)
}

#[derive(Debug, Clone)]
pub struct IncomingPort {
    pub name: String,
    pub args: Vec<(String, String)>, // (ident, type)
}

#[derive(Debug, Clone)]
pub struct OutgoingPort {
    pub name: String,
    pub args: Vec<(String, String)>, // (ident, type)
}

#[derive(Debug, Clone)]
pub enum PortSpec {
    Incoming(IncomingPort),
    Outgoing(OutgoingPort),
}

/// Manager info extracted from Cargo.toml metadata
#[derive(Debug, Clone)]
pub struct ManagerInfo {
    /// Crate name (e.g., "mock-time")
    pub crate_name: String,
    /// Module name (e.g., "mock_time")
    pub module_name: String,
    /// Variant name in Request enum (e.g., "Time")
    pub variant: String,
    /// Request type (e.g., "TimeRequest")
    pub request_type: String,
    /// Manager type (e.g., "TimeManager")
    pub manager_type: String,
    /// Self message type (e.g., "()" or "ChatManagerMsg")
    pub self_msg_type: String,
    /// Path to dependency's lib.rs
    pub lib_path: std::path::PathBuf,
}

/// Template info discovered from main.rs or transitive templates
#[derive(Debug, Clone)]
pub struct TemplateInfo {
    /// Module name (e.g., "app")
    pub module_name: String,
    /// Crate name providing the template
    pub crate_name: String,
    /// Path to .tea.rs file
    pub path: std::path::PathBuf,
    /// Whether this template was imported directly from main.rs (root templates may declare ports)
    pub is_root: bool,
    /// Fictional helpers used by this template: (manager_module, helper_name)
    pub used_helpers: Vec<(String, String)>,
    /// Ports parsed from this template (only captured for root templates)
    pub ports: Vec<PortSpec>,
}

#[derive(Debug, Clone)]
pub struct RuntimeSpec {
    pub package_name: String,
    pub managers: Vec<ManagerInfo>,
    pub effect_managers: Vec<ManagerInfo>,
    pub reconciler_manager: ManagerInfo,
    pub templates: Vec<TemplateInfo>,
    pub transformed_templates: Vec<(String, String)>,
    pub used_helpers: Vec<(String, String)>,
    pub all_ports: Vec<PortSpec>,
    pub ports_with_paths: Vec<(PortSpec, proc_macro2::Ident, proc_macro2::Ident)>,
    pub app_msg_path: TokenStream,
    pub has_outgoing_ports: bool,
}
