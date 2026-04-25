# Noye - サーバー死活監視システム

Cloudflare Workers 上で稼働する、軽量かつ堅牢なサーバー死活監視システム。
Unix哲学に基づく「最少機能での安全性と透明性」を重視し、Web UIは「Accessible by Default and by Design (ABDD)」の思想で構築。

## アーキテクチャ概要

**Gateway 層と Core 層に責務分離したモノレポ構成**。外部リクエストを受け付けるのは Gateway のみで、Core は Service Binding (+ Cron) からしか到達できない。

```
                ┌──────────────────┐
                │   OIDC IdP       │  (Google / Okta / Auth0 /
                │ (任意 OIDC 準拠) │   Entra ID / Keycloak 等)
                └────┬─────────────┘
                     │ Authorization Code + PKCE
                     │ ID Token (JWT, JWKS 検証)
                     ▼
┌────────────────────────────────────────────────────────────┐
│                       Cloudflare                           │
│  ┌────────────────────────────────────────────────────┐    │
│  │           Gateway Worker (外部公開)                │    │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐            │    │
│  │  │  OIDC    │ │ Session  │ │   UI     │            │    │
│  │  │  Client  │ │ (KV)     │ │  (SSR)   │            │    │
│  │  └──────────┘ └──────────┘ └──────────┘            │    │
│  └──────────────────────┬─────────────────────────────┘    │
│                         │ Service Binding                  │
│                         │ + X-Gateway-Token                │
│                         │ + X-Caller-*                     │
│                         ▼                                  │
│  ┌────────────────────────────────────────────────────┐    │
│  │           Core Worker (workers_dev=false)          │    │
│  │           外部到達不能 / Cron + Service Binding のみ   │    │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────────┐        │    │
│  │  │Internal  │ │ Monitor  │ │ Notification │        │    │
│  │  │REST API  │ │ Engine   │ │ Dispatch     │        │    │
│  │  └──────────┘ └──────────┘ └──────────────┘        │    │
│  └────┬───────────────┬────────────────┬───────────────┘    │
└───────┼───────────────┼────────────────┼────────────────────┘
        │               │                │              ▲
   ┌────▼────┐    ┌─────▼────┐     ┌─────▼────┐         │
   │   D1    │    │ R2       │     │ Cron     │  毎分  │
   │ (正本)  │    │(アーカイブ)│    │ Trigger  │─────────┘
   └─────────┘    └──────────┘     └──────────┘
```

### レイヤ責務

| 層 | 責務 | 外部到達 | バインディング |
|---|---|---|---|
| **Gateway** | OIDC 認証 / セッション管理 / UI SSR / Core 呼び出し | ✅ HTTPS | KV (セッション+OIDC state+JWKS)、Service Binding → Core |
| **Core** | DB アクセス / 監視エンジン / 通知ディスパッチ / Cron 実行 | ❌ 遮断 | D1、R2、Cron Triggers |

### セキュリティ多層防御

1. **ルート遮断**: Core は `workers_dev = false` かつ独自ルート未設定。公開 URL を持たず Cloudflare の Routing 層で既に到達不能。
2. **Service Binding のみ**: 外部からの到達経路は Gateway からの Service Binding と Cron Trigger の 2 系統のみ。
3. **共有秘密**: Gateway → Core の各呼び出しに `X-Gateway-Token` (Secret として両ワーカーに登録) を付与し、Core 側でヘッダ検証。
4. **データ隔離**: Gateway には D1 バインディングが存在しない。コード上 Gateway から直接 DB には触れられない。
5. **OIDC 認証**: Authorization Code + PKCE (S256) + state + nonce による Web 標準の認証フロー。
6. **ゲスト拒否**: OIDC で認証通過しても D1 `users` に事前登録されていないユーザーはログイン時に 403。

## 技術スタック

| 項目 | 技術 |
|------|------|
| コア言語 | Rust 2024 Edition (rustc 1.91+) |
| ビルドツール | Cargo workspace (resolver=3) |
| ランタイム | Cloudflare Workers (wasm32-unknown-unknown) |
| フロントエンド | SSR HTML (ABDD準拠) |
| データベース | Cloudflare D1 (SQLite互換) ← Core 専属 |
| キャッシュ/セッション | Cloudflare KV ← Gateway 専属 |
| オブジェクトストレージ | Cloudflare R2 ← Core 専属 |
| スケジューラー | Cloudflare Cron Triggers ← Core 専属 |
| ワーカー間通信 | Cloudflare Service Bindings |
| 認証 | 汎用 OIDC クライアント (Authorization Code + PKCE, OIDC Core 1.0) |
| 暗号処理 | Web Crypto API (`globalThis.crypto.subtle`) |
| ボット対策 | Cloudflare Turnstile (公開フォーム限定) |
| デプロイ | Wrangler v4 |

