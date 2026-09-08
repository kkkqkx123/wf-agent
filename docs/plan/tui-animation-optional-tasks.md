# TUI 动画效果可选任务文档

## 1. 概述

本文档详细说明 Codex TUI 中动画效果的具体实现位置和实现细节，作为后续可选任务的参考。这些动画效果不纳入本次实施计划，但可以作为后续改进的参考。

## 2. Codex 动画效果实现位置

### 2.1 Markdown 渲染动画

**Codex 位置：** `ref/codex/tui/src/render/markdown_render.rs`

**实现细节：**

1. **语法高亮动画**
   - 代码块中的语法元素动画
   - 支持不同编程语言的语法高亮
   - 动态颜色过渡效果

2. **代码块动画**
   - 代码块的渐入效果
   - 语法元素的逐个高亮
   - 支持 TrueColor 和 ANSI 256 色

**具体实现：**

```rust
// markdown_render.rs 中的动画相关代码
pub struct MarkdownRenderer {
    // 动画状态
    animation_state: AnimationState,
    // 语法高亮缓存
    highlight_cache: HashMap<String, Vec<Span<'static>>>,
}

impl MarkdownRenderer {
    pub fn render_code_block(&self, code: &str, language: &str) -> Vec<Line<'static>> {
        // 语法高亮
        let highlighted = self.highlight_code(code, language);
        // 应用动画效果
        self.apply_animation(highlighted)
    }
    
    fn apply_animation(&self, spans: Vec<Span<'static>>) -> Vec<Line<'static>> {
        // 基于时间的动画效果
    }
}
```

### 2.2 历史记录单元动画

**Codex 位置：** `ref/codex/tui/src/history_cell.rs`

**实现细节：**

1. **HistoryCell 动画信号**
   - `transcript_animation_tick()` 方法
   - 支持时间相关输出的重新渲染
   - 动画状态管理

2. **不同历史单元类型的动画**
   - 用户消息的渐入效果
   - 助手回复的打字机效果
   - 工具调用的状态动画

**具体实现：**

```rust
// history_cell.rs 中的动画相关代码
pub trait HistoryCell {
    fn display_lines(&self, width: u16) -> Vec<Line<'static>>;
    fn raw_lines(&self, width: u16) -> Vec<String>;
    fn transcript_lines(&self, width: u16) -> Vec<Line<'static>>;
    fn desired_height(&self, width: u16) -> u16;
    fn transcript_animation_tick(&mut self);  // 动画信号
}

pub struct AssistantHistoryCell {
    text: String,
    rendered: Option<Vec<Line<'static>>>,
    animation_state: AnimationState,  // 动画状态
}

impl HistoryCell for AssistantHistoryCell {
    fn transcript_animation_tick(&mut self) {
        // 更新动画状态
        self.animation_state.tick();
        // 重新渲染
        self.rendered = None;
    }
}
```

### 2.3 底部面板动画

**Codex 位置：** `ref/codex/tui/src/bottom_pane/`

**实现细节：**

1. **状态指示器动画**
   - 加载状态的旋转动画
   - 进度条动画
   - 状态文本的闪烁效果

2. **审批覆盖层动画**
   - 覆盖层的渐入/渐出效果
   - 按钮的悬停动画
   - 状态转换动画

3. **选择视图动画**
   - 列表项的选中动画
   - 滚动动画
   - 过渡效果

**具体实现：**

```rust
// bottom_pane/status_indicator.rs 中的动画相关代码
pub struct StatusIndicatorWidget {
    // 动画状态
    animation: AnimationState,
    // 旋转帧
    rotation_frame: usize,
}

impl StatusIndicatorWidget {
    pub fn tick(&mut self) {
        // 更新旋转帧
        self.rotation_frame = (self.rotation_frame + 1) % SPINNER_FRAMES.len();
        // 更新动画状态
        self.animation.tick();
    }
    
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        // 渲染动画效果
        let frame = SPINNER_FRAMES[self.rotation_frame];
        // 应用动画样式
    }
}
```

### 2.4 转录覆盖层动画

**Codex 位置：** `ref/codex/tui/src/transcript.rs`

**实现细节：**

1. **时间相关输出的重新渲染**
   - 基于时间戳的动画更新
   - 支持 Ctrl+T 触发的全屏覆盖层动画
   - 缓存机制：`ActiveCellTranscriptKey`

2. **覆盖层过渡动画**
   - 覆盖层的打开/关闭动画
   - 内容滚动动画
   - 状态指示器动画

**具体实现：**

