pub mod compose;
pub mod custom;
pub mod dynamic;
pub mod predefined;
pub mod registrar;
pub mod result;
pub mod starter;

pub use compose::Config as PromptComposition;
pub use custom::{
    load_custom_prompts, load_custom_resources, load_custom_tools, load_custom_triggers,
    register_custom_prompts, register_custom_resources, register_custom_tools,
    register_custom_triggers, CustomResources, CustomResourcesPresetConfig, CustomValidationLevel,
};
pub use dynamic::{build_system_context, build_user_context, SystemConfig, UserInput};
pub use predefined::builder::{
    build_minimal_system_prompt, build_system_prompt, BuildOptions, PromptType,
};
pub use predefined::render::{render_tool_descriptions, ToolFormat};
pub use predefined::starters::{GoalReviewConfig, GoalReviewStarter};
pub use registrar::{
    are_fragments_registered, are_predefined_tool_descriptions_registered,
    are_prompt_templates_registered, is_resource_disabled, register_all, register_item,
    unregister_predefined_content, Options, Registries, StarterActivation,
};
pub use result::Summary;
pub use starter::{
    Bundle, BundleRegistry, Starter, StarterConfigField, StarterConfigFieldType, StarterMetadata,
};
