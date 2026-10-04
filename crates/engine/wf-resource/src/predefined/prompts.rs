use crate::embedded_assets;
use crate::registry::{register_template, RegisterOptions, ResourceRegistries};
use crate::result::Summary;

/// Borrowed view of the built-in prompt templates; callers needing owned
/// values clone individual items instead of the whole asset.
pub fn builtin_prompts() -> &'static [wf_types::Template] {
    embedded_assets::prompt_templates()
}

pub fn register(regs: &ResourceRegistries, opts: &RegisterOptions) -> Summary {
    let mut total = Summary::new();
    for prompt in builtin_prompts() {
        total.merge(register_template(regs, prompt.clone(), opts.skip_if_exists));
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_prompts_expose_template_fields() {
        let prompts = builtin_prompts();
        assert!(!prompts.is_empty());
        for prompt in prompts {
            assert!(!prompt.id.trim().is_empty());
            assert!(!prompt.content.trim().is_empty());
        }
    }
}