## プロジェクト構成

```
noye/                                  # Cargo workspace ルート
├── Cargo.toml                         # workspace 定義 (resolver=3)
├── README.md
├── sql/
│   └── 0001_initial.sql               # D1 スキーマ (Core が管理)
│
├── shared/                            # 共有型クレート
│   ├── Cargo.toml                     # noye-shared
│   └── src/lib.rs                     # Caller, Target, Incident 等の型 +
│                                      # header 名規約 (X-Caller-*, X-Gateway-Token)
│
└── workers/
    ├── gateway/                       # Gateway ワーカー (外部公開)
    │   ├── Cargo.toml                 # noye-gateway
    │   ├── wrangler.toml              # KV + Service Binding, OIDC 設定
    │   └── src/
    │       ├── lib.rs                 # fetch ハンドラ + ルート定義
    │       ├── auth.rs                # extract_caller (core_client 経由)
    │       ├── auth/
    │       │   ├── oidc.rs            # OIDC クライアント (Discovery + Auth Req + Token Exchange)
    │       │   ├── jwt.rs             # JWT パース + クレーム検証
    │       │   ├── jwks.rs            # JWKS 取得 + KV キャッシュ
    │       │   ├── crypto.rs          # Web Crypto ラッパー
    │       │   ├── session.rs         # KV ベースセッション
    │       │   ├── cookie.rs          # Cookie パース/ビルダー
    │       │   └── rbac.rs            # ロールベースアクセス制御
    │       ├── core_client.rs         # Core への Service Binding クライアント
    │       ├── ui.rs                  # UI モジュールルート
    │       └── ui/                    # SSR HTML レンダラ群
    │           ├── layout.rs          # 共通レイアウト (ABDD)
    │           ├── dashboard.rs
    │           ├── targets.rs
    │           ├── incidents.rs
    │           ├── maintenance.rs
    │           ├── audit.rs
    │           └── settings.rs
    │
    └── core/                          # Core ワーカー (内部ロジック)
        ├── Cargo.toml                 # noye-core
        ├── wrangler.toml              # workers_dev=false, D1 + R2 + Cron
        └── src/
            ├── lib.rs                 # fetch (内部 API) + scheduled (Cron)
            ├── api.rs                 # 認証ミドルウェア (Caller ヘッダ検証)
            ├── api/                   # 内部 REST ハンドラ
            │   ├── targets.rs
            │   ├── incidents.rs
            │   ├── maintenance.rs
            │   ├── audit.rs
            │   └── users.rs
            ├── db.rs + db/            # D1 CRUD (監視対象/状態/結果/インシデント/メンテ/監査/ユーザー/保持期間)
            ├── monitor.rs + monitor/  # 監視エンジン (engine/http/tcp/smtp/tls)
            └── notify.rs + notify/    # 通知ディスパッチ (channels)
```

## セットアップ

### 前提条件

- Rust ツールチェーン (rustc 1.85+ で Edition 2024 対応)
- `wasm32-unknown-unknown` ターゲット
- Node.js 18+ (Wrangler CLI 用)
- Wrangler v4 (`npm install -g wrangler`)

### 手順

