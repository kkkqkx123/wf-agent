# tui-debug

Debug and diagnostics utilities for the TUI stack.

Currently provides `DiffRecorderBackend`, a ratatui `Backend` decorator that
taps into every `draw()` call to support render performance analysis:

- Records each frame's ANSI diff bytes to a secondary output (file / buffer).
- Feeds per-frame metrics into `tui_core::frame_metrics::FrameMetrics`:
  changed cells, diff bytes, frame time, plus optional layout/parse sampling
  points reported via `note_layout()` before each draw.
- Forwards the real draw unchanged to the wrapped terminal backend.

All recording code is gated behind the `diff-record` feature flag, so
production builds carry zero overhead.

## Usage

```sh
cargo run -p wf-cli-demo --example tui_diff_record --features diff-record
```

Then inspect `diff_log.txt` (`head -40 diff_log.txt`), or convert to HTML:

```sh
cat diff_log.txt | ansi-to-html > diff_log.html
```

To instrument any ratatui event loop, wrap the real backend before creating
the `Terminal`:

```rust
let backend = DiffRecorderBackend::new(CrosstermBackend::new(stdout), File::create("diff_log.txt")?);
let mut terminal = Terminal::new(backend)?;

// Optional: report layout/parse sampling for the next frame.
terminal.backend_mut().note_layout(laid_out_rows, parsed_bytes);

terminal.draw(|f| render(f, &state))?;

// On exit: one-line summary of the recorded frames.
eprintln!("{}", terminal.backend().metrics_report());
```

Notes:

- Requires the `diff-record` feature; without it the code is not compiled.
- `note_layout()` values apply to the next drawn frame.
- The first frame's elapsed time is reported as 0.
- The log contains raw ANSI escape sequences; view via `ansi-to-html` rather
  than `cat`.

## Dependency position

Leaf-level debug crate: depends on `tui-core` (for `FrameMetrics`) and is
re-exported by `wf-tui` (the `diff-record` feature enables both
`tui-core/diff-record` and `tui-debug/diff-record`). No other crate should depend on it in reverse.
