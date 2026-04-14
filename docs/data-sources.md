# データソース仕様

Claude Code / Codex のセッションデータの保存場所と形式。

## 保存場所

```
~/.claude/projects/{project-dir}/{session-uuid}.jsonl
~/.claude/projects/{project-dir}/subagents/agent-{id}.jsonl
```

- `{project-dir}`: 作業ディレクトリの絶対パスを `-` 区切りに変換したもの
  - 例: `/Users/nwiizo/ghq/github.com/nwiizo/nippo` → `-Users-nwiizo-ghq-github-com-nwiizo-nippo`
- `{session-uuid}`: セッション固有の UUID（例: `12141d3c-d109-4410-a6af-bbcd1e1f0755`）
- `subagents/`: サブエージェント（Agent ツール）の個別セッション

## JSONL 形式

1行1JSON オブジェクト。各行は独立した完全な JSON。

## エントリ型（トップレベル `type` フィールド）

| type | 用途 | nippo での扱い |
|------|------|---------------|
| `user` | ユーザーメッセージ | **収集対象** |
| `assistant` | アシスタント応答 | **収集対象** |
| `queue-operation` | セッション管理 | スキップ |
| `progress` | 進捗通知 | スキップ |
| `file-history-snapshot` | ファイル状態記録 | スキップ |
| `system` | システムメッセージ | スキップ |
| `last-prompt` | 最終プロンプト記録 | スキップ |

## user エントリ

```json
{
  "type": "user",
  "userType": "external",
  "cwd": "/path/to/working/directory",
  "sessionId": "{uuid}",
  "gitBranch": "main",
  "version": "2.1.51",
  "timestamp": "2026-03-21T03:31:05.087Z",
  "uuid": "{message-uuid}",
  "parentUuid": "{parent-uuid}",
  "isSidechain": false,
  "message": {
    "role": "user",
    "content": "string or array of content blocks"
  }
}
```

## assistant エントリ

```json
{
  "type": "assistant",
  "cwd": "/path/to/working/directory",
  "sessionId": "{uuid}",
  "gitBranch": "main",
  "timestamp": "2026-03-21T03:31:11.083Z",
  "message": {
    "role": "assistant",
    "model": "claude-opus-4-6",
    "content": [/* content blocks */],
    "usage": {
      "input_tokens": 9411,
      "output_tokens": 200
    }
  }
}
```

## content ブロック型

| type | 内容 |
|------|------|
| `text` | テキスト応答（`{type: "text", text: "..."}`) |
| `tool_use` | ツール呼び出し（`{type: "tool_use", name: "Read", input: {...}}`) |
| `tool_result` | ツール実行結果 |
| `thinking` | 思考ブロック（内部推論） |

user メッセージの content は `string`（単純テキスト）または `array`（content ブロック配列）のどちらか。

## Codex の保存場所

```
~/.codex/history.jsonl
~/.codex/state_5.sqlite
~/.codex/logs_2.sqlite
```

- `history.jsonl`: user prompt 履歴。`nippo` での Codex 収集の主データソース
- `state_5.sqlite`: thread の `cwd` / `git_branch` / `model` などのメタデータ
- `logs_2.sqlite`: 内部診断ログ。`nippo` では**日報の主データソースに使わない**

## Codex history.jsonl エントリ

```json
{
  "session_id": "019d8a74-1bc5-7f70-ae36-35d80a42681f",
  "ts": 1776144399,
  "text": "https://github.com/nwiizo/nippo/issues/6 を参考に修正してほしいです。"
}
```

- `session_id`: thread ID
- `ts`: Unix timestamp（秒）
- `text`: user prompt

## Codex threads テーブル（使用列）

```sql
SELECT id, cwd, git_branch, model FROM threads;
```

- `id`: history の `session_id` と対応
- `cwd`: プロジェクトパス
- `git_branch`: ブランチ名
- `model`: 使用モデル（現状は集計には未使用）

## コレクター CLI オプション

```bash
nippo collect [OPTIONS]
```

| オプション | 説明 | デフォルト |
|-----------|------|----------|
| `--days N` | 過去N日分を収集（0 = 全期間） | `1` |
| `--from YYYY-MM-DD` | 開始日（`--days` より優先） | なし |
| `--to YYYY-MM-DD` | 終了日 | なし（今日） |
| `--period PERIOD` | 名前付き期間（`--days` より優先） | なし |
| `--project NAME` | プロジェクト名でフィルタ（部分一致） | なし |
| `--stats-only` | セッション詳細を省略し統計のみ出力 | `false` |
| `--format json\|summary` | 出力形式 | `json` |
| `--source auto\|claude\|codex\|all` | データソース選択 | `auto` |
| `--claude-dir PATH` | Claude データディレクトリ | `~/.claude` |
| `--codex-dir PATH` | Codex データディレクトリ | `~/.codex` |

`--period` の値: `today`, `yesterday`, `this-week`, `last-week`, `week-before-last`, `this-month`, `last-month`, `month-before-last`

優先順位: `--period` > `--from`/`--to` > `--days`

`--source auto` の判定:

- `CODEX_THREAD_ID` があるときは `codex`
- それ以外は Claude Code のデータがあれば `claude`
- Claude がなく Codex があれば `codex`
- 明示的に両方混ぜたいときだけ `--source all`
