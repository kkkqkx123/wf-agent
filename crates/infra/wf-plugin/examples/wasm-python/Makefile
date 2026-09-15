WIT      ?= wit/plugin.wit
WORLD    ?= plugin
OUT      ?= plugin.wasm
BINDINGS ?= plugin/bindings

.PHONY: all bindings build clean

all: build

bindings:
	@command -v componentize-py >/dev/null || (echo "componentize-py not found: pip install componentize-py" && exit 1)
	rm -rf $(BINDINGS)
	componentize-py -d wit -w $(WORLD) bindings -o $(BINDINGS)

build: bindings
	componentize-py -d wit -w $(WORLD) componentize -o $(OUT) plugin

clean:
	rm -f $(OUT)
	rm -rf $(BINDINGS)
