//! Built-in provider connection templates (host-owned).
//!
//! These are static connection baselines for the well-known providers:
//! base URL, wire format, auth type and discovery mode. They carry no
//! model rows and no prices — model metadata comes from the host pricing
//! table (`wf-pricing`, derived from LiteLLM), live discovery fills the
//! rest. Registration reuses the existing `ProviderDefinitionRegistry`;
//! file-layer definitions registered afterwards win on id collision.
//!
//! Coverage mirrors `ref/pi/packages/ai/src/providers/` (all static chat
//! providers), folded onto the four built-in wire formats plus one
//! plugin-owned custom format:
//!
//! - OpenAI-compatible chat (`OpenaiChat`) is the default. Providers pi
//!   flags with a legacy `max_tokens` field (DeepSeek, Moonshot, Z.AI,
//!   Together, NVIDIA, Ant Ling, OpenCode, Cloudflare gateways) carry an
//!   explicit `compat.max_tokens_field` instead of relying on detection.
//! - Anthropic-protocol vendors (MiniMax, Vercel AI Gateway) use
//!   `Anthropic` with `x-api-key`, matching pi's SDK header behavior.
//! - Responses-protocol vendors (Meta, xAI, OpenAI Codex, OpenAI
//!   Responses) use `OpenaiResponse`.
//! - Endpoint-configured vendors (Azure, Bedrock, Cloudflare, Vertex)
//!   leave `base_url` empty: the caller supplies the deployment, region or
//!   account-specific URL. Bedrock additionally uses a `Custom` format
//!   because SigV4 signing needs a plugin codec; every other entry works
//!   with the built-in codecs.
//!
//! OpenCode (`opencode` / `opencode-go`) is the only vendor with a
//! mandatory static request header: pi's `withOpenCodeSessionHeader`
//! sends `x-opencode-client` (attribution) on every request plus a
//! per-conversation `x-opencode-session` routing header. The static half
//! lives in `default_headers` below; the session half is per-conversation
//! by definition, so callers must pass it through per-request
//! `LlmRequest.extra_headers` (this crate has no session field on the
//! profile, and stuffing a dynamic value into a static template would
//! leak conversations into each other).

use llm_types::llm::generation::LlmMaxTokenField;
use llm_types::llm::{LlmFormat, LlmProviderDefinition, ModelDiscovery, ProviderCompat};

use llm_codec::error::LlmResult;
use llm_config::provider::ProviderDefinitionRegistry;

