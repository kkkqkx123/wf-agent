# 硅基流动 Rerank 重排序 API
# 运行方式（仓库根目录，密钥经 .env 注入，不硬编码）：
#   set -a; . ./.env; set +a
#   hurl tools/hurl/silicon-rerank.hurl --variable SILICONFLOW_API_KEY="$LIBREF_EMBEDDING_API_KEY" --insecure
POST https://api.siliconflow.cn/v1/rerank
Authorization: Bearer {{SILICONFLOW_API_KEY}}
Content-Type: application/json
{
    "model": "BAAI/bge-reranker-v2-m3",
    "query": "Apple",
    "documents": ["apple", "banana", "fruit", "vegetable"],
    "return_documents": true,
    "top_n": 4
}

# 断言接口正常返回 200
HTTP 200