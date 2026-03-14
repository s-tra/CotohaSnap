# KotohaSnap 仕様書

バージョン: 0.9.9
最終更新: 2026-03-14

---

## 目次

1. [プロバイダー](#1-プロバイダー)
2. [ファイル監視](#2-ファイル監視)
3. [画像処理](#3-画像処理)
4. [OSC 送信](#4-osc-送信)
5. [設定項目](#5-設定項目)
6. [Tauri コマンド](#6-tauri-コマンド)
7. [イベント（バックエンド → フロントエンド）](#7-イベントバックエンド--フロントエンド)
8. [翻訳ログ](#8-翻訳ログ)
9. [UI 構成](#9-ui-構成)
10. [その他](#10-その他)

---

## 1. プロバイダー

| プロバイダー | チャット補完 URL | モデル取得 URL | デフォルトモデル |
|---|---|---|---|
| Anthropic | `https://api.anthropic.com/v1/messages` | `https://api.anthropic.com/v1/models` | claude-haiku-4-5-20251001 |
| OpenAI | `https://api.openai.com/v1/chat/completions` | `https://api.openai.com/v1/models` | gpt-4o |
| Groq | `https://api.groq.com/openai/v1/chat/completions` | `https://api.groq.com/openai/v1/models` | meta-llama/llama-4-scout-17b-16e-instruct |
| Google (Gemini) | `https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent?key={key}` | `https://generativelanguage.googleapis.com/v1beta/models` | gemini-flash-latest |
| カスタム (OpenAI 互換) | ユーザー定義 | ユーザー定義（任意） | ユーザー定義 |

**共通設定**

- リクエストタイムアウト: 60 秒
- コネクションタイムアウト: 10 秒
- max_tokens: 1024
- Anthropic API バージョンヘッダー: `2023-06-01`

---

## 2. ファイル監視

- 対象拡張子: `.png`（大文字小文字問わず）
- 監視方式: 再帰的（サブフォルダも含む）
- ライブラリ: `notify` crate

**重複除外**

同一パスから 5 秒以内のイベントはスキップ。最終検出時刻を `HashMap<PathBuf, Instant>` で管理し、5 秒経過後のエントリは自動削除。

**書き込み完了待機**

ファイル検出後、`file_ready_wait_ms`（デフォルト 200 ms）待機してから翻訳を開始する。

**検出対象イベント種別**

- `EventKind::Create(_)`
- `EventKind::Modify(ModifyKind::Name(RenameMode::To | RenameMode::Any))`
- `EventKind::Modify(ModifyKind::Data(_))`
- `EventKind::Any`

---

## 3. 画像処理

### API 送信用画像

| 条件 | 処理 |
|---|---|
| 2 MB 以下 | そのまま送信（元の MIME タイプを維持） |
| 2 MB 超 | 最大辺 1920 px にリサイズして JPEG (q=85) で再エンコード |

リサイズにはアスペクト比を保持する `image::resize` を使用（`FilterType::Triangle`）。CPU バウンド処理は `tokio::task::spawn_blocking` で実行。

### サムネイル

- サイズ: 160×120 px 以内（アスペクト比保持）
- フォーマット: JPEG (q=75)
- 保存先: `{cache_dir}/kotoha-snap/thumbnails/{stem}.jpg`
- 生成タイミング: 翻訳完了後（失敗しても翻訳エントリの作成は継続）
- 削除タイミング: アプリ起動時に前回セッション分を全削除（翻訳ログはメモリのみで永続化されないため）

---

## 4. OSC 送信

### デフォルト設定

| 項目 | 値 |
|---|---|
| ホスト | 127.0.0.1 |
| ポート | 9000 |
| アドレス | /chatbox/input |
| チャンク間隔 | 4 秒 |

### OSC パケット引数

```
[String(text), Bool(true), Bool(false)]
```

- 第 2 引数 `immediate = true`: キーボードアニメーションをスキップ
- 第 3 引数 `notification = false`: VRChat 通知音なし

### 分割送信

VRChat チャットボックスの上限（120 文字 / Unicode コードポイント）に対し、コンテンツ目標を 90 文字として分割する。

**事前処理**

1. 先頭・末尾の空白を trim
2. 連続改行（`\n\n`）を `\n` に圧縮（VRChat の表示行数制限対策）

**プレフィックス形式**

| 条件 | プレフィックス |
|---|---|
| 1 チャンク + prefix ON | `[翻訳結果]\n` |
| 複数チャンク + prefix ON | `[翻訳結果{i}/{n}]\n` |
| prefix OFF | なし |

### タイミング

OSC 送信は `tokio::spawn` でバックグラウンド実行。`translation_done` イベントは OSC 送信完了を待たず即時 emit される（UI への全文表示を遅延させない）。

### 進捗通知

各チャンク送信後に `osc_chunk_progress` イベントを emit。フロントエンドの OSC ステータスバーに「OSC送信中 (N/M)」を表示。

### OSC キャンセル

チャンク間の `sleep` を `tokio::select!` で監視し、`cancel_osc` コマンド受信時に即座に中断。中断後に `osc_cancelled` イベントを emit。送信済みチャンクは取り消し不可。

---

## 5. 設定項目

保存先: `{config_dir}/kotoha-snap/config.toml`（TOML 形式）

### Config 全フィールド

| フィールド | 型 | デフォルト値 | 説明 |
|---|---|---|---|
| `provider` | String | `"groq"` | 使用するプロバイダー |
| `models.anthropic` | String | `""` | Anthropic モデル名（空 = プロバイダーデフォルト） |
| `models.openai` | String | `""` | OpenAI モデル名 |
| `models.groq` | String | `""` | Groq モデル名 |
| `models.google` | String | `""` | Google モデル名 |
| `models.custom` | String | `""` | カスタムプロバイダーモデル名 |
| `api_keys.anthropic` | String | `""` | Anthropic API キー |
| `api_keys.openai` | String | `""` | OpenAI API キー |
| `api_keys.groq` | String | `""` | Groq API キー |
| `api_keys.google` | String | `""` | Google API キー |
| `custom_provider.display_name` | String | `""` | カスタムプロバイダーの表示名 |
| `custom_provider.api_url` | String | `""` | チャット補完エンドポイント URL |
| `custom_provider.api_key` | String | `""` | カスタムプロバイダー API キー |
| `custom_provider.models_url` | String | `""` | モデル一覧取得 URL（任意） |
| `osc.host` | String | `"127.0.0.1"` | OSC 送信先ホスト |
| `osc.port` | u16 | `9000` | OSC 送信先ポート（1〜65535） |
| `osc.address` | String | `"/chatbox/input"` | OSC アドレス |
| `osc.chunk_interval_secs` | u64 | `4` | チャンク間の送信間隔（秒） |
| `osc_enabled` | bool | `true` | OSC 送信の有効フラグ |
| `osc_prefix_enabled` | bool | `true` | `[翻訳結果]` プレフィックスを付与するか |
| `sound_enabled` | bool | `true` | 翻訳完了時の通知音を鳴らすか |
| `is_enabled` | bool | `true` | 翻訳機能の有効フラグ |
| `font_size` | u8 | `13` | ログのフォントサイズ（10〜20 px） |
| `watch_dir` | PathBuf | `{picture_dir}/VRChat` | 監視するスクリーンショットフォルダ |
| `translation_prompt` | String | （日本語翻訳プロンプト） | 翻訳 API に渡すプロンプト |
| `file_ready_wait_ms` | u64 | `200` | ファイル検出後の待機時間（ms） |

### バリデーション

- OSC ポートが `0` の場合はエラー（`"OSC ポートには 1〜65535 の値を指定してください"`）
- `font_size` は自動的に 10〜20 に clamp

### save_config の挙動

`is_enabled` と `osc_enabled` はメインウィンドウのトグルで管理するため、設定画面からの保存時は現在の実行時値を引き継ぐ。

---

## 6. Tauri コマンド

すべてのコマンドはエラー時に `Err(String)` を返す。

### 設定

| コマンド | 引数 | 戻り値 | 説明 |
|---|---|---|---|
| `get_config` | — | `Config` | 現在の設定を取得 |
| `save_config` | `newConfig: Config` | `()` | 設定を保存・即時反映 |
| `reset_config` | — | `Config` | 設定をデフォルトにリセット |
| `set_enabled` | `enabled: bool` | `()` | 翻訳 ON/OFF を切り替え |
| `set_osc_enabled` | `enabled: bool` | `()` | OSC 送信 ON/OFF を切り替え |
| `set_font_size` | `size: u8` | `()` | フォントサイズを変更（自動 clamp 10〜20） |

### プロバイダー・モデル

| コマンド | 引数 | 戻り値 | 説明 |
|---|---|---|---|
| `get_default_models` | — | `HashMap<String, String>` | 各プロバイダーのデフォルトモデル名 |
| `fetch_models` | `provider, apiKey, modelsUrl?` | `Vec<String>` | 指定プロバイダーからモデル一覧を取得 |

### ウィンドウ操作

| コマンド | 引数 | 戻り値 | 説明 |
|---|---|---|---|
| `get_version` | — | `String` | アプリバージョン |
| `open_settings` | — | `()` | 設定ウィンドウを開く（既に開いている場合はフォーカス） |
| `open_about` | — | `()` | About ウィンドウを開く（既に開いている場合はフォーカス） |
| `open_file` | `path: String` | `()` | ファイルを OS のデフォルトアプリで開く |
| `open_url` | `url: String` | `()` | URL をブラウザで開く |

### 履歴

| コマンド | 引数 | 戻り値 | 説明 |
|---|---|---|---|
| `get_history` | — | `Vec<TranslationEntry>` | 翻訳ログを新着順で取得 |
| `clear_history` | — | `()` | 翻訳ログをクリア |

### キャンセル・テスト

| コマンド | 引数 | 戻り値 | 説明 |
|---|---|---|---|
| `cancel_translation` | — | `()` | 進行中の翻訳をキャンセル |
| `cancel_osc` | — | `()` | 進行中の OSC 送信をキャンセル |
| `test_osc` | — | `()` | OSC テストメッセージを送信 |

---

## 7. イベント（バックエンド → フロントエンド）

| イベント名 | ペイロード | タイミング |
|---|---|---|
| `translation_start` | `String`（画像パス） | 翻訳開始時 |
| `translation_done` | `TranslationEntry` | 翻訳完了時 |
| `translation_cancelled` | `()` | 翻訳キャンセル時 |
| `osc_chunk_progress` | `{ current: usize, total: usize }` | 各チャンク送信後 |
| `osc_cancelled` | `()` | OSC 送信キャンセル時 |
| `watcher_error` | `String`（エラー文） | ファイル監視エラー発生時 |
| `watcher_status` | `String` | 監視ステータス変化時 |
| `config_saved` | `()` | 設定保存完了時 |

---

## 8. 翻訳ログ

### TranslationEntry

```rust
pub struct TranslationEntry {
    pub timestamp: DateTime<Utc>,
    pub image_path: PathBuf,
    pub translated_text: String,
    pub provider: String,
    pub model: String,
    pub thumbnail_path: Option<PathBuf>,
}
```

### 制限・仕様

- 上限: 200 件（超過時は最古の 1 件を削除）
- **メモリのみ**（永続化なし。再起動でリセット）
- 取得順: 新着順（`get_history` の戻り値）

### 仮想スクロール（VirtualList）

- ビューポート内 ± バッファ 5 件のみ DOM に保持
- アイテム推定高さ: 110 px（測定前）
- アイテム間ギャップ: 8 px
- スクロール・リサイズ時に `requestAnimationFrame` で再レンダリング

---

## 9. UI 構成

### メインウィンドウ

- サイズ: 800×600 px、リサイズ可

**ヘッダー**

| 要素 | 説明 |
|---|---|
| タイトル | "KotohaSnap" |
| 翻訳トグル | ON/OFF。状態を config に即時保存 |
| チャット送信トグル | OSC 送信の ON/OFF。状態を config に即時保存 |
| 設定ボタン | 設定ウィンドウを開く |
| About ボタン（ⓘ） | About ウィンドウを開く |

**ツールバー**

| 要素 | 説明 |
|---|---|
| フォントサイズコントロール | − / 数値表示 / ＋。最小・最大時はボタンを無効化 |
| クリアボタン | 翻訳ログを全削除 |

**ログリスト**

各エントリに表示される情報：
- サムネイル画像（80×60 px。クリックで元画像を OS のデフォルトアプリで開く）
- プロバイダーバッジ
- モデル名バッジ（モデル名が存在する場合のみ）
- ファイル名（ホバーで完全パスをツールチップ表示）
- タイムスタンプ
- 翻訳テキスト（`pre-wrap`、改行保持）

**翻訳中カード**

- スピナー + "翻訳中..." テキスト
- 複数件同時処理時: "翻訳中... (N件)"（DOM 上は 1 枚のみ）
- キャンセルボタン付き（押下後は `disabled`）

**OSC ステータスバー**

- 表示: "OSC送信中 (N/M)"
- 「送信を中止」ボタン付き（最終チャンク送信後は `disabled`）
- 送信完了後 2 秒で自動非表示

**エラーバー**

- 監視エラーなど発生時に表示
- 閉じるボタンで手動非表示

---

### 設定ウィンドウ

- サイズ: 580×720 px、リサイズ可
- スクロール領域 + 固定フッター構成

**1. スクリーンショットフォルダ**
- パステキスト入力 + 「参照…」ボタン（OS ネイティブのディレクトリ選択ダイアログ）

**2. 通知**
- チェックボックス: 翻訳完了時に通知音を鳴らす

**3. OSC 設定**
- ホスト（テキスト）
- ポート（数値、min=1, max=65535）
- アドレス（テキスト）
- 分割送信の間隔（数値、秒、min=1, max=30）
- チェックボックス: 送信テキストの先頭に `[翻訳結果]` を付ける
- 「OSC テスト送信」ボタン

**4. プロバイダー設定**

プロバイダー選択（Google / OpenAI / Anthropic / Groq / カスタム）

- API キー入力（選択中のプロバイダーのみ表示）
- モデル入力フィールド（`<datalist>` によるオートコンプリート）+ 「モデルを取得」ボタン
- モデルヒント表示（デフォルトモデル名）
- プロバイダー切り替え時にモデル入力値をプロバイダーごとに保持

カスタムプロバイダー選択時は追加入力欄を表示：
- 表示名
- チャット補完 URL
- API キー（任意）
- モデル一覧取得 URL（任意）

**5. 翻訳プロンプト**
- テキストエリア（複数行、縦リサイズ可）

**6. すべての設定をリセット**
- ボタン（通常時はグレー、ホバー時に赤）
- クリック時に確認ダイアログ（`dialog:allow-confirm` 権限使用）
- 確認後、デフォルト設定に戻しフォームを再描画

**フッター**
- 保存ステータスメッセージ（OK / Error。4 秒で自動消去）
- 「設定を保存」ボタン

---

### About ウィンドウ

- サイズ: 400×300 px、リサイズ不可

| 表示項目 | 内容 |
|---|---|
| アイコン | 72×72 px |
| アプリ名 | KotohaSnap |
| バージョン | `get_version` コマンドで動的取得 |
| ライセンス | MIT |
| コピーライト | Copyright (c) 2026 s-tra |
| GitHub リンク | https://github.com/s-tra/KotohaSnap |
| X (Twitter) リンク | https://x.com/_s_tra |

---

## 10. その他

### フォントサイズ

- 範囲: 10〜20 px
- デフォルト: 13 px
- CSS 変数 `--font-size-base` で制御
- ログテキスト（`.log-entry-text`, `.pending-body`）は `1em` で親フォントサイズに追従
- その他 UI 要素は固定 px

### 通知音

Web Audio API による生成音（外部ファイルなし）：

- 波形: サイン波（440 Hz ではなく 420 Hz）
- ゲイン: 0.10 → 0.001（指数減衰）
- 継続時間: 150 ms

### CSS カラーテーマ（ダークテーマ固定）

| 変数 | 値 |
|---|---|
| `--bg` | #0d1117 |
| `--surface` | #161b22 |
| `--surface2` | #1c2128 |
| `--border` | #30363d |
| `--accent` | #58a6ff |
| `--accent-h` | #79b8ff |
| `--danger` | #f85149 |
| `--success` | #3fb950 |
| `--muted` | #8b949e |
| `--text` | #c9d1d9 |
| `--text-h` | #e6edf3 |
| `--radius` | 6px |

### イベントフロー（全体）

```
[ファイルシステム]
    ↓ notify が .png を検出
[watcher.rs / handle_event]
    ↓ is_enabled 確認 → 重複除外（5秒） → file_ready_wait_ms 待機
[process_screenshot]
    ↓ cancel_sender をセット
[translator/*.rs]
    ↓ 画像を Base64 エンコード → API リクエスト（タイムアウト 60 秒）
    ↓ （cancel_translation が呼ばれた場合は tokio::select! でキャンセル）
[watcher.rs]
    ↓ 翻訳テキスト取得 → サムネイル生成（失敗は無視）
[history / emit translation_done]
    ↓ TranslationEntry を VecDeque に追加（上限 200 件）
    ↓ translation_done を emit → UI にリアルタイム表示
[osc.rs / tokio::spawn]
    ↓ テキストを前処理（trim, 改行圧縮） → チャンク分割
    ↓ チャンクごとに UDP 送信 → osc_chunk_progress を emit
    ↓ チャンク間で sleep（cancel_osc 受信時は中断 → osc_cancelled を emit）
```

### パッケージ情報

| 項目 | 値 |
|---|---|
| パッケージ名 | kotoha-snap |
| バージョン | 0.9.9 |
| ライセンス | MIT |
| Tauri | 2.10.3 |
| Rust edition | 2021 |
| 最小 Rust バージョン | 1.77.2 |