/// Built-in provider definition ids. Stable strings hosts can rely on.
pub const BUILTIN_OPENAI_ID: &str = "builtin-openai";
pub const BUILTIN_ANTHROPIC_ID: &str = "builtin-anthropic";
pub const BUILTIN_GEMINI_ID: &str = "builtin-gemini";
pub const BUILTIN_OLLAMA_ID: &str = "builtin-ollama";
pub const BUILTIN_OPENROUTER_ID: &str = "builtin-openrouter";
pub const BUILTIN_OPENAI_RESPONSES_ID: &str = "builtin-openai-responses";
pub const BUILTIN_ANT_LING_ID: &str = "builtin-ant-ling";
pub const BUILTIN_BASETEN_ID: &str = "builtin-baseten";
pub const BUILTIN_CEREBRAS_ID: &str = "builtin-cerebras";
pub const BUILTIN_DEEPSEEK_ID: &str = "builtin-deepseek";
pub const BUILTIN_FIREWORKS_ID: &str = "builtin-fireworks";
pub const BUILTIN_GITHUB_COPILOT_ID: &str = "builtin-github-copilot";
pub const BUILTIN_GROQ_ID: &str = "builtin-groq";
pub const BUILTIN_HUGGINGFACE_ID: &str = "builtin-huggingface";
pub const BUILTIN_KIMI_CODING_ID: &str = "builtin-kimi-coding";
pub const BUILTIN_MISTRAL_ID: &str = "builtin-mistral";
pub const BUILTIN_MOONSHOTAI_ID: &str = "builtin-moonshotai";
pub const BUILTIN_MOONSHOTAI_CN_ID: &str = "builtin-moonshotai-cn";
pub const BUILTIN_NVIDIA_ID: &str = "builtin-nvidia";
pub const BUILTIN_QWEN_TOKEN_PLAN_ID: &str = "builtin-qwen-token-plan";
pub const BUILTIN_QWEN_TOKEN_PLAN_CN_ID: &str = "builtin-qwen-token-plan-cn";
pub const BUILTIN_QWEN_TOKEN_PLAN_INDIVIDUAL_ID: &str = "builtin-qwen-token-plan-individual";
pub const BUILTIN_TOGETHER_ID: &str = "builtin-together";
pub const BUILTIN_XIAOMI_ID: &str = "builtin-xiaomi";
pub const BUILTIN_XIAOMI_TOKEN_PLAN_AMS_ID: &str = "builtin-xiaomi-token-plan-ams";
pub const BUILTIN_XIAOMI_TOKEN_PLAN_CN_ID: &str = "builtin-xiaomi-token-plan-cn";
pub const BUILTIN_XIAOMI_TOKEN_PLAN_SGP_ID: &str = "builtin-xiaomi-token-plan-sgp";
pub const BUILTIN_ZAI_ID: &str = "builtin-zai";
pub const BUILTIN_ZAI_CODING_CN_ID: &str = "builtin-zai-coding-cn";
pub const BUILTIN_OPENCODE_ID: &str = "builtin-opencode";
pub const BUILTIN_OPENCODE_GO_ID: &str = "builtin-opencode-go";
pub const BUILTIN_AZURE_ID: &str = "builtin-azure";
pub const BUILTIN_CLOUDFLARE_AI_GATEWAY_ID: &str = "builtin-cloudflare-ai-gateway";
pub const BUILTIN_CLOUDFLARE_WORKERS_AI_ID: &str = "builtin-cloudflare-workers-ai";
pub const BUILTIN_MINIMAX_ID: &str = "builtin-minimax";
pub const BUILTIN_MINIMAX_CN_ID: &str = "builtin-minimax-cn";
pub const BUILTIN_VERCEL_AI_GATEWAY_ID: &str = "builtin-vercel-ai-gateway";
pub const BUILTIN_META_ID: &str = "builtin-meta";
pub const BUILTIN_XAI_ID: &str = "builtin-xai";
pub const BUILTIN_OPENAI_CODEX_ID: &str = "builtin-openai-codex";
pub const BUILTIN_GOOGLE_VERTEX_ID: &str = "builtin-google-vertex";
pub const BUILTIN_AMAZON_BEDROCK_ID: &str = "builtin-amazon-bedrock";

/// Static attribution header OpenCode requires on every request.
/// Mirrors pi's `x-opencode-client` attribution for `opencode` targets.
pub const OPENCODE_CLIENT_HEADER: &str = "x-opencode-client";
/// Per-conversation routing header OpenCode requires. Dynamic by
/// definition: callers pass it per request via `LlmRequest.extra_headers`,
/// mirroring pi's `withOpenCodeSessionHeader` (`x-opencode-session`).
pub const OPENCODE_SESSION_HEADER: &str = "x-opencode-session";
/// Value sent as `x-opencode-client` by the built-in OpenCode templates.
pub const OPENCODE_CLIENT_NAME: &str = "wf-agent";

/// Narrow legacy-field compat: the endpoint expects `max_tokens`.
fn legacy_max_tokens_compat() -> ProviderCompat {
    ProviderCompat {
        max_tokens_field: Some(LlmMaxTokenField::MaxTokens),
        ..Default::default()
    }
}

/// Discovery mode for a template: probe `/models` when a base URL is
/// known, otherwise leave discovery to explicit configuration.
fn discovery_for(base_url: Option<&str>) -> ModelDiscovery {
    match base_url {
        Some(_) => ModelDiscovery::ModelsEndpoint {
            path: None,
            json_path: None,
        },
        None => ModelDiscovery::Disabled,
    }
}

/// Standard OpenAI-compatible chat template with bearer auth.
fn openai_chat_template(
    id: &str,
    name: &str,
    description: &str,
    base_url: Option<&str>,
    legacy_max_tokens: bool,
) -> LlmProviderDefinition {
    LlmProviderDefinition {
        id: id.to_string(),
        name: Some(name.to_string()),
        description: Some(description.to_string()),
        base_url: base_url.map(str::to_string),
        auth_type: Some("bearer".to_string()),
        default_headers: None,
        format: LlmFormat::OpenaiChat,
        model_discovery: Some(discovery_for(base_url)),
        api_version: None,
        metadata: None,
        proxy: None,
        no_proxy: None,
        rate_limit: None,
        auth: None,
        compat: legacy_max_tokens.then(legacy_max_tokens_compat),
        identity: None,
        transport: None,
        env: None,
    }
}

