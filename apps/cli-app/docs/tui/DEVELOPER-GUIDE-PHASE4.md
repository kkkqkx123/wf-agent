# TUI Application Developer Guide - Phase 4

## Quick Start

### Running the TUI Application

```bash
# Navigate to cli-app directory
cd apps/cli-app

# Build the project
pnpm build

# Start in TUI mode
pnpm start --tui
```

## Creating a New Screen

To add a new screen to the application, follow these steps:

### 1. Create Screen File

```typescript
// src/tui/screens/my-screen.ts
import { Container, Box, Text } from "../core/index.js";
import type { Screen } from "./screen.js";

export class MyScreen implements Screen {
  private container: Container;

  constructor() {
    this.container = new Container();
    this.setupLayout();
  }

  private setupLayout() {
    // Add your components here
    const header = new Box();
    header.addChild(new Text("My Screen Title"));
    
    this.container.addChild(header);
  }

  render(): Container {
    return this.container;
  }

  // Optional lifecycle methods
  onActivate() {
    console.log("Screen activated");
  }

  onDeactivate() {
    console.log("Screen deactivated");
  }

  destroy() {
    console.log("Screen destroyed");
  }
}
```

### 2. Export from Index

```typescript
// src/tui/screens/index.ts
export { MyScreen } from "./my-screen.js";
```

### 3. Register in App

```typescript
// src/tui/app.ts
private initializeScreens() {
  // ... existing screens
  
  // Register your new screen
  this.screens.set("my-screen", new MyScreen());
}
```

### 4. Add Navigation

```typescript
// In DashboardScreen or other navigation point
this.menuList = new SelectList([
  // ... existing items
  {
    value: "my-screen",
    label: "🎯 My Feature",
    description: "Description of my feature",
  },
]);
```

## Screen Interface

All screens must implement the `Screen` interface:

```typescript
interface Screen {
  // Required: Render the screen content
  render(): Component;

  // Optional: Called when screen becomes active
  onActivate?(): void;

  // Optional: Called when screen becomes inactive
  onDeactivate?(): void;

  // Optional: Handle keyboard input at screen level
  handleInput?(data: string): boolean;

  // Optional: Cleanup resources
  destroy?(): void;
}
```

## Available Components

### Text
Display text content with optional padding.

```typescript
const text = new Text("Hello World", paddingX, paddingY);
```

### Box
Container with padding and optional background.

```typescript
const box = new Box(paddingX, paddingY, bgFn);
box.addChild(component);
```

### SelectList
Interactive list for user selection.

```typescript
const list = new SelectList([
  { value: "id1", label: "Item 1", description: "Desc 1" },
  { value: "id2", label: "Item 2", description: "Desc 2" },
]);

list.onSelect = (item) => {
  console.log("Selected:", item.value);
};
```

### Editor
Multi-line text editor (Phase 3).

```typescript
const editor = new Editor({
  multiline: true,
  placeholder: "Enter text...",
  onSubmit: (text) => {
    console.log("Submitted:", text);
  },
});
```

### Input
Single-line input field.

```typescript
const input = new Input({
  placeholder: "Enter value...",
  onSubmit: (value) => {
    console.log("Value:", value);
  },
});
```

### Spacer
Empty space for layout.

```typescript
const spacer = new Spacer(height);
```

### Loader
Loading indicator.

```typescript
const loader = new Loader("Loading...");
```

## Keyboard Handling

### Global Shortcuts

Defined in `CLIAppTUI.handleGlobalInput()`:

```typescript
private handleGlobalInput(data: string): boolean {
  // Ctrl+Q to quit
  if (data === "\x11") {
    this.quit();
    return true;
  }
  
  return false;
}
```

### Screen-level Input

Implement `handleInput()` in your screen:

```typescript
handleInput(data: string): boolean {
  // Check for specific keys
  if (data === "r") {
    this.refresh();
    return true;
  }
  
  // Delegate to child components
  if (this.myList.handleInput) {
    this.myList.handleInput(data);
    return true;
  }
  
  return false;
}
```

## Navigation Patterns

### Callback-based Navigation (Recommended)

```typescript
// In DashboardScreen
constructor(onNavigate?: (screenId: string) => void) {
  this.menuList.onSelect = (item) => {
    onNavigate?.(item.value);
  };
}

// In CLIAppTUI
const dashboard = new DashboardScreen(screenId => {
  this.showScreen(screenId);
});
```

