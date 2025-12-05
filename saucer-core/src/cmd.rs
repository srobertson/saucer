/// Generic command container - holds a batch of requests.
/// The request type `Req` is provided by the app's generated code.
pub struct CoreCmd<Req>(pub Vec<Req>);

impl<Req> CoreCmd<Req> {
    /// No-op command - produces no effects
    pub fn none() -> Self {
        CoreCmd(Vec::new())
    }

    /// Single request
    pub fn single(req: Req) -> Self {
        CoreCmd(vec![req])
    }

    /// Batch multiple commands together
    pub fn batch(cmds: impl IntoIterator<Item = CoreCmd<Req>>) -> Self {
        let mut all = Vec::new();
        for CoreCmd(mut v) in cmds {
            all.append(&mut v);
        }
        CoreCmd(all)
    }

    /// Access inner requests (for runtime dispatch)
    pub fn into_inner(self) -> Vec<Req> {
        self.0
    }
}

impl<Req> Default for CoreCmd<Req> {
    fn default() -> Self {
        Self::none()
    }
}
