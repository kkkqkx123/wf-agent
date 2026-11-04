# 按键绑定配置（keymap-settings.toml）

- 全局：`{settings_dir}/keymap-settings.toml`
- 项目：`{project}/.wf/keymap-settings.toml`（优先）
- 格式：TOML，`[keymap.<context>]` 下 `action = "chord"` 字符串
- chord 示例：`"ctrl+shift+s"`、`"esc"`、`"?"`
- 命名键：enter、esc、tab、up/down/left/right、home、end、pageup、pagedown、backspace、delete、space、f1-f12 等
- 修饰键：ctrl/control、alt/option、shift，顺序与大小写不敏感
- 校验：非法 chord 与同一 context 内同一 chord 绑定多个 action 会报告问题
