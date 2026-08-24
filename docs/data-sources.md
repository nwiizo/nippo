# データソース仕様

Claude Code / Codex / GitHub Copilot CLI のセッションデータの保存場所と形式。

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
- `state_5.sqlite`: thread の `cwd` / `git_branch` / `rollout_path` などのメタデータ
- `rollout_path` が指す rollout JSONL: assistant メッセージ、ツール呼び出し、トークン使用量、変更ファイル
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
SELECT id, cwd, git_branch, rollout_path FROM threads;
```

- `id`: history の `session_id` と対応
- `cwd`: プロジェクトパス
- `git_branch`: ブランチ名
- `rollout_path`: assistant 側の rollout JSONL へのパス

## GitHub Copilot CLI の保存場所

```
~/.copilot/session-state/{session-id}/events.jsonl
~/.copilot/session-state/{session-id}/workspace.yaml
```

- `events.jsonl`: セッション再開に使われる完全なイベントログ。`nippo` の主データソース
- `workspace.yaml`: `cwd` / `branch` / `repository` のメタデータ補完
- `session-store.db`: 横断検索用インデックス。`nippo` では主データソースに使わない

### Copilot イベント（収集対象）

| type | 用途 | nippo での扱い |
|------|------|---------------|
| `session.start` | session ID と workspace context | メタデータとして使用 |
| `session.context_changed` | cwd / repository / branch の変更 | メタデータとして使用 |
| `user.message` | ユーザーメッセージ | **収集対象** |
| `assistant.message` | 完成したアシスタント応答 | **収集対象** |
| `assistant.usage` | API 呼び出しの token 使用量 | 保存されている場合のみ input token を収集 |
| `tool.execution_start` | 実行されたツールと引数 | **収集対象** |
| その他 | reasoning、進捗、権限、tool result 等 | スキップ |

全イベント共通の `timestamp` は ISO 8601。`data.content`、`data.toolName`、
`data.arguments` など、イベント型ごとの確定フィールドだけを読む。`agentId` がある
サブエージェントイベントは、Claude Code の sidechain と同様に二重集計を避けるため除外する。

Copilot の永続ログでは ephemeral な `assistant.usage` が存在しないことがある。
その場合、`assistant.message.data.outputTokens` は集計できるが input token は 0 のままになる。

## コレクター CLI オプション

```bash
nippo collect [OPTIONS]
```

| オプション | 説明 | デフォルト |
|-----------|------|----------|
| `--days N` | 今日を含む過去N日分を収集（ローカル日付基準、0 = 全期間） | `1` |
| `--from YYYY-MM-DD` | 開始日（`--days` より優先） | なし |
| `--to YYYY-MM-DD` | 終了日 | なし（今日） |
| `--period PERIOD` | 名前付き期間（`--days` より優先） | なし |
| `--project NAME` | プロジェクト名でフィルタ（部分一致） | なし |
| `--stats-only` | セッション詳細を省略し統計のみ出力 | `false` |
| `--include-prompt-noise` | 定型通知・画像プレースホルダ・短い肯定応答なども含める | `false` |
| `--include-self` | コマンドを実行しているホストエージェントのセッションも含める | `false` |
| `--max-sessions N` | 出力するセッション数の上限（0 = 無制限） | `0` |
| `--format json\|summary` | 出力形式 | `json` |
| `--source auto\|claude\|codex\|copilot\|all` | データソース選択 | `auto` |
| `--claude-dir PATH` | Claude データディレクトリ | `~/.claude` |
| `--codex-dir PATH` | Codex データディレクトリ | `~/.codex` |
| `--copilot-dir PATH` | GitHub Copilot データディレクトリ | `~/.copilot` |

`--period` の値: `today`, `yesterday`, `this-week`, `last-week`, `week-before-last`, `this-month`, `last-month`, `month-before-last`

優先順位: `--period` > `--from`/`--to` > `--days`

日付境界は実行環境のローカルタイムゾーン基準。`--days 1` は「今日」、`--days 7` は「今日を含む過去7日」を意味する。

JSON の `meta.period.from` と `meta.period.to` は指定した日付範囲の両端を含む。
`meta.period.timezone` は境界の基準になった IANA タイムゾーン名を返し、OS から名前を
取得できない場合は UTC オフセットを返す。`--days 0` では両端が `null` になる。
`--from` だけを指定した場合の終了日は今日、`--to` だけを指定した場合の開始日は `null` になる。
プロジェクト総数は `stats.projects_worked_on` の要素数から求める。

既定では、スラッシュコマンド展開、ハーネス通知、画像プレースホルダ、中断通知、
compact の導入文、短い肯定応答を user エントリから除外する。対応する assistant エントリは
同じ ID の通常プロンプトと統合されている場合だけ、ツール使用や変更ファイルの集計に残る。
統合後に意味のある user prompt がないセッションは、定期実行やコマンド展開だけの記録として
セッション全体を除外する。除外前の記録が必要な場合は `--include-prompt-noise` を指定する。

収集後は同じ `session_id` の記録を 1 件に統合する。`time_range` は最初から最後まで、
プロンプトは timestamp と本文の組み合わせで重複除去する。assistant エントリは全項目が
一致する重複を除外してから、メッセージ・ツール使用・トークンを合算する。
`--max-sessions` は統合後のセッションを最新順に並べてから適用する。

`CLAUDE_CODE_SESSION_ID`、`CODEX_THREAD_ID`、`COPILOT_SESSION_ID` のいずれかが設定されている場合、その値と
完全一致するセッションは既定で除外する。時刻やメッセージ数を使った推測では除外しない。

JSON の `render_helpers` はローカル時刻のセッション範囲、30 分未満の間隔でつながる
最長作業ブロック、使用回数上位 5 ツールを返す。元データから毎回算出する表示補助で、
`sessions` と `stats` の既存項目は維持する。

`--source auto` の判定:

- `COPILOT_SESSION_ID` があり Copilot データもあるときは `copilot`
- `CODEX_THREAD_ID` があるときは `codex`
- それ以外は Claude Code のデータがあれば `claude`
- Claude がなく Codex があれば `codex`
- Claude と Codex がなく Copilot があれば `copilot`
- 利用可能なソースをすべて混ぜたいときは `--source all`
