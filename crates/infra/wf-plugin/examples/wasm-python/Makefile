# Build the Python component-model guest with componentize-py.
#
# Requirements:
#   - componentize-py (`pip install componentize-py`)
#   - wasm-tools for `verify`

WIT      ?= wit
WORLD    ?= plugin
OUT      ?= plugin.wasm
BINDINGS ?= plugin/bindings

.PHONY: all bindings build verify clean

all: build

# Stub bindings for IDEs and type checkers. `componentize` generates the real
# `wit_world` code itself, so the stubs are not required to build.
bindings:
	@command -v componentize-py >/dev/null || (echo "componentize-py not found: pip install componentize-py" && exit 1)
	componentize-py -d $(WIT) -w $(WORLD) bindings $(BINDINGS)

# The app is the `plugin` package. It may share the world name because
# componentize-py generates its bindings under the `wit_world` module.
build:
	@command -v componentize-py >/dev/null || (echo "componentize-py not found: pip install componentize-py" && exit 1)
	componentize-py -d $(WIT) -w $(WORLD) componentize -p . -o $(OUT) plugin

verify:
	@command -v wasm-tools >/dev/null || (echo "wasm-tools not found: cargo install wasm-tools" && exit 1)
	wasm-tools validate --features component-model $(OUT)
	wasm-tools component wit $(OUT)

clean:
	rm -f $(OUT)
	rm -rf $(BINDINGS)