/// OpenAI Responses-protocol template with bearer auth.
fn openai_responses_template(
    id: &str,
    name: &str,
    description: &str,
    base_url: Option<&str>,
) -> LlmProviderDefinition {
    LlmProviderDefinition {
        id: id.to_string(),
        name: Some(name.to_string()),
        description: Some(description.to_string()),
        base_url: base_url.map(str::to_string),
        auth_type: Some("bearer".to_string()),
        default_headers: None,
        format: LlmFormat::OpenaiResponse,
        model_discovery: Some(discovery_for(base_url)),
        api_version: None,
        metadata: None,
        proxy: None,
        no_proxy: None,
        rate_limit: None,
        auth: None,
        compat: None,
        identity: None,
        transport: None,
        env: None,
    }
}

/// Anthropic-protocol template. pi sends the key as `x-api-key` through
/// the Anthropic SDK for these vendors, hence the explicit auth type.
fn anthropic_template(
    id: &str,
    name: &str,
    description: &str,
    base_url: Option<&str>,
) -> LlmProviderDefinition {
    LlmProviderDefinition {
        id: id.to_string(),
        name: Some(name.to_string()),
        description: Some(description.to_string()),
        base_url: base_url.map(str::to_string),
        auth_type: Some("x-api-key".to_string()),
        default_headers: None,
        format: LlmFormat::Anthropic,
        model_discovery: Some(discovery_for(base_url)),
        api_version: None,
        metadata: None,
        proxy: None,
        no_proxy: None,
        rate_limit: None,
        auth: None,
        compat: None,
        identity: None,
        transport: None,
        env: None,
    }
}

/// OpenCode template: OpenAI-compatible chat plus the mandatory static
/// `x-opencode-client` attribution header. The per-conversation
/// `x-opencode-session` header stays a caller concern (see module docs).
fn opencode_template(
    id: &str,
    name: &str,
    description: &str,
    base_url: &str,
) -> LlmProviderDefinition {
    let mut template = openai_chat_template(id, name, description, Some(base_url), true);
    template.default_headers = Some(
        [(
            OPENCODE_CLIENT_HEADER.to_string(),
            serde_json::json!(OPENCODE_CLIENT_NAME),
        )]
        .into_iter()
        .collect(),
    );
    template
}

