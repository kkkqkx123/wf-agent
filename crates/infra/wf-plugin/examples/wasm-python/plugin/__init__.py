# wf-agent Python wasm guest example (componentize-py).
#
# The Makefile generates bindings from wit/plugin.wit into `plugin/bindings/`
# and packages this module into `plugin.wasm`. Only `exports.py` needs to be
# edited by plugin authors; `bindings/` is regenerated on every build.
