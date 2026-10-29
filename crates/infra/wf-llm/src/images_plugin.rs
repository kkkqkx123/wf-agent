//! Bridge a plugin-contributed [`PluginLlmImages`] onto the host image
//! generation surface.
//!
//! The plugin contract is synchronous and JSON-based like the codec/auth
//! contracts: `context` carries the resolved endpoint plus credential,
//! `options` carries the per-call knobs, and the result follows the host
//! `ImagesResult` JSON shape. The host owns the async boundary; the plugin
//! owns the protocol. Network implementations must run on the native
//! backend, like `llm-auth` (Lua/Wasm sandboxes have no HTTP).

use std::sync::Arc;

use wf_plugin_sdk::PluginLlmImages;

use llm_images::{ImageGenerationProvider, ImagesContext, ImagesOptions, ImagesResult};

/// Adapt a plugin-contributed images implementation to the host
/// [`ImageGenerationProvider`].
///
/// The images context is bound at construction (mirroring
/// `OpenAiCompatibleImagesProvider`); each call serializes the bound
/// context plus the per-call options across the JSON boundary and
/// deserializes the `ImagesResult` answer.
pub struct PluginImagesAdapter {
    images: Arc<dyn PluginLlmImages>,
    context: ImagesContext,
}

impl PluginImagesAdapter {
    /// Wrap one plugin images implementation with its endpoint context.
    pub fn new(images: Arc<dyn PluginLlmImages>, context: ImagesContext) -> Self {
        Self { images, context }
    }

    /// The plugin implementation behind this adapter.
    pub fn images(&self) -> &Arc<dyn PluginLlmImages> {
        &self.images
    }

    /// The bound endpoint context serialized on every call.
    pub fn context(&self) -> &ImagesContext {
        &self.context
    }
}

impl std::fmt::Debug for PluginImagesAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginImagesAdapter")
            .field("model", &self.context.model)
            .field("base_url", &self.context.base_url)
            .finish_non_exhaustive()
    }
}

fn plugin_error(error: wf_plugin_sdk::PluginError) -> llm_images::ImageGenerationError {
    llm_images::ImageGenerationError::Transport(error.to_string())
}

#[async_trait::async_trait]
impl ImageGenerationProvider for PluginImagesAdapter {
    async fn generate_images(
        &self,
        prompt: &str,
        options: &ImagesOptions,
    ) -> llm_images::Result<ImagesResult> {
        if prompt.is_empty() {
            return Err(llm_images::ImageGenerationError::InvalidRequest(
                "prompt is required".to_string(),
            ));
        }
        let context = serde_json::to_value(&self.context).map_err(|e| {
            llm_images::ImageGenerationError::Config(format!("images context: {e}"))
        })?;
        let options = serde_json::to_value(options).map_err(|e| {
            llm_images::ImageGenerationError::Config(format!("images options: {e}"))
        })?;
        let answer = self
            .images
            .generate_images(context, prompt.to_string(), options)
            .map_err(plugin_error)?;
        serde_json::from_value(answer).map_err(|e| {
            llm_images::ImageGenerationError::Decode(format!("plugin images result: {e}"))
        })
    }

    fn model_name(&self) -> &str {
        &self.context.model
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wf_plugin_sdk::{PluginError, PluginResult};

    struct EchoImages;

    impl PluginLlmImages for EchoImages {
        fn name(&self) -> String {
            "echo images".to_string()
        }

        fn generate_images(
            &self,
            _context: serde_json::Value,
            prompt: String,
            _options: serde_json::Value,
        ) -> PluginResult<serde_json::Value> {
            Ok(json!({
                "images": [{"url": format!("mock://{prompt}")}],
                "created": serde_json::Value::Null,
            }))
        }
    }

    struct FailingImages;

    impl PluginLlmImages for FailingImages {
        fn name(&self) -> String {
            "failing images".to_string()
        }

        fn generate_images(
            &self,
            _context: serde_json::Value,
            _prompt: String,
            _options: serde_json::Value,
        ) -> PluginResult<serde_json::Value> {
            Err(PluginError::Internal("boom".to_string()))
        }
    }

    fn context() -> ImagesContext {
        ImagesContext::new("https://api.acme.test/v1/images/generations", "acme-images")
    }

    #[tokio::test]
    async fn adapter_round_trips_plugin_answer() {
        let adapter = PluginImagesAdapter::new(Arc::new(EchoImages), context());
        let result = adapter
            .generate_images("a cat", &ImagesOptions::new())
            .await
            .expect("generate");
        assert_eq!(result.images.len(), 1);
        assert_eq!(result.images[0].url.as_deref(), Some("mock://a cat"));
        assert_eq!(adapter.model_name(), "acme-images");
    }

    #[tokio::test]
    async fn empty_prompt_fails_before_plugin() {
        let adapter = PluginImagesAdapter::new(Arc::new(EchoImages), context());
        let err = adapter
            .generate_images("", &ImagesOptions::new())
            .await
            .expect_err("must fail");
        assert!(err.to_string().contains("prompt is required"));
    }

    #[tokio::test]
    async fn plugin_error_surfaces_as_transport() {
        let adapter = PluginImagesAdapter::new(Arc::new(FailingImages), context());
        let err = adapter
            .generate_images("a cat", &ImagesOptions::new())
            .await
            .expect_err("must fail");
        assert!(err.to_string().contains("boom"));
    }
}