```bash
# 1. wasm ターゲットの追加
rustup target add wasm32-unknown-unknown

# 2. worker-build のインストール
cargo install worker-build

# 3. ワークスペース全体のコンパイル検証 (任意)
cargo check --workspace

# ── Cloudflare リソースの作成 ──

# 4. D1 データベースの作成 (Core で使用)
wrangler d1 create noye_db
# 出力された database_id を workers/core/wrangler.toml に記入

# 5. KV ネームスペースの作成 (Gateway で使用)
cd workers/gateway && wrangler kv namespace create CACHE_KV && cd ../..
# 出力された id を workers/gateway/wrangler.toml に記入

# 6. R2 バケットの作成 (Core で使用)
wrangler r2 bucket create noye-logs

# 7. D1 マイグレーションの実行 (Core ディレクトリから)
cd workers/core && wrangler d1 migrations apply noye_db && cd ../..

# ── OIDC IdP の設定 ──

# 8. 使用する IdP で OAuth Client を作成
#    - Redirect URI: https://<gateway-worker-domain>/auth/callback
#    - Scopes: openid, email, profile
#    - Grant type: authorization_code (PKCE 対応)
#
#    workers/gateway/wrangler.toml の [vars] で:
#    - OIDC_ISSUER_URL
#    - OIDC_CLIENT_ID
#    - OIDC_REDIRECT_URI
#    を設定。

# ── 共有秘密の生成・登録 ──

# 9. Gateway <-> Core 共有秘密の生成
SHARED_TOKEN=$(openssl rand -hex 32)
echo "Generated: $SHARED_TOKEN"  # 両方に同じ値を登録するので控えておく

# 10. Gateway に secret を登録
cd workers/gateway
echo "$SHARED_TOKEN" | wrangler secret put GATEWAY_SHARED_TOKEN
wrangler secret put OIDC_CLIENT_SECRET
# プロンプトで OIDC クライアントシークレットを入力
cd ../..

# 11. Core にも同じ GATEWAY_SHARED_TOKEN を登録
cd workers/core
echo "$SHARED_TOKEN" | wrangler secret put GATEWAY_SHARED_TOKEN
cd ../..

# ── 初期管理者登録 ──

# 12. D1 に管理者ユーザーを登録 (IdP 側の email と一致させる)
cd workers/core
wrangler d1 execute noye_db --command \
  "INSERT INTO users (id, email, name, role) VALUES ('admin-001', 'admin@example.com', 'Admin', 'admin')"
cd ../..

# ── デプロイ (Core -> Gateway の順で) ──

# 13. Core を先にデプロイ (Gateway が Service Binding で参照するため)
cd workers/core && wrangler deploy && cd ../..

# 14. Gateway をデプロイ
cd workers/gateway && wrangler deploy && cd ../..
```

### ローカル開発

`wrangler dev` は Service Binding をサポートしているので、2 つのターミナルでそれぞれ起動:

```bash
# ターミナル 1: Core
cd workers/core && wrangler dev

# ターミナル 2: Gateway (Core への Service Binding を自動接続)
cd workers/gateway && wrangler dev
```

### OIDC プロバイダ別の `OIDC_ISSUER_URL` 設定例

| プロバイダ | Issuer URL |
|---|---|
| Google | `https://accounts.google.com` |
| Microsoft Entra ID | `https://login.microsoftonline.com/{tenant-id}/v2.0` |
| Okta | `https://{tenant}.okta.com/oauth2/default` |
| Auth0 | `https://{tenant}.auth0.com/` |
| Keycloak | `https://{host}/realms/{realm}` |
| AWS Cognito | `https://cognito-idp.{region}.amazonaws.com/{userPoolId}` |

いずれも `{issuer}/.well-known/openid-configuration` が応答することを確認してください (Discovery 自動取得で必要)。

## 機能要件の実装状況

### 認証と権限管理 (要件2-1)
- [x] 汎用 OIDC クライアント (Authorization Code + PKCE + state + nonce)
- [x] OIDC Discovery 自動取得 (KV キャッシュ付き)
- [x] ID Token 署名検証 (Web Crypto 経由で RS256/RS384/RS512/PS256/ES256/ES384 対応)
- [x] 任意 OIDC プロバイダ対応 (Google/Okta/Auth0/Entra ID/Keycloak/Cognito 等)
- [x] KV ベースのセッション管理 (HttpOnly + Secure + SameSite=Lax Cookie)
- [x] RBAC: 管理者 / 会員の2段階権限分離 (D1 で管理)
- [x] ゲストユーザー不可 (D1 未登録ユーザーはログイン時に 403)
- [x] end_session_endpoint 対応 (IdP サイドのログアウトも連動)
- [x] Gateway / Core 層分離 (Core は外部から到達不能)
- [ ] Turnstile 統合 (公開フォーム限定、Phase 3で実装予定)

### 監視対象の管理 (要件2-2)
- [x] HTTP / HTTPS / TCP / SMTP / TLS の種別対応
- [x] 接続先情報 (URL, ホスト, ポート, パス)
- [x] 判定条件 (期待ステータス, TLS閾値, タイムアウト, リトライ, 実行間隔)
- [x] 運用属性 (無効化フラグ, 所有者, タグ)

### 正常判定 (要件2-3)
- [x] HTTP/HTTPS: ステータスコード検証, レスポンス本文検証
- [x] TCP: ポート接続確認, バナー応答確認
- [x] SMTP: バナー受信, EHLO/HELO応答, STARTTLS確認
- [x] TLS証明書: 有効期限残日数チェック (crt.sh API経由)
- [x] タイムアウト・リトライ判定

