// Minimal component-model wasm guest written in Go, targeting the
// `wf:plugin/plugin@0.1.0-draft` world declared in wit/plugin.wit.
//
// Build with TinyGo (recommended):
//
//	tinygo build -target=wasip2 -o plugin.wasm ./...
//
// The host loads plugin.wasm through the component path; no `wf_*`
// export names are required — the WIT interface names drive dispatch.
package main

// HookInput mirrors the `wf:plugin/lifecycle#hook-input` record.
type HookInput struct {
	PluginId string
	Config   string
}

// MiddlewareDecl mirrors `wf:plugin/contributions#middleware-decl`.
type MiddlewareDecl struct {
	Phase    string
	Priority int32
}

// Declaration mirrors `wf:plugin/contributions#declaration`.
type Declaration struct {
	NodeTypes     []string
	ToolTypes     []string
	LlmProviders  []string
	Formatters    []string
	EventHandlers []string
	Middleware    []MiddlewareDecl
}

//go:wasmexport wf:plugin/lifecycle@0.1.0-draft#on-load
func OnLoad(input HookInput) error {
	return nil
}

//go:wasmexport wf:plugin/lifecycle@0.1.0-draft#on-activate
func OnActivate(input HookInput) error {
	return nil
}

//go:wasmexport wf:plugin/lifecycle@0.1.0-draft#on-deactivate
func OnDeactivate() error {
	return nil
}

//go:wasmexport wf:plugin/lifecycle@0.1.0-draft#on-unload
func OnUnload() error {
	return nil
}

//go:wasmexport wf:plugin/lifecycle@0.1.0-draft#on-config-change
func OnConfigChange(config string) error {
	return nil
}

//go:wasmexport wf:plugin/contributions@0.1.0-draft#register
func Register() Declaration {
	return Declaration{
		ToolTypes: []string{"echo"},
	}
}

//go:wasmexport wf:plugin/contributions@0.1.0-draft#dispatch
func Dispatch(handlerType, handlerName, inputJson string) (string, error) {
	if handlerType == "tool" && handlerName == "echo" {
		return `{"result":{"echo":true}}`, nil
	}
	return "", errUnknownHandler{handlerType: handlerType, handlerName: handlerName}
}

type errUnknownHandler struct {
	handlerType string
	handlerName string
}

func (e errUnknownHandler) Error() string {
	return "unknown handler: " + e.handlerType + "/" + e.handlerName
}

func main() {}
