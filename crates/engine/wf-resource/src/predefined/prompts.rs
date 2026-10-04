use serde::Deserialize;
use wf_types::Template;

use crate::registry::{register_template, RegisterOptions, ResourceRegistries};
use crate::result::Summary;

#[derive(Debug, Deserialize)]
struct PromptsFile {
    prompts: Vec<Template>,
}

fn embedded_prompts() -> PromptsFile {
    serde_json::from_str(include_str!("../../configs/prompts.json"))
        .expect("embedded prompts.json is valid")
}

pub fn builtin_prompts() -> Vec<Template> {
    embedded_prompts().prompts
}

pub fn register(regs: &ResourceRegistries, opts: &RegisterOptions) -> Summary {
    let mut total = Summary::new();
    for prompt in builtin_prompts() {
        total.merge(register_template(regs, prompt, opts.skip_if_exists));
    }
    total
}