/// Static connection templates for the well-known providers.
///
/// Returned in a fixed order: the original five first, then OpenAI-chat
/// vendors, OpenCode pair, endpoint-configured chat gateways, Anthropic
/// vendors, Responses vendors, Vertex and Bedrock. Callers register them
/// through the existing `ProviderDefinitionRegistry`; there is no new
/// registry.
pub fn builtin_provider_definitions() -> Vec<LlmProviderDefinition> {
    vec![
        LlmProviderDefinition {
            id: BUILTIN_OPENAI_ID.to_string(),
            name: Some("OpenAI".to_string()),
            description: Some("OpenAI Chat Completions API".to_string()),
            base_url: Some("https://api.openai.com/v1".to_string()),
            auth_type: Some("bearer".to_string()),
            default_headers: None,
            format: LlmFormat::OpenaiChat,
            model_discovery: Some(llm_types::llm::ModelDiscovery::ModelsEndpoint {
                path: None,
                json_path: None,
            }),
            api_version: None,
            metadata: None,
            proxy: None,
            no_proxy: None,
            rate_limit: None,
            auth: None,
            compat: None,
            identity: None,
            transport: None,
            env: None,
        },
        LlmProviderDefinition {
            id: BUILTIN_ANTHROPIC_ID.to_string(),
            name: Some("Anthropic".to_string()),
            description: Some("Anthropic Messages API".to_string()),
            base_url: Some("https://api.anthropic.com/v1".to_string()),
            auth_type: Some("x-api-key".to_string()),
            default_headers: None,
            format: LlmFormat::Anthropic,
            model_discovery: Some(llm_types::llm::ModelDiscovery::ModelsEndpoint {
                path: None,
                json_path: None,
            }),
            api_version: None,
            metadata: None,
            proxy: None,
            no_proxy: None,
            rate_limit: None,
            auth: None,
            compat: None,
            identity: None,
            transport: None,
            env: None,
        },
        LlmProviderDefinition {
            id: BUILTIN_GEMINI_ID.to_string(),
            name: Some("Gemini".to_string()),
            description: Some("Google Gemini native API".to_string()),
            base_url: Some("https://generativelanguage.googleapis.com/v1beta".to_string()),
            auth_type: Some("x-goog-api-key".to_string()),
            default_headers: None,
            format: LlmFormat::GeminiNative,
            model_discovery: Some(llm_types::llm::ModelDiscovery::Disabled),
            api_version: None,
            metadata: None,
            proxy: None,
            no_proxy: None,
            rate_limit: None,
            auth: None,
            compat: None,
            identity: None,
            transport: None,
            env: None,
        },
        LlmProviderDefinition {
            id: BUILTIN_OLLAMA_ID.to_string(),
            name: Some("Ollama".to_string()),
            description: Some("Local Ollama OpenAI-compatible gateway".to_string()),
            base_url: Some("http://localhost:11434/v1".to_string()),
            auth_type: Some("bearer".to_string()),
            default_headers: None,
            format: LlmFormat::OpenaiChat,
            model_discovery: Some(llm_types::llm::ModelDiscovery::ModelsEndpoint {
                path: None,
                json_path: None,
            }),
            api_version: None,
            metadata: None,
            proxy: None,
            no_proxy: None,
            rate_limit: None,
            auth: None,
            // Explicit legacy field for the local gateway: duplicates what
            // the narrow auto-detection would guess, so the baseline holds
            // even if the detection signal ever narrows further. Explicit
            // config always wins over detection.
            compat: Some(ProviderCompat {
                max_tokens_field: Some(llm_types::llm::generation::LlmMaxTokenField::MaxTokens),
                ..Default::default()
            }),
            identity: None,
            transport: None,
            env: None,
        },
        LlmProviderDefinition {
            id: BUILTIN_OPENROUTER_ID.to_string(),
            name: Some("OpenRouter".to_string()),
            description: Some("OpenRouter OpenAI-compatible gateway".to_string()),
            base_url: Some("https://openrouter.ai/api/v1".to_string()),
            auth_type: Some("bearer".to_string()),
            default_headers: None,
            format: LlmFormat::OpenaiChat,
            model_discovery: Some(llm_types::llm::ModelDiscovery::ModelsEndpoint {
                path: None,
                json_path: None,
            }),
            api_version: None,
            metadata: None,
            proxy: None,
            no_proxy: None,
            rate_limit: None,
            auth: None,
            compat: None,
            identity: None,
            transport: None,
            env: None,
        },
        // Responses wire format on the same OpenAI endpoint (pi's `openai`
        // provider is Responses-based; the chat template above stays the
        // default for backward compatibility).
        openai_responses_template(
            BUILTIN_OPENAI_RESPONSES_ID,
            "OpenAI Responses",
            "OpenAI Responses API on the shared v1 endpoint",
            Some("https://api.openai.com/v1"),
        ),
        // OpenAI-compatible chat vendors (pi: `openai-completions`).
        openai_chat_template(
            BUILTIN_DEEPSEEK_ID,
            "DeepSeek",
            "DeepSeek OpenAI-compatible API",
            // pi keeps the bare host without `/v1`.
            Some("https://api.deepseek.com"),
            true,
        ),
        openai_chat_template(
            BUILTIN_GROQ_ID,
            "Groq",
            "Groq OpenAI-compatible API",
            Some("https://api.groq.com/openai/v1"),
            false,
        ),
        openai_chat_template(
            BUILTIN_TOGETHER_ID,
            "Together",
            "Together OpenAI-compatible API",
            Some("https://api.together.ai/v1"),
            true,
        ),
        openai_chat_template(
            BUILTIN_FIREWORKS_ID,
            "Fireworks",
            "Fireworks OpenAI-compatible API (also serves Anthropic protocol)",
            Some("https://api.fireworks.ai/inference"),
            false,
        ),
        openai_chat_template(
            BUILTIN_CEREBRAS_ID,
            "Cerebras",
            "Cerebras OpenAI-compatible API",
            Some("https://api.cerebras.ai/v1"),
            false,
        ),
        openai_chat_template(
            BUILTIN_BASETEN_ID,
            "Baseten",
            "Baseten OpenAI-compatible inference API",
            Some("https://inference.baseten.co/v1"),
            false,
        ),
        openai_chat_template(
            BUILTIN_HUGGINGFACE_ID,
            "Hugging Face",
            "Hugging Face router OpenAI-compatible API",
            Some("https://router.huggingface.co/v1"),
            false,
        ),
        openai_chat_template(
            BUILTIN_NVIDIA_ID,
            "NVIDIA",
            "NVIDIA NIM OpenAI-compatible API",
            Some("https://integrate.api.nvidia.com/v1"),
            true,
        ),
        openai_chat_template(
            BUILTIN_MOONSHOTAI_ID,
            "Moonshot AI",
            "Moonshot AI OpenAI-compatible API",
            Some("https://api.moonshot.ai/v1"),
            true,
        ),
        openai_chat_template(
            BUILTIN_MOONSHOTAI_CN_ID,
            "Moonshot AI CN",
            "Moonshot AI China OpenAI-compatible API",
            Some("https://api.moonshot.cn/v1"),
            true,
        ),
        openai_chat_template(
            BUILTIN_ANT_LING_ID,
            "Ant Ling",
            "Ant Ling OpenAI-compatible API",
            Some("https://api.ant-ling.com/v1"),
            true,
        ),
        openai_chat_template(
            BUILTIN_KIMI_CODING_ID,
            "Kimi For Coding",
            "Kimi coding-plan OpenAI-compatible API",
            Some("https://api.kimi.com/coding/v1"),
            false,
        ),
        openai_chat_template(
            BUILTIN_ZAI_ID,
            "Z.AI",
            "Z.AI coding-plan OpenAI-compatible API",
            Some("https://api.z.ai/api/coding/paas/v4"),
            true,
        ),
        openai_chat_template(
            BUILTIN_ZAI_CODING_CN_ID,
            "Z.AI Coding CN",
            "Z.AI China coding-plan OpenAI-compatible API",
            Some("https://open.bigmodel.cn/api/coding/paas/v4"),
            true,
        ),
        openai_chat_template(
            BUILTIN_XIAOMI_ID,
            "Xiaomi",
            "Xiaomi OpenAI-compatible API",
            Some("https://api.xiaomimimo.com/v1"),
            false,
        ),
        openai_chat_template(
            BUILTIN_XIAOMI_TOKEN_PLAN_AMS_ID,
            "Xiaomi Token Plan AMS",
            "Xiaomi token-plan OpenAI-compatible API (AMS region)",
            Some("https://token-plan-ams.xiaomimimo.com/v1"),
            false,
        ),
        openai_chat_template(
            BUILTIN_XIAOMI_TOKEN_PLAN_CN_ID,
            "Xiaomi Token Plan CN",
            "Xiaomi token-plan OpenAI-compatible API (CN region)",
            Some("https://token-plan-cn.xiaomimimo.com/v1"),
            false,
        ),
        openai_chat_template(
            BUILTIN_XIAOMI_TOKEN_PLAN_SGP_ID,
            "Xiaomi Token Plan SGP",
            "Xiaomi token-plan OpenAI-compatible API (SGP region)",
            Some("https://token-plan-sgp.xiaomimimo.com/v1"),
            false,
        ),
        openai_chat_template(
            BUILTIN_QWEN_TOKEN_PLAN_ID,
            "Qwen Token Plan",
            "Qwen token-plan OpenAI-compatible API",
            Some("https://token-plan.ap-southeast-1.maas.aliyuncs.com/compatible-mode/v1"),
            false,
        ),
        openai_chat_template(
            BUILTIN_QWEN_TOKEN_PLAN_CN_ID,
            "Qwen Token Plan CN",
            "Qwen token-plan OpenAI-compatible API (Beijing region)",
            Some("https://token-plan.cn-beijing.maas.aliyuncs.com/compatible-mode/v1"),
            false,
        ),
        openai_chat_template(
            BUILTIN_QWEN_TOKEN_PLAN_INDIVIDUAL_ID,
            "Qwen Token Plan Individual",
            "Qwen token-plan OpenAI-compatible API (individual)",
            Some("https://token-plan.ap-southeast-1.maas.aliyuncs.com/compatible-mode/v1"),
            false,
        ),
        openai_chat_template(
            BUILTIN_GITHUB_COPILOT_ID,
            "GitHub Copilot",
            "GitHub Copilot OpenAI-compatible API (subscription OAuth; usable models are filtered server-side)",
            Some("https://api.individual.githubcopilot.com"),
            false,
        ),
        // pi serves Mistral through its dedicated Conversations API; the
        // template targets Mistral's OpenAI-compatible endpoint instead so
        // the built-in chat codec applies unchanged.
        openai_chat_template(
            BUILTIN_MISTRAL_ID,
            "Mistral",
            "Mistral OpenAI-compatible API",
            Some("https://api.mistral.ai/v1"),
            false,
        ),
        // OpenCode pair: multi-API gateways where the template pins the
        // OpenAI-compatible surface (pi picks per model). Callers add the
        // per-conversation `x-opencode-session` header per request.
        opencode_template(
            BUILTIN_OPENCODE_ID,
            "OpenCode Zen",
            "OpenCode Zen OpenAI-compatible gateway (pass x-opencode-session per request)",
            "https://opencode.ai/inference/openai/v1",
        ),
        opencode_template(
            BUILTIN_OPENCODE_GO_ID,
            "OpenCode Go",
            "OpenCode Go OpenAI-compatible gateway (pass x-opencode-session per request)",
            "https://opencode.ai/inference/go/openai/v1",
        ),
        // Endpoint-configured chat gateways: no static base URL. Callers
        // supply the deployment/region/account-specific URL on the profile.
        LlmProviderDefinition {
            id: BUILTIN_AZURE_ID.to_string(),
            name: Some("Azure OpenAI".to_string()),
            description: Some(
                "Azure OpenAI (set the deployment base URL, including ?api-version, on the profile)".to_string(),
            ),
            base_url: None,
            auth_type: Some("bearer".to_string()),
            default_headers: None,
            format: LlmFormat::OpenaiChat,
            model_discovery: Some(ModelDiscovery::Disabled),
            api_version: None,
            metadata: None,
            proxy: None,
            no_proxy: None,
            rate_limit: None,
            auth: None,
            compat: None,
            identity: None,
            transport: None,
            env: None,
        },
        LlmProviderDefinition {
            id: BUILTIN_CLOUDFLARE_AI_GATEWAY_ID.to_string(),
            name: Some("Cloudflare AI Gateway".to_string()),
            description: Some(
                "Cloudflare AI Gateway (set the account/gateway base URL and cf-aig-authorization header on the profile)".to_string(),
            ),
            base_url: None,
            auth_type: Some("bearer".to_string()),
            default_headers: None,
            format: LlmFormat::OpenaiChat,
            model_discovery: Some(ModelDiscovery::Disabled),
            api_version: None,
            metadata: None,
            proxy: None,
            no_proxy: None,
            rate_limit: None,
            auth: None,
            compat: Some(legacy_max_tokens_compat()),
            identity: None,
            transport: None,
            env: None,
        },
        LlmProviderDefinition {
            id: BUILTIN_CLOUDFLARE_WORKERS_AI_ID.to_string(),
            name: Some("Cloudflare Workers AI".to_string()),
            description: Some(
                "Cloudflare Workers AI (set the account-specific base URL on the profile)".to_string(),
            ),
            base_url: None,
            auth_type: Some("bearer".to_string()),
            default_headers: None,
            format: LlmFormat::OpenaiChat,
            model_discovery: Some(ModelDiscovery::Disabled),
            api_version: None,
            metadata: None,
            proxy: None,
            no_proxy: None,
            rate_limit: None,
            auth: None,
            compat: None,
            identity: None,
            transport: None,
            env: None,
        },
        // Anthropic-protocol vendors (pi: `anthropic-messages`).
        anthropic_template(
            BUILTIN_MINIMAX_ID,
            "MiniMax",
            "MiniMax Anthropic-compatible API",
            Some("https://api.minimax.io/anthropic"),
        ),
        anthropic_template(
            BUILTIN_MINIMAX_CN_ID,
            "MiniMax CN",
            "MiniMax China Anthropic-compatible API",
            Some("https://api.minimaxi.com/anthropic"),
        ),
        anthropic_template(
            BUILTIN_VERCEL_AI_GATEWAY_ID,
            "Vercel AI Gateway",
            "Vercel AI Gateway Anthropic-compatible API",
            Some("https://ai-gateway.vercel.sh"),
        ),
        // Responses-protocol vendors (pi: `openai-responses`).
        openai_responses_template(
            BUILTIN_META_ID,
            "Meta",
            "Meta Model API (Responses protocol)",
            Some("https://api.meta.ai/v1"),
        ),
        openai_responses_template(
            BUILTIN_XAI_ID,
            "xAI",
            "xAI Grok API (Responses protocol)",
            Some("https://api.x.ai/v1"),
        ),
        openai_responses_template(
            BUILTIN_OPENAI_CODEX_ID,
            "OpenAI Codex",
            "OpenAI Codex legacy API (subscription OAuth only)",
            Some("https://chatgpt.com/backend-api"),
        ),
        // Google Vertex AI: project/location-specific base URL supplied by
        // the caller; bearer carries the ADC access token.
        LlmProviderDefinition {
            id: BUILTIN_GOOGLE_VERTEX_ID.to_string(),
            name: Some("Google Vertex AI".to_string()),
            description: Some(
                "Google Vertex AI native API (set the location-specific base URL on the profile)".to_string(),
            ),
            base_url: None,
            auth_type: Some("bearer".to_string()),
            default_headers: None,
            format: LlmFormat::GeminiNative,
            model_discovery: Some(ModelDiscovery::Disabled),
            api_version: None,
            metadata: None,
            proxy: None,
            no_proxy: None,
            rate_limit: None,
            auth: None,
            compat: None,
            identity: None,
            transport: None,
            env: None,
        },
        // Amazon Bedrock: region/credential-derived endpoint plus SigV4
        // signing, so the wire format stays plugin-owned. The template
        // reserves the id and documents the contract; requests resolve
        // once a `BEDROCK_CONVERSE` codec plugin is registered.
        LlmProviderDefinition {
            id: BUILTIN_AMAZON_BEDROCK_ID.to_string(),
            name: Some("Amazon Bedrock".to_string()),
            description: Some(
                "Amazon Bedrock Converse API (set the regional base URL on the profile; requires a BEDROCK_CONVERSE codec plugin)".to_string(),
            ),
            base_url: None,
            auth_type: Some("bearer".to_string()),
            default_headers: None,
            format: LlmFormat::Custom("BEDROCK_CONVERSE".to_string()),
            model_discovery: Some(ModelDiscovery::Disabled),
            api_version: None,
            metadata: None,
            proxy: None,
            no_proxy: None,
            rate_limit: None,
            auth: None,
            compat: None,
            identity: None,
            transport: None,
            env: None,
        },
    ]
}

