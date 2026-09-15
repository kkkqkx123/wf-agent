# Export surface for the `wf:plugin/plugin@0.1.0-draft` component world.
#
# `componentize-py` binds this module to the interfaces declared in
# `wit/plugin.wit` by matching Python class names to WIT interface names
# (PascalCase) and function names to WIT function names (snake_case).
#
# Regenerate `plugin/bindings/` via `make bindings` before importing.

from .bindings.wf import plugin


class Lifecycle(plugin.lifecycle.Lifecycle):
    @staticmethod
    def on_load(input: plugin.lifecycle.HookInput) -> None:
        pass

    @staticmethod
    def on_activate(input: plugin.lifecycle.HookInput) -> None:
        pass

    @staticmethod
    def on_deactivate() -> None:
        pass

    @staticmethod
    def on_unload() -> None:
        pass

    @staticmethod
    def on_config_change(config: str) -> None:
        pass


class Contributions(plugin.contributions.Contributions):
    @staticmethod
    def register() -> plugin.contributions.Declaration:
        return plugin.contributions.Declaration(
            node_types=[],
            tool_types=["echo"],
            llm_providers=[],
            formatters=[],
            event_handlers=[],
            middleware=[],
        )

    @staticmethod
    def dispatch(
        handler_type: str,
        handler_name: str,
        input_json: str,
    ) -> str:
        if handler_type == "tool" and handler_name == "echo":
            return '{"result":{"echo":true}}'
        raise ValueError(f"unknown handler: {handler_type}/{handler_name}")
