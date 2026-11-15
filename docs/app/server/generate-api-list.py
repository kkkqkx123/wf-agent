import json, os, re

BASE='/home/kkkqkx/code/wf-agent'
OPENAPI=f'{BASE}/crates/app/wf-server/openapi.json'
OUT=f'{BASE}/docs/app/server'
d=json.load(open(OPENAPI))

# collect ops
ops=[]
for path,methods in d['paths'].items():
    for method,op in methods.items():
        if method.lower() not in ('get','post','put','patch','delete'):
            continue
        tag=(op.get('tags') or ['untagged'])[0]
        ops.append({'method':method.upper(),'path':path,'tag':tag,'op':op})

def params_summary(op):
    parts=[]
    for x in op.get('parameters',[]) or []:
        loc = {'query':'q','path':'p','header':'h'}.get(x.get('in'),'')
        name=x.get('name','')
        parts.append(f"{name} ({loc})")
    body=''
    if 'requestBody' in op:
        body='body'
    s=', '.join(parts)
    if body:
        s = (s+', ' if s else '') + body
    return s

def esc(s):
    return str(s).replace('|','\\|').replace('\n',' ')

# group
from collections import defaultdict, OrderedDict
groups=defaultdict(list)
for o in ops:
    groups[o['tag']].append(o)

# tag order (logical)
order=['system','workflow','agent','checkpoint','template','llm','observation','entity','git','chat','web','fs','trigger','pty','rag','cli']
# include any missing
for t in groups:
    if t not in order:
        order.append(t)

# file naming
tag_files={
 'system':'02-system-and-health.md',
 'workflow':'03-workflow.md',
 'agent':'04-agent.md',
 'checkpoint':'05-checkpoint-and-file.md',
 'template':'06-template.md',
 'llm':'07-llm.md',
 'observation':'08-observation-query-analysis.md',
 'entity':'09-entity.md',
 'git':'10-git.md',
 'chat':'11-chat.md',
 'web':'12-web.md',
 'fs':'13-fs.md',
 'trigger':'14-trigger.md',
 'pty':'15-pty.md',
 'rag':'16-rag.md',
 'cli':'17-cli.md',
}

os.makedirs(OUT,exist_ok=True)

# README
readme='''# Server HTTP API 清单

> 范围：`crates/app/wf-server` HTTP 传输层暴露的全部端点。
> 来源：`crates/app/wf-server/openapi.json`（OpenAPI 3.0 快照，658 个操作 / 572 个路径）。
> 基础前缀：业务端点挂载在 `/api/v1` 下；`/`、`/health`、`/metrics`、`/api/v1/ws`、`/api/v1/info` 等系统端点按各自路径。
> 版本：`v1`（`API_VERSION`）。

## 通用约定

- **鉴权**：除公开健康检查外，所有端点接受 `x-api-key` 请求头或 `api_key` 查询参数；WebSocket 通过 `x-api-key` 头或 `api_key` 查询参数。
- **信封**：JSON 响应统一为 `{ "success": bool, "data": ..., "error": { "code", "message" } | null }`。
- **错误码映射**：NotFound/ExecutionNotFound → 404 `NOT_FOUND`；Validation → 400 `INVALID_PARAMS`；AlreadyExists → 409 `ALREADY_EXISTS`；Conflict → 409 `CONFLICT`；Timeout → 504 `TIMEOUT`；Storage → 500 `STORAGE_ERROR`；Execution → 500 `INTERNAL_ERROR`；未授权 401/无权限 403/限流 429（带 `Retry-After`）/SSE 限额 503。
- **SSE**：5 个端点返回 `text/event-stream`（`/events/stream`、`/workflows/{id}/execute/stream`、`/executions/{id}/error-analysis/stream`、`/agent-loops/{id}/stream`、`/llm/generate-stream`）。
- **WebSocket**：`GET /api/v1/ws`，支持 subscribe/unsubscribe/ping。
- **目录作用域**：请求可通过 `x-wf-directory` 头或 `directory` 查询参数指定目录；无默认根目录。

## 文档索引

| 文件 | 域 | 操作数 |
|------|----|--------|
'''
for t in order:
    cnt=len(groups[t])
    readme += f"| [{tag_files[t]}]({tag_files[t]}) | {t} | {cnt} |\n"
readme += "\n## 生成说明\n\n本目录由 `crates/app/wf-server/openapi.json` 自动生成；如端点变更，请重新生成。\n"
open(f'{OUT}/README.md','w').write(readme)

# generate per tag
for t in order:
    fname=tag_files[t]
    items=sorted(groups[t], key=lambda x:(x['path'],x['method']))
    lines=[]
    lines.append(f"# {t} 域 HTTP 端点\n")
    lines.append(f"> 操作数：{len(items)}\n")
    lines.append("| Method | Path | Handler (operationId) | Parameters / Body |\n")
    lines.append("|--------|------|------------------------|-------------------|\n")
    for it in items:
        op=it['op']
        oid=op.get('operationId','')
        ps=params_summary(op)
        lines.append(f"| {it['method']} | `{it['path']}` | `{oid}` | {esc(ps)} |\n")
    open(f'{OUT}/{fname}','w').write(''.join(lines))

print('done', len(ops))
