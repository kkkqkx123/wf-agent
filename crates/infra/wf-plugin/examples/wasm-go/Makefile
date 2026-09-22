# Build the Go component-model guest with TinyGo.
#
# The app bindings under internal/ are generated from the canonical world in
# wit/ with wit-bindgen-go and committed, so a plain build only needs TinyGo.
# TinyGo lifts the compiled core module into a component using the world in
# wit-component/, which adds the WASI p2 imports the TinyGo runtime carries.
#
# Requirements:
#   - TinyGo 0.34+ (wasip2 target) for `build`
#   - wit-bindgen-go for `bindings`
#   - wasm-tools for `verify`

WIT_DIR       ?= wit
COMPONENT_DIR ?= wit-component
WORLD         ?= component
OUT           ?= plugin.wasm
TARGET        ?= wasip2

TINYGOROOT := $(shell tinygo env TINYGOROOT 2>/dev/null)
WASI_WIT   := $(TINYGOROOT)/lib/wasi-cli/wit

.PHONY: all build deps bindings verify clean

all: build

# Stage the component world: the canonical world and the committed
# component.wit live together, and the WASI WIT shipped with TinyGo is copied
# in so `include wasi:cli/imports` resolves.
deps:
	@test -d "$(WASI_WIT)" || (echo "WASI WIT not found at $(WASI_WIT); check TINYGOROOT" && exit 1)
	mkdir -p $(COMPONENT_DIR)/deps/cli
	cp $(WIT_DIR)/plugin.wit $(COMPONENT_DIR)/plugin.wit
	cp $(WASI_WIT)/*.wit $(COMPONENT_DIR)/deps/cli/
	cp -R $(WASI_WIT)/deps/. $(COMPONENT_DIR)/deps/

build: deps
	tinygo build -target=$(TARGET) -wit-package $(COMPONENT_DIR) -wit-world $(WORLD) -o $(OUT) .

# Regenerate the committed bindings in internal/ from the canonical world.
# Only needed when wit/plugin.wit changes.
bindings:
	wit-bindgen-go generate -w plugin -o internal ./$(WIT_DIR)

verify:
	wasm-tools validate --features component-model $(OUT)
	wasm-tools component wit $(OUT)

clean:
	rm -f $(OUT)
	rm -rf $(COMPONENT_DIR)/plugin.wit $(COMPONENT_DIR)/deps
