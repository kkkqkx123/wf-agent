pub mod compose;
pub mod custom;
pub mod dynamic;
pub mod predefined;
pub mod registry;
pub mod resource_plugin;
pub mod result;
pub mod template;

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
pub use predefined::resource_plugin::{GoalReviewConfig, GoalReviewResourcePlugin};
pub use predefined::tool_visibility::{
    builtin_tool_visibility_templates, ACTIVATION_TEMPLATE_ID, BLOCK_TEMPLATE_ID,
    DISCOVERABLE_METADATA_TEMPLATE_ID, GENERAL_DESCRIPTION_TEMPLATE_ID,
};
pub use registry::{
    are_fragments_registered, are_predefined_tool_descriptions_registered,
    are_prompt_templates_registered, is_resource_disabled, list_fragments_by_category,
    list_templates_by_category, register_all, register_fragment, register_item_skip,
    register_item_strict, register_template, templates_depending_on_fragment,
    unregister_fragment_checked, unregister_predefined_content, unregister_template,
    RegisterOptions, ResourcePluginActivation, ResourceRegistries,
};
pub use resource_plugin::{
    ResourceBundle, ResourcePlugin, ResourcePluginConfigField, ResourcePluginConfigFieldType,
    ResourcePluginMetadata, ResourcePluginRegistry,
};
pub use result::Summary;
pub use template::{
    builtin_default, render_template, render_visibility_message, TemplateRenderOptions,
};
