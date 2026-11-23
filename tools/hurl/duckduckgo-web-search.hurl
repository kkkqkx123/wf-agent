# DuckDuckGo HTML web_search reference request.
#
# Hurl has no per-request proxy directive; the proxy is a CLI/global setting.
# Run from the repository root with the local proxy on port 7890:
#
#   hurl --proxy http://127.0.0.1:7890 --test --max-time 30 \
#     tools/hurl/duckduckgo-web-search.hurl
#
# libcurl also honors the environment, so this is equivalent:
#
#   https_proxy=http://127.0.0.1:7890 http_proxy=http://127.0.0.1:7890 \
#     hurl --test --max-time 30 tools/hurl/duckduckgo-web-search.hurl
#
# No credentials are stored here; add them to the proxy URL only at runtime
# if the operator requires an authenticated proxy.

GET https://html.duckduckgo.com/html/?q=Rust+async+programming
User-Agent: wf-agent/0.1 (Modular Agent Framework)
Accept: text/html,application/xhtml+xml
Accept-Language: en-US,en;q=0.9

# Structural assertions only: search rankings and snippets are volatile.
HTTP 200
[Asserts]
header "Content-Type" contains "text/html"
xpath "//a[contains(@class, 'result__a')]" exists
xpath "//a[contains(@class, 'result__snippet')]" exists
body contains "result__a"
body contains "result__snippet"
duration < 20000
