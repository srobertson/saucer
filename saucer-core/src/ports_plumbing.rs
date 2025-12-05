/// Marker type used in templates to declare incoming ports (host → app).
/// Purely a build-script signal; not used at runtime.
pub struct Sub<Msg>(pub std::marker::PhantomData<Msg>);

impl<Msg> Sub<Msg> {
    pub fn new() -> Self {
        Sub(std::marker::PhantomData)
    }
}

// Note: runtime plumbing is generated per-port in build.rs; this module only
// carries the Sub marker used during parsing/templating.
