# SiliconFlow Chat API Debug
# 运行方式（仓库根目录，密钥经 .env 注入，不硬编码）：
#   set -a; . ./.env; set +a
#   hurl tools/hurl/silicon-chat.hurl --variable SILICONFLOW_API_KEY="$LIBREF_EMBEDDING_API_KEY" --insecure

POST https://api.siliconflow.cn/v1/chat/completions
Authorization: Bearer {{SILICONFLOW_API_KEY}}
Content-Type: application/json
{
    "model": "Qwen/Qwen3.5-4B",
    "messages": [
        {"role": "user", "content": "Hello, world!"}
    ],
    "stream": false,
    "enable_thinking": true
}

HTTP 200
[Asserts]
jsonpath "$.choices" exists
jsonpath "$.choices[0].message.content" exists