### 監視ワーカー制御 (要件2-4)
- [x] Cron Triggers による毎分実行
- [x] 1本のスケジューラーで次回実行時刻ベース処理
- [x] 連続失敗/成功回数による状態遷移
- [x] メンテナンス期間中の通知抑止
- [x] 同一障害の重複通知防止

### データ要件 (要件3)
- [x] D1: 監視対象, ユーザー, 結果, 障害履歴
- [x] KV: セッション + OIDC state + JWKS + Discovery キャッシュ
- [x] R2: ログスナップショット, アーカイブ
- [x] データライフサイクル: 自動削除 + R2アーカイブ

### 運用要件 (要件4)
- [x] 監査ログ: 操作時刻, 操作者, 対象, 変更内容, 結果
- [x] メンテナンス期間: 作成者・更新者記録
- [x] バックアップ: R2への定期スナップショット

## Gateway 外部 API エンドポイント

| Method | Path | 説明 | 権限 |
|--------|------|------|------|
| GET | `/auth/login` | OIDC IdP へリダイレクト | 認証不要 |
| GET | `/auth/callback` | OIDC code 交換 + セッション発行 | 認証不要 |
| GET/POST | `/auth/logout` | セッション破棄 + IdP ログアウト | 認証不要 |
| GET | `/` | ダッシュボード | 全員 |
| GET | `/targets` | 監視対象一覧 | 全員 |
| GET | `/targets/:id` | 監視対象詳細 | 全員 |
| POST | `/api/targets` | 監視対象の作成 | 管理者 |
| PUT | `/api/targets/:id` | 監視対象の更新 | 管理者 |
| DELETE | `/api/targets/:id` | 監視対象の削除 | 管理者 |
| GET | `/api/targets/:id/results` | チェック結果一覧 (JSON) | 全員 |
| GET | `/incidents` | インシデント一覧 | 全員 |
| POST | `/api/incidents/:id/resolve` | インシデント手動復旧 | 管理者 |
| GET | `/maintenance` | メンテナンス期間一覧 | 全員 |
| POST | `/api/maintenance` | メンテナンス期間作成 | 管理者 |
| GET | `/audit` | 監査ログ | 管理者 |
| GET | `/settings` | 設定・ユーザー管理 | 管理者 |
| POST | `/api/settings/users` | ユーザー作成/更新 | 管理者 |
| GET | `/healthz` | ヘルスチェック | 認証不要 |

## Core 内部 API エンドポイント

Service Binding 経由でのみ到達可能。`X-Gateway-Token` + `X-Caller-*` ヘッダ必須。

| Method | Path | 用途 |
|--------|------|------|
| GET | `/users/lookup/:email` | Gateway が認証時にロール解決 |
| GET | `/targets` 他 | Gateway の各 UI ルートが裏で呼び出す |
| ... | ... | (Gateway API と 1:1 対応) |

## アクセシビリティ (ABDD)

本システムのWeb UIは以下のアクセシビリティ要件を満たします:

- **セマンティックHTML**: `<header>`, `<nav>`, `<main>`, `<footer>`, `<table>`, `<dl>` を適切に使用
- **WAI-ARIA**: `role`, `aria-label`, `aria-current`, `aria-disabled` を適切に付与
- **キーボードナビゲーション**: スキップリンク, `:focus-visible` によるフォーカスリング
- **カラーコントラスト**: ダーク/ライトモード対応, WCAG AA準拠配色
- **モーション軽減**: `prefers-reduced-motion` メディアクエリ対応
- **HTMLファースト**: CSS/JS が制限された環境でもステータスが把握可能

## 留意事項

1. **Workers TCP制限**: `connect()` API はGA済みだが、Rust (`wasm32-unknown-unknown`) からの呼び出しは JS バインディング経由。`tokio` 依存クレートは非互換。
2. **Cron CPU制限**: Workers の CPU 時間上限 (Free: 10ms, Paid: 30ms) に注意。監視対象が多い場合は Cloudflare Queues へのタスク分割を検討。
3. **TLS証明書検証**: Workers 内での直接的な X.509 アクセスは不可。crt.sh API または KV キャッシュ併用で対応。
4. **Service Binding のレイテンシ**: Gateway → Core 呼び出しは同一データセンター内での dispatch のため数ms〜十数ms。ただし認証済みルートは 1 リクエストで最低 2 回の Service Binding (認証用 lookup + 本処理) が走る。
