WIT     ?= wit/plugin.wit
OUT     ?= plugin.wasm
TARGET  ?= wasip2

.PHONY: build clean verify

build:
	@command -v tinygo >/dev/null || (echo "tinygo not found: install TinyGo 0.34+ for the wasip2 target" && exit 1)
	tinygo build -target=$(TARGET) -scheduler=none -gc=conservative -o $(OUT) ./...

clean:
	rm -f $(OUT)

verify:
	@command -v wasm-tools >/dev/null || (echo "wasm-tools not found: cargo install wasm-tools" && exit 1)
	wasm-tools component inspect $(OUT)
