# git 域 HTTP 端点
> 操作数：19
| Method | Path | Handler (operationId) | Parameters / Body |
|--------|------|------------------------|-------------------|
| POST | `/api/v1/git/action` | `post_git_action` | body |
| GET | `/api/v1/git/branches` | `get_git_branches` |  |
| GET | `/api/v1/git/commits` | `get_git_commits` | limit (q), offset (q), branch (q) |
| GET | `/api/v1/git/commits/files` | `get_git_commit_files` | commit (q) |
| GET | `/api/v1/git/diff` | `get_git_diff` | path (q) |
| GET | `/api/v1/git/diff/range` | `get_git_diff_range` | from (q), to (q), path (q) |
| GET | `/api/v1/git/diff/staged` | `get_git_diff_staged` | path (q) |
| GET | `/api/v1/git/file` | `get_git_file` | path (q), rev (q) |
| GET | `/api/v1/git/files` | `get_git_files` |  |
| GET | `/api/v1/git/fingerprint` | `get_git_fingerprint` |  |
| GET | `/api/v1/git/meta` | `get_git_meta` |  |
| GET | `/api/v1/git/reflog` | `get_git_reflog` |  |
| GET | `/api/v1/git/registry` | `get_git_registry` |  |
| GET | `/api/v1/git/remotes` | `get_git_remotes` |  |
| GET | `/api/v1/git/stash` | `get_git_stash` |  |
| GET | `/api/v1/git/status` | `get_git_status` |  |
| GET | `/api/v1/git/submodules` | `get_git_submodules` |  |
| GET | `/api/v1/git/tags` | `get_git_tags` |  |
| GET | `/api/v1/git/worktrees` | `get_git_worktrees` |  |
