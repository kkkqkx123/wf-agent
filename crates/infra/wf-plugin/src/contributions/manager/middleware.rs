use super::*;

impl ContributionManager {
    pub fn get_middleware(
        &self,
        phase: &MiddlewarePhase,
    ) -> Vec<(i32, Arc<dyn PluginMiddlewareHandler>)> {
        let mut handlers = self.middleware_registry.get(phase.as_str());
        // Higher priority runs first (project-wide ordering convention).
        handlers.sort_by_key(|(p, _)| std::cmp::Reverse(*p));
        handlers
    }

    pub fn all_middleware_phases(&self) -> Vec<String> {
        self.middleware_registry.keys()
    }

    /// All middleware registrations as `(phase, plugin_id)` pairs.
    pub fn all_middleware(&self) -> Vec<(String, String)> {
        self.middleware_registry.all()
    }

    /// Run middleware handlers for `phase` as an onion chain threaded on
    /// one context value. Higher priority runs outermost; each handler
    /// receives the context as rewritten by its outer neighbors. The
    /// returned value is the final context: handlers that skip `next`
    /// short-circuit the chain with whatever they return, and handlers
    /// may rewrite the value coming back from `next` (response rewrite).
    pub async fn run_middleware(
        &self,
        phase: &MiddlewarePhase,
        context: Value,
    ) -> PluginResult<Value> {
        let handlers = self.get_middleware(phase);

        let mut next: NextFn = Box::new(|ctx| Box::pin(async move { Ok(ctx) }));

        for (_, handler) in handlers.into_iter().rev() {
            let prev = next;
            next = Box::new(move |ctx| {
                let h = handler;
                let p = prev;
                Box::pin(async move { h.handle(ctx, p).await })
            });
        }

        next(context).await
    }
}
