# TUI 前端配置（tui.toml）

文件：`$XDG_CONFIG_HOME/wf/tui.toml` 或 `$HOME/.config/wf/tui.toml`（仅 CLI/TUI 前端，不参与服务端配置）。

顶层段：`behavior`、`terminal`、`mini`、`tool_output`、`appearance`、`rendering`、`motion`、`notifications`、`history`。

- `behavior`：`mouse_capture`、`copy_on_select`、`session_scrollbar`、`sidebar`、`approval_timeout_secs`、`recap`、`double_escape_action`
- `terminal`：`copy_target`、`cursor_style`、`cursor_blinking`、`scroll_speed`、`scroll_acceleration`、`keyboard_enhancement`、`show_terminal_progress`、`auto_title_mode`、`title_model`
- `mini`：`markdown`、`show_reasoning`、`show_usage`、`show_tools`、`approval_timeout_secs`、`suggestion_display_limit`、`history_limit`
- `tool_output`：预览行数/字符数、`level`、`tool_call_details`、`compact_notifications`、`show_bash_output`、`show_agentgrep_output`、`diff_line_wrap`
- `appearance`：`theme`、`colors`、`emoji`、`centered`、动画开关、`keybinding_hints`
- `rendering`：思考/推理显示、markdown 间距、LaTeX、图、图片/待办固定、usage 显示、overscroll 状态、diff 模式、外部会话等
- `motion`：`mode`、`disabled_animations`、`animation_fps`、`redraw_fps`、`performance`、`sidebar_refresh_interval_ms`
- `notifications`：启用、回合完成、终端/系统/铃声、背景 only、声音、音量、音效包
- `history`：`replay_max_rows`、`debug_socket`、`slow_frame_ms`

缺失或损坏文件回退默认值，不会阻断启动。
