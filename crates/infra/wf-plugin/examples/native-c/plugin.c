/*
 * native-echo-c: minimal C native plugin against `wf-plugin-sdk/c-api/wf_plugin.h`.
 *
 * Covered contract: version handshake, two-phase manifest, lifecycle hooks,
 * tool-type registration, single dispatch entry.
 * Memory rule: read host strings only, write host buffers only, never hand
 * ownership of plugin memory to the host.
 */

#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include "../../../../foundation/wf-plugin-sdk/c-api/wf_plugin.h"

/* Manifest TOML; keep id and version aligned with plugin.toml next to it. */
static const char kManifest[] =
    "id = \"native-echo-c\"\n"
    "version = \"1.0.0\"\n"
    "entry_point = \"native_echo.so\"\n"
    "plugin_type = \"native\"\n";

/* Tool dispatch reply: minimal JSON matching host PluginToolResult. */
static const char kEchoReply[] = "{\"result\":{\"echo\":true}}";

static int32_t write_bytes(const uint8_t *src, size_t src_len, uint8_t *out, size_t *len) {
    if (len == NULL) {
        return 1;
    }
    if (out == NULL) {
        *len = src_len;
        return 0;
    }
    if (*len < src_len) {
        *len = src_len;
        return 2;
    }
    memcpy(out, src, src_len);
    *len = src_len;
    return 0;
}

int32_t wf_plugin_supported_abi_versions(uint32_t *out_versions, size_t *out_len) {
    static const uint32_t kSupported[] = {WF_ABI_VERSION_V1};
    size_t need = 1;
    if (out_len == NULL) {
        return 1;
    }
    if (out_versions == NULL) {
        *out_len = need;
        return 0;
    }
    if (*out_len < need) {
        *out_len = need;
        return 2;
    }
    out_versions[0] = kSupported[0];
    *out_len = need;
    return 0;
}

int32_t wf_plugin_get_manifest(uint8_t *out, size_t *len) {
    return write_bytes((const uint8_t *)kManifest, sizeof(kManifest) - 1u, out, len);
}

int32_t wf_plugin_on_load(const WfPluginContext *ctx) {
    (void)ctx;
    return 0;
}

int32_t wf_plugin_on_activate(const WfPluginContext *ctx) {
    (void)ctx;
    return 0;
}

int32_t wf_plugin_on_deactivate(const WfPluginContext *ctx) {
    (void)ctx;
    return 0;
}

int32_t wf_plugin_on_unload(const WfPluginContext *ctx) {
    (void)ctx;
    return 0;
}

int32_t wf_plugin_on_config_change(const WfPluginContext *ctx) {
    (void)ctx;
    return 0;
}

int32_t wf_plugin_register_contributions(WfContributionRegistrar *host) {
    if (host == NULL) {
        return 1;
    }
    if (host->register_tool_type != NULL) {
        if (host->register_tool_type(host->context, "echo") != 0) {
            return 1;
        }
    }
    return 0;
}

int32_t wf_plugin_dispatch_handler(const char *handler_type, const char *handler_name,
                                   const char *input_json, uint8_t *output_buf,
                                   size_t *output_len) {
    (void)input_json;
    if (handler_type == NULL || handler_name == NULL || output_len == NULL) {
        return 1;
    }
    if (strcmp(handler_type, "tool") != 0 || strcmp(handler_name, "echo") != 0) {
        return 1;
    }
    return write_bytes((const uint8_t *)kEchoReply, sizeof(kEchoReply) - 1u, output_buf,
                       output_len);
}
