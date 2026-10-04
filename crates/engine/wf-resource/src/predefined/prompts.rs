use crate::embedded_assets;
use crate::registry::{register_template, RegisterOptions, ResourceRegistries};
use crate::result::Summary;

pub fn builtin_prompts() -> Vec<wf_types::Template> {
    embedded_assets::prompt_templates().to_vec()
}

pub fn register(regs: &ResourceRegistries, opts: &RegisterOptions) -> Summary {
    let mut total = Summary::new();
    for prompt in builtin_prompts() {
        total.merge(register_template(regs, prompt, opts.skip_if_exists));
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
        for prompt in &prompts {
            assert!(!prompt.id.trim().is_empty());
            assert!(!prompt.content.trim().is_empty());
        }
    }
}