### Direct Navigation

```typescript
// Access app instance (less preferred)
class MyScreen implements Screen {
  private app: CLIAppTUI;
  
  constructor(app: CLIAppTUI) {
    this.app = app;
  }
  
  private navigateToOther() {
    this.app.showScreen("other-screen");
  }
}
```

## Using Overlays

Overlays are modal dialogs that appear on top of the current screen.

```typescript
// Show an overlay
const overlay = new Box();
overlay.addChild(new Text("Overlay Content"));

const handle = this.tui.showOverlay(overlay, {
  anchor: "center",
  width: "50%",
  maxHeight: "50%",
});

// Hide the overlay
handle.hide();

// Temporarily hide/show
handle.setHidden(true);
handle.setHidden(false);

// Focus management
handle.focus();
handle.unfocus();
```

## State Management

### Local Screen State

```typescript
export class MyScreen implements Screen {
  private items: string[] = [];
  private selectedIndex: number = 0;

  async loadData() {
    this.items = await fetchItems();
    this.render(); // Re-render with new data
  }
}
```

### Shared Application State

For shared state, consider:
- Passing data through constructors
- Using callback functions
- Implementing an event bus (future enhancement)

## Best Practices

### 1. Keep Screens Focused

Each screen should have a single responsibility.

✅ Good:
```typescript
class WorkflowListScreen { /* only lists workflows */ }
class WorkflowDetailScreen { /* only shows details */ }
```

❌ Bad:
```typescript
class WorkflowScreen { /* does everything */ }
```

### 2. Use Lifecycle Methods

Clean up resources in `destroy()`:

```typescript
destroy() {
  // Cancel pending requests
  this.abortController.abort();
  
  // Clear intervals/timeouts
  clearInterval(this.refreshInterval);
  
  // Remove event listeners
  this.emitter.removeAllListeners();
}
```

### 3. Handle Errors Gracefully

```typescript
async loadData() {
  try {
    this.items = await fetchData();
  } catch (error) {
    this.showError(error.message);
  }
}
```

### 4. Invalidate Cache When Needed

Components cache their rendering. Call `invalidate()` when data changes:

```typescript
updateData(newData: string) {
  this.data = newData;
  this.textComponent.invalidate();
  this.tui.requestRender();
}
```

### 5. Use TypeScript Types

Always use proper types for better IDE support:

```typescript
import type { SelectItem } from "../components/select-list.js";

const items: SelectItem[] = [
  { value: "id", label: "Label", description: "Desc" }
];
```

## Debugging Tips

### Enable Verbose Logging

```bash
pnpm start --tui -v
```

### Check Terminal State

If terminal gets stuck:

```bash
# Reset terminal
reset

# Or
stty sane
```

### Inspect Component Tree

```typescript
// In your screen
console.log("Container children:", this.container.children.length);
```

## Common Issues

### Issue: Screen not rendering

**Solution**: Ensure you call `this.tui.requestRender()` after updates.

### Issue: Keyboard input not working

**Solution**: Check if component has `handleInput` method and it's being called.

### Issue: Terminal stuck in raw mode

**Solution**: Make sure `app.stop()` is called before exit.

## Testing

Write unit tests for your screens:

```typescript
import { describe, it, expect } from "vitest";
import { MyScreen } from "../my-screen.js";

describe("MyScreen", () => {
  it("should render correctly", () => {
    const screen = new MyScreen();
    const component = screen.render();
    expect(component).toBeDefined();
  });
});
```

Run tests:

```bash
pnpm test:unit src/tui/screens/__tests__/my-screen.test.ts
```

## Next Steps

After mastering Phase 4 concepts, proceed to:

- **Phase 5**: Implement Workflow Management Screen
- **Phase 6**: Implement Agent Loop Screen with real-time updates
- **Phase 7**: Integrate Human Relay with TUI Editor
- **Phase 8**: Add Settings and Configuration screens

## Resources

- [Phase 4 Complete Documentation](./PHASE4-COMPLETE.md)
- [Phase 4 Summary](./PHASE4-SUMMARY.md)
- [TUI Migration Design](./tui-migration-design.md)
- [Component Examples](../../src/tui/components/examples.ts)

---

**Happy Coding!** 🚀