```rust
// transcript.rs 中的动画相关代码
pub struct TranscriptState {
    // 动画状态
    animation: AnimationState,
    // 缓存键
    cache_key: Option<ActiveCellTranscriptKey>,
}

impl TranscriptState {
    pub fn animation_tick(&mut self) {
        // 更新动画状态
        self.animation.tick();
        // 检查是否需要重新渲染
        if self.animation.should_rerender() {
            self.cache_key = None;  // 使缓存失效
        }
    }
    
    pub fn render_overlay(&self, area: Rect, buf: &mut Buffer) {
        // 渲染动画效果
    }
}
```

## 3. 实现指导

### 3.1 动画系统架构

**核心组件：**

1. **AnimationState**
   ```rust
   pub struct AnimationState {
       tick: u64,
       mode: AnimationMode,
       start_time: Instant,
   }
   
   pub enum AnimationMode {
       Animated,      // 动画模式
       Reduced,       // 减少动画模式（无障碍）
       Static,        // 静态模式
   }
   ```

2. **动画信号系统**
   ```rust
   pub trait Animatable {
       fn animation_tick(&mut self);
       fn should_animate(&self) -> bool;
   }
   ```

3. **动画缓存**
   ```rust
   pub struct AnimationCache {
       entries: HashMap<CacheKey, CachedAnimation>,
       max_size: usize,
   }
   ```

### 3.2 动画效果类型

1. **渐变动画**
   - 渐入/渐出效果
   - 颜色过渡
   - 透明度变化

2. **位移动画**
   - 滚动效果
   - 滑入/滑出
   - 弹跳效果

3. **旋转动画**
   - 旋转指示器
   - 脉冲效果
   - 闪烁效果

4. **缩放动画**
   - 大小变化
   - 弹性效果
   - 压缩/拉伸

### 3.3 性能优化

1. **缓存机制**
   - 缓存动画帧
   - 避免重复计算
   - 使用增量更新

2. **帧率控制**
   - 支持可变帧率
   - 根据终端能力调整
   - 减少不必要的重绘

3. **无障碍支持**
   - 支持减少动画模式
   - 检测系统偏好
   - 提供静态回退

## 4. 测试策略

### 4.1 单元测试

```rust
#[test]
fn animation_state_tick_updates_correctly() {
    let mut state = AnimationState::new();
    state.tick();
    assert_eq!(state.tick, 1);
}

#[test]
fn animation_cache_stores_and_retrieves() {
    let mut cache = AnimationCache::new();
    let key = CacheKey::new("test");
    let animation = CachedAnimation::new();
    cache.insert(key.clone(), animation);
    assert!(cache.get(&key).is_some());
}
```

### 4.2 集成测试

```rust
#[test]
fn markdown_renderer_applies_animation() {
    let renderer = MarkdownRenderer::new();
    let code = "fn main() {}";
    let lines = renderer.render_code_block(code, "rust");
    assert!(!lines.is_empty());
}
```

### 4.3 性能测试

```rust
#[test]
fn animation_performance_under_16ms() {
    let mut state = AnimationState::new();
    let start = Instant::now();
    for _ in 0..60 {
        state.tick();
    }
    let elapsed = start.elapsed();
    assert!(elapsed < Duration::from_millis(16 * 60)); // 60帧 < 1秒
}
```

## 5. 实施建议

### 5.1 优先级

1. **高优先级**
   - 任务执行过程中的动画（已纳入核心组件）
   - 任务执行状态指示器动画（已纳入核心组件）
   - 加载指示器动画（已纳入核心组件）

2. **中优先级**
   - Markdown渲染动画
   - 历史记录单元动画

3. **低优先级**
   - 底部面板动画
   - 转录覆盖层动画

### 5.2 实施步骤

1. **第一阶段：核心动画系统**
   - 实现 `AnimationState` 和 `AnimationMode`
   - 实现动画信号系统
   - 实现基础动画效果

2. **第二阶段：组件动画**
   - 实现 Markdown渲染动画
   - 实现历史记录单元动画
   - 实现底部面板动画

3. **第三阶段：高级动画**
   - 实现转录覆盖层动画
   - 优化性能
   - 添加无障碍支持

### 5.3 注意事项

1. **性能考虑**
   - 动画不应该影响TUI的响应性
   - 使用缓存避免重复计算
   - 支持减少动画模式

2. **兼容性考虑**
   - 支持不同终端的颜色能力
   - 提供静态回退
   - 检测系统动画偏好

3. **用户体验**
   - 动画应该增强而不是干扰用户体验
   - 提供禁用动画的选项
   - 支持键盘快捷键控制动画

## 6. 总结

本文档详细说明了 Codex TUI 中动画效果的具体实现位置和实现细节。这些动画效果作为可选任务，可以作为后续改进的参考。建议按照优先级和实施步骤逐步实现，确保动画效果既美观又不影响性能和用户体验。