# native-echo-c 示例插件

最小 C 实现的原生插件，对照契约见 `wf-plugin-sdk/c-api/wf_plugin.h`。

## 构建

```sh
make
```

产物为 `native_echo.so`；macOS 改名为 `native_echo.dylib` 并同步修改
`plugin.toml` 的入口名，Windows 改名为 `native_echo.dll`。

## 安装试运行

原生后端默认关闭，仅用于完全受信的第一方扩展。开启后按以下方式试运行：

```sh
mkdir -p ~/.wf/plugins/native-echo-c
cp native_echo.so ~/.wf/plugins/native-echo-c/
cp plugin.toml ~/.wf/plugins/native-echo-c/
```

该插件注册一个名为 `echo` 的工具类型，分发固定返回工具结果 JSON。
生命周期钩子全部返回成功。

## 实现要点

* 只用 C 标准库实现全部导出符号，不依赖 Rust 工具链，证明边界已统一为 C ABI。
* 变长返回统一两段式：空指针查尺寸，容量不足回告所需尺寸并由宿主重试。
* 插件只读宿主字符串、只写宿主缓冲，不传递需对方释放的内存。
* 清单以内嵌 TOML 字符串形式提供，与 `plugin.toml` 的标识与版本保持一致。
