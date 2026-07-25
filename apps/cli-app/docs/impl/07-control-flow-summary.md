# Control Flow Summary

## Application Startup Flow

```
scripts/modular-agent.js
    ↓ ESM import
src/index.ts
    ↓
initializeTomlParser()
    ↓
loadConfigWithEnvOverride()  ←── config file + env vars
    ↓
getOutput().reconfigure()   ←── output format (text/json/table)
    ↓
initializeFormatter()
    ↓
initLogger() / initSDKLogger()
    ↓
createAppSDK() + StorageManager
    ↓
CLIUserInteractionManager
    ↓
initializeContainer()       ←── DI container
    ↓
Commander preAction hook    ←── runs bootstrap
    ↓
┌─────────────────────────────────────────┐
│ CLI Mode          │  TUI Mode           │
│ (with args)       │  (--tui / no args)  │
│                   │                     │
│ Execute command   │  new CLIAppTUI()    │
│ Format output     │  .start()           │
│ Exit              │  Event loop         │
└─────────────────────────────────────────┘
```

## TUI Input Processing Flow

```
Raw stdin bytes
    ↓
ProcessTerminal (raw mode, Kitty protocol)
    ↓
StdinBuffer (sequence splitting: CSI, OSC, DCS, APC, mouse)
    ↓
TUI.handleInput(data)
    ↓
┌─────────────────────────────────────────────────────┐
│ PhaseManager.handleInput()                          │
│   - Streaming: buffer printable chars (0x20-0x7E)  │
│   - Others: pass through                            │
│   → consumed? return                                │
└─────────────────────────────────────────────────────┘
    ↓ not consumed
┌─────────────────────────────────────────────────────┐
│ Active Modal (capturesAllKeys)?                     │
│   → handleKey(data)                                 │
│   → ModalAction.Close? closeModal()                 │
│   → return                                          │
└─────────────────────────────────────────────────────┘
    ↓ no capturing modal
┌─────────────────────────────────────────────────────┐
│ App-level onInput (screen routing)                  │
│   - Ctrl+L → force redraw                           │
│   - Esc → close overlay                             │
│   - Ctrl+D → quit                                   │
│   - Delegate to current screen.handleInput()        │
│   → handled? return                                 │
└─────────────────────────────────────────────────────┘
    ↓ not handled
┌─────────────────────────────────────────────────────┐
│ Non-capturing Modal?                                │
│   → handleKey(data)                                 │
│   → ModalAction.Close? closeModal()                 │
│   → return                                          │
└─────────────────────────────────────────────────────┘
    ↓ no modal
┌─────────────────────────────────────────────────────┐
│ Focused Component.handleInput(data)                 │
│   - SelectList: navigation, selection               │
│   - Input: character editing                        │
│   - Editor: multi-line editing                      │
│   → requestRender()                                 │
└─────────────────────────────────────────────────────┘
```

## Render Flow

```
requestRender(force?)
    ↓
┌─────────────────────────────────────────────────────┐
│ Force: reset renderer, render on next tick          │
│ Normal: coalesce (skip if already requested)        │
└─────────────────────────────────────────────────────┘
    ↓
scheduleRender()
    ↓
setTimeout(max(0, 16ms - elapsed))
    ↓
doRender()
    ↓
┌─────────────────────────────────────────────────────┐
│ 1. this.render(width) — main content                │
│ 2. compositeOverlays() — overlay stack compositing  │
│ 3. renderer.render(lines, width, height)            │
│    - RetainedRenderer: diff + minimal redraw        │
│    - PlainRenderer: full pass-through               │
└─────────────────────────────────────────────────────┘
    ↓
stdout
```

## Modal Lifecycle

```
ModalManager.show(modal, options)
    ↓
TUI.showModal(modal, options)
    ↓
modalToOverlayOptions() → overlayOptions
    ↓
showOverlay(component, overlayOptions)
    ↓
┌─────────────────────────────────────────────────────┐
│ Push to overlayStack                                │
│ Set focus (if capturesAllKeys)                      │
│ Hide cursor                                         │
│ requestRender()                                     │
└─────────────────────────────────────────────────────┘
    ↓
modal.onOpen?.()
    ↓
Push to modalStack
    ↓
... user interacts ...
    ↓
TUI.closeModal(modal)
    ↓
Find in modalStack → remove
handle.hide() → remove from overlayStack
modal.onClose?.()
requestRender()
```

## Screen Navigation Flow

```
showScreen(screenId)
    ↓
┌─────────────────────────────────────────────────────┐
│ oldScreen.onDeactivate()                            │
│ mainContainer.clear()                               │
│ currentScreenId = screenId                          │
│ tui.setContext(SCREEN_CONTEXTS[screenId])           │
│ screen.onActivate()                                 │
│ mainContainer.addChild(screen.render())             │
│ tui.requestRender()                                 │
└─────────────────────────────────────────────────────┘
```

## Phase Transition Flow

```
PhaseManager.transition(to)
    ↓
┌─────────────────────────────────────────────────────┐
│ Validate transition (TRANSITIONS table)             │
│ If leaving Streaming: releaseBufferedInput()        │
│ Update _phase                                        │
│ Call onPhaseChange(from, to, data)                  │
└─────────────────────────────────────────────────────┘
    ↓
onPhaseChange callback (TUI.setupPhaseManager)
    ↓
┌─────────────────────────────────────────────────────┐
│ If → Idle && capturing modal open: closeModal()     │
│ Update currentContext:                              │
│   Idle/Streaming/Normal → "chat"                   │
│   Approval → "modal"                                │
│ Update inputMode:                                   │
│   Normal → InputMode.Normal                         │
│   Others → InputMode.Chat                           │
│ requestRender()                                     │
└─────────────────────────────────────────────────────┘
```

## Event Scheduler Sources

```
┌────────────────┬───────────┬──────────────────────────────┐
│ Source         │ Interval  │ Condition                    │
├────────────────┼───────────┼──────────────────────────────┤
│ spinner        │ 80ms      │ phase === Streaming          │
│ configPoll     │ 500ms     │ _configPollHandler exists    │
│ renderTick     │ 50ms      │ always (when not stopped)    │
└────────────────┴───────────┴──────────────────────────────┘
```

## Data Adapter Error Handling

```
Adapter.executeWithErrorHandling()
    ↓
try {
  ↓
  SDK operation
    ↓
  Return result
} catch (error) {
  ↓
  Convert to CLIError
    ↓
  Format for output
    ↓
  Exit with code
}
```

## Dependency Injection Flow

```
initializeContainer()
    ↓
┌─────────────────────────────────────────────────────┐
│ CLIDependencyContainer                              │
│   - SDK instance (from createAppSDK)                │
│   - TerminalManager                                 │
│   - ExecutionService                                │
│   - WorkflowExecutionAdapter                        │
│   - CLIUserInteractionHandler                       │
└─────────────────────────────────────────────────────┘
    ↓
getContainer() → singleton access
    ↓
Commands/Services resolve dependencies
```
