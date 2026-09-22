# Export surface for the `wf:plugin/plugin@0.1.0-draft` component world.
#
# `componentize-py` binds this module to the interfaces declared in
# `wit/plugin.wit` by matching class names to WIT interface names
# (PascalCase) and method names to WIT function names (snake_case).
#
# The `wit_world` package is generated on the fly by
# `componentize-py componentize`; `make bindings` only emits stubs for IDEs
# and type checkers.

import wit_world
from wit_world import exports
from wit_world.exports import contributions, lifecycle


class Lifecycle(exports.Lifecycle):
    def on_load(self, input: lifecycle.HookInput) -> None:
        pass

    def on_activate(self, input: lifecycle.HookInput) -> None:
        pass

    def on_deactivate(self) -> None:
        pass

    def on_unload(self) -> None:
        pass

    def on_config_change(self, config: str) -> None:
        pass


class Contributions(exports.Contributions):
    def register(self) -> contributions.Declaration:
        return contributions.Declaration(
            node_types=[],
            tool_types=["echo"],
            llm_providers=[],
            event_handlers=[],
            middleware=[],
        )

    def dispatch(
        self,
        handler_type: str,
        handler_name: str,
        input_json: str,
    ) -> str:
        # LLM codecs arrive as ("llm-codec", "<name>/<op>") with a JSON
        # envelope; the host builds the HTTP request from the returned
        # description.
        if handler_type == "tool" and handler_name == "echo":
            return '{"result":{"echo":true}}'
        raise ValueError(f"unknown handler: {handler_type}/{handler_name}")
