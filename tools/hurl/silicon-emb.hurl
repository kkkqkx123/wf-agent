# SiliconFlow Embeddings API Debug
# 运行方式（仓库根目录，密钥经 .env 注入，不硬编码）：
#   set -a; . ./.env; set +a
#   hurl tools/hurl/silicon-emb.hurl --variable SILICONFLOW_API_KEY="$LIBREF_EMBEDDING_API_KEY" --insecure

POST https://api.siliconflow.cn/v1/embeddings
Authorization: Bearer {{SILICONFLOW_API_KEY}}
Content-Type: application/json
{
    "input": "Hello, world!",
    "model": "BAAI/bge-m3"
}

# 断言响应状态码为 200
HTTP 200