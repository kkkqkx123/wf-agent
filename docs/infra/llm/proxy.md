# 代理与 HTTP 客户端构造

本篇覆盖 `wf-llm` 暴露的代理能力：路由校验、凭据组合、HTTP 客户端构造、传输调优。
所有客户端统一通过本模块构造，避免行为漂移。

## 核心函数

- `has_userinfo(url)`：检测路由地址是否内嵌 userinfo 凭据。
- `reject_userinfo(url)`：拒绝内嵌凭据的持久化路由（返回 `ProxyError::UserinfoForbidden`）。
- `credential_key(route)`：代理凭据的存储键（`proxy:{route}`）。
- `compose_proxy_url(route, username, password)`：把路由与存储凭据合成内存地址（percent-encode），
  结果仅瞬态使用，不持久化。
- `build_proxy(url, no_proxy)`：构造 `reqwest::Proxy`，支持 bypass 列表。
- `build_http_client(timeout, proxy, no_proxy)` / `build_http_client_with_options(..., options)`：
  共享客户端构造入口。

## 支持的 scheme

`http`, `https`, `socks4`, `socks4a`, `socks5`, `socks5h`。

## 传输调优

`HttpTransportOptions`：

- `connect_timeout_secs`：TCP/TLS 连接超时（秒）。
- `danger_accept_invalid_certs`：接受无效证书（自签名本地网关），显式 opt-in；网关构建时记录警告。

## 语义约定

- **凭据不落日志**：代理 URL 常含凭据；错误消息绝不包含 URL 或凭据。
- **持久化路由禁止 userinfo**：密钥存 credential store，键由 `credential_key` 派生；内存合成地址是 userinfo 唯一可出现处。
- **代理优先级**：显式配置 > 环境变量 > 直连。配置了 URL 即禁用环境代理；未配置时保留 reqwest 默认（读取 `ALL_PROXY` 等）。
- **bypass 列表**：与代理并列配置，由 host 决定哪些流量不走代理；空条目被丢弃。
- **模型发现与网关一致**：`ModelCatalog` 与网关共用 `build_http_client_with_options` 与 `ProxyError` 语义。