/// All built-in provider ids in registration order.
pub fn builtin_provider_ids() -> Vec<String> {
    builtin_provider_definitions()
        .into_iter()
        .map(|d| d.id)
        .collect()
}

/// Register every built-in provider definition on a registry.
///
/// File-layer definitions registered afterwards win on id collision
/// because registration replaces by id.
pub fn register_builtin_providers(registry: &ProviderDefinitionRegistry) -> LlmResult<()> {
    for definition in builtin_provider_definitions() {
        registry.register(definition)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn by_id(id: &str) -> LlmProviderDefinition {
        builtin_provider_definitions()
            .into_iter()
            .find(|d| d.id == id)
            .unwrap_or_else(|| panic!("missing builtin provider {id}"))
    }

    #[test]
    fn builtin_providers_cover_all_wire_formats() {
        let definitions = builtin_provider_definitions();
        assert!(definitions.len() >= 4);
        let ids = builtin_provider_ids();
        assert!(ids.contains(&BUILTIN_OPENAI_ID.to_string()));
        assert!(ids.contains(&BUILTIN_ANTHROPIC_ID.to_string()));
        assert!(ids.contains(&BUILTIN_GEMINI_ID.to_string()));
        assert!(ids.contains(&BUILTIN_OLLAMA_ID.to_string()));
    }

    #[test]
    fn builtin_provider_ids_are_unique() {
        let ids = builtin_provider_ids();
        let unique: HashSet<_> = ids.iter().collect();
        assert_eq!(ids.len(), unique.len(), "duplicate builtin provider id");
        for id in &ids {
            assert!(!id.trim().is_empty());
        }
    }

    #[test]
    fn local_builtin_keeps_legacy_max_tokens_field() {
        let ollama = by_id(BUILTIN_OLLAMA_ID);
        assert_eq!(
            ollama.compat.and_then(|c| c.max_tokens_field),
            Some(llm_types::llm::generation::LlmMaxTokenField::MaxTokens)
        );
    }

    #[test]
    fn legacy_gateways_keep_max_tokens_field() {
        // Mirrors pi's `useMaxTokens` branch for these endpoints.
        for id in [
            BUILTIN_DEEPSEEK_ID,
            BUILTIN_MOONSHOTAI_ID,
            BUILTIN_MOONSHOTAI_CN_ID,
            BUILTIN_TOGETHER_ID,
            BUILTIN_NVIDIA_ID,
            BUILTIN_ZAI_ID,
            BUILTIN_ZAI_CODING_CN_ID,
            BUILTIN_ANT_LING_ID,
            BUILTIN_OPENCODE_ID,
            BUILTIN_OPENCODE_GO_ID,
            BUILTIN_CLOUDFLARE_AI_GATEWAY_ID,
        ] {
            let definition = by_id(id);
            assert_eq!(
                definition.compat.and_then(|c| c.max_tokens_field),
                Some(LlmMaxTokenField::MaxTokens),
                "{id} should keep the legacy max_tokens field"
            );
        }
    }

    #[test]
    fn opencode_templates_carry_static_client_header() {
        // pi's `withOpenCodeSessionHeader`: static `x-opencode-client` on
        // every request, dynamic `x-opencode-session` per conversation
        // (the latter stays a per-request caller concern).
        for id in [BUILTIN_OPENCODE_ID, BUILTIN_OPENCODE_GO_ID] {
            let definition = by_id(id);
            assert_eq!(definition.format, LlmFormat::OpenaiChat);
            let base = definition.base_url.expect("opencode needs a base url");
            assert!(base.contains("opencode.ai"), "{id} base: {base}");
            let headers = definition.default_headers.expect("client header");
            assert_eq!(
                headers.get(OPENCODE_CLIENT_HEADER),
                Some(&serde_json::json!(OPENCODE_CLIENT_NAME)),
                "{id} must send {OPENCODE_CLIENT_HEADER}"
            );
            assert!(
                definition
                    .description
                    .unwrap_or_default()
                    .contains(OPENCODE_SESSION_HEADER),
                "{id} must document the per-request session header"
            );
        }
    }

    #[test]
    fn endpoint_configured_templates_have_no_base_url() {
        // Deployment/region/account-specific endpoints cannot be baked in.
        for id in [
            BUILTIN_AZURE_ID,
            BUILTIN_AMAZON_BEDROCK_ID,
            BUILTIN_CLOUDFLARE_AI_GATEWAY_ID,
            BUILTIN_CLOUDFLARE_WORKERS_AI_ID,
            BUILTIN_GOOGLE_VERTEX_ID,
        ] {
            assert_eq!(by_id(id).base_url, None, "{id} needs caller base_url");
        }
    }

    #[test]
    fn custom_format_is_reserved_for_bedrock_only() {
        for definition in builtin_provider_definitions() {
            if let LlmFormat::Custom(name) = &definition.format {
                assert_eq!(definition.id, BUILTIN_AMAZON_BEDROCK_ID);
                assert_eq!(name, "BEDROCK_CONVERSE");
            }
        }
    }

    #[test]
    fn responses_vendors_use_responses_format() {
        for id in [
            BUILTIN_META_ID,
            BUILTIN_XAI_ID,
            BUILTIN_OPENAI_CODEX_ID,
            BUILTIN_OPENAI_RESPONSES_ID,
        ] {
            assert_eq!(by_id(id).format, LlmFormat::OpenaiResponse, "{id}");
        }
    }

    #[test]
    fn anthropic_vendors_use_api_key_header() {
        for id in [
            BUILTIN_MINIMAX_ID,
            BUILTIN_MINIMAX_CN_ID,
            BUILTIN_VERCEL_AI_GATEWAY_ID,
        ] {
            let definition = by_id(id);
            assert_eq!(definition.format, LlmFormat::Anthropic, "{id}");
            assert_eq!(definition.auth_type.as_deref(), Some("x-api-key"), "{id}");
        }
    }

    #[test]
    fn register_builtin_providers_fills_registry() {
        let registry = ProviderDefinitionRegistry::new();
        register_builtin_providers(&registry).expect("register");
        assert_eq!(registry.size(), builtin_provider_definitions().len());
        // Re-registering is idempotent (replace by id).
        register_builtin_providers(&registry).expect("re-register");
        assert_eq!(registry.size(), builtin_provider_definitions().len());
    }
}
