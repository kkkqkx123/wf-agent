#ifndef WF_PLUGIN_H
#define WF_PLUGIN_H

/*
 * Native plugin C ABI v1 (stable contract).
 *
 * Status: hand-written mirror of `crates/foundation/wf-plugin-sdk/src/native.rs`,
 * for C / C++ / Zig / Go (cgo) authors implementing native plugins without Rust.
 * Rust authors keep using `wf-plugin-sdk::export_plugin!`; its expansion matches
 * the symbol table declared here one by one. Host-side loading lives in
 * `crates/infra/wf-plugin/src/native/abi.rs` (symbol resolution, negotiation)
 * and `crates/infra/wf-plugin/src/native/plugin.rs` (forwarding, dispatch).
 *
 * Design notes in plain language:
 * - Only C scalars and pointers cross the boundary; no Rust layouts,
 *   generics, trait objects, or async bodies.
 * - Complex payloads are always NUL-terminated JSON strings; structs are envelopes.
 * - Single-direction memory ownership: the host allocates, the plugin only
 *   reads or writes host buffers, never transfers allocator ownership.
 * - Variable-length results use two-phase query: size probe with a null pointer
 *   first, then fill; on short buffers the guest reports the needed size for retry.
 * - Integer status codes only: zero means success, non-zero means failure.
 */

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Matches `WF_ABI_VERSION_V1`; the only major version for now. */
#define WF_ABI_VERSION_V1 1u

/* Plugin context envelope, matches `PluginContextC`. Strings stay read-only
 * for the call; plugins must not retain or free them. */
typedef struct WfPluginContext {
    uint32_t abi_version;
    const char *plugin_id;
    const char *config_json;
} WfPluginContext;

/* Host contribution registrar, matches `ContributionRegistrarC`. The first
 * argument of each callback is the opaque host context handle; string
 * arguments are NUL-terminated names or JSON documents. */
typedef struct WfContributionRegistrar {
    uint32_t abi_version;
    void *context;
    int (*register_node_type)(void *ctx, const char *name);
    int (*register_tool_type)(void *ctx, const char *name);
    int (*register_llm_provider)(void *ctx, const char *name);
    int (*register_llm_auth)(void *ctx, const char *name);
    int (*register_event_handler)(void *ctx, const char *event_type);
    int (*register_middleware)(void *ctx, const char *phase, int32_t priority);
    int (*register_tool)(void *ctx, const char *tool_json);
    int (*register_tool_description)(void *ctx, const char *description_json);
    int (*register_cli_command)(void *ctx, const char *command_json);
    int (*register_completion_source)(void *ctx, const char *source_json);
    int (*register_setting_field)(void *ctx, const char *field_json);
    int (*register_tool_summary)(void *ctx, const char *summary_json);
    int (*register_markdown_rule)(void *ctx, const char *rule_json);
    int (*register_llm_images)(void *ctx, const char *name);
    int (*register_llm_provider_definition)(void *ctx, const char *definition_json);
    int (*register_workflow)(void *ctx, const char *workflow_json);
    int (*register_prompt)(void *ctx, const char *prompt_json);
    int (*register_fragment)(void *ctx, const char *fragment_json);
    int (*register_agent_template)(void *ctx, const char *agent_json);
    int (*register_node_template)(void *ctx, const char *node_json);
    int (*register_trigger)(void *ctx, const char *trigger_json);
    int (*register_compaction_advisor)(void *ctx, const char *name);
    int (*register_text_analyzer)(void *ctx, const char *name);
    int (*register_retrieval_advisor)(void *ctx, const char *name);
    int (*register_vector_encoding)(void *ctx, const char *name);
    int (*register_snapshot_policy)(void *ctx, const char *name, const char *policy_json);
} WfContributionRegistrar;

/*
 * Symbols every plugin must export. Semantics match the `export_plugin!` output:
 *
 * - supported_abi_versions: report supported ABI major versions, two-phase.
 * - get_manifest: return manifest TOML bytes, two-phase.
 * - on_load / on_activate / on_deactivate / on_unload / on_config_change:
 *   lifecycle hooks, zero means success.
 * - register_contributions: declare contributions via the registrar.
 * - dispatch_handler: run one registered behavior by type and name into a host buffer.
 *
 * Two-phase rule: null output means report the needed size and return zero;
 * a short buffer means update the size to the needed value and return 2 for
 * a host retry; any other non-zero value means failure.
 */
int32_t wf_plugin_supported_abi_versions(uint32_t *out_versions, size_t *out_len);
int32_t wf_plugin_get_manifest(uint8_t *out, size_t *len);
int32_t wf_plugin_on_load(const WfPluginContext *ctx);
int32_t wf_plugin_on_activate(const WfPluginContext *ctx);
int32_t wf_plugin_on_deactivate(const WfPluginContext *ctx);
int32_t wf_plugin_on_unload(const WfPluginContext *ctx);
int32_t wf_plugin_on_config_change(const WfPluginContext *ctx);
int32_t wf_plugin_register_contributions(WfContributionRegistrar *host);
int32_t wf_plugin_dispatch_handler(const char *handler_type, const char *handler_name,
                                   const char *input_json, uint8_t *output_buf,
                                   size_t *output_len);

#ifdef __cplusplus
}
#endif

#endif /* WF_PLUGIN_H */
