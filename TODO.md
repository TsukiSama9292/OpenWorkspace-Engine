# OpenWorkspace Engine — 開發計畫

## 架構總覽

```
Browser
  │
  ▼
Traefik (port 80/443)
  ├── /              → apps/web   (SvelteKit 管理介面)
  ├── /api/          → apps/api   (Rust REST API)
  ├── /kasm{n}/      → apps/vnc-ui (SvelteKit VNC viewer)
  └── /kasm{n}/websockify → KasmVNC container (WebSocket)
```

**為什麼用 Traefik 而不是 nginx**: 實例增減時 nginx 需要重寫 `default.conf` + `nginx -s reload`，這段時間會有短暫的代理中斷。Traefik 透過 Docker labels 自動發現服務，實例建立/銷毀時零停機。

**網路**: 所有容器都在 `openworkspace-engine` Docker network 上。

**服務啟動鏈**:
```
scripts/docker-network.sh (✅ 已完成)
  → apps/api build & start → health check ✓
    → apps/web build & start → health check ✓
      → Traefik start
apps/vnc-ui build ──────────────→ Traefik start
```

---

## Phase 1: 基礎設施

### 1.1 Docker 網路 — `scripts/docker-network.sh` ✅
- [x] 建立 `scripts/` 目錄
- [x] 寫 `scripts/docker-network.sh`：檢查 `openworkspace-engine` network 是否存在，不存在則建立
- [ ] 確認 `docker-compose.yml` 的 networks 使用 `openworkspace-engine: external: true`

### 1.2 根目錄 `package.json` 調整
- [ ] 加入 `dev` script：執行 `bash scripts/dev.sh`（完整啟動鏈）
- [ ] 加入 `dev:api` script：`cd apps/api && cargo run`
- [ ] 加入 `dev:web` script：`pnpm --filter web dev`
- [ ] 加入 `dev:vnc` script：`pnpm --filter vnc-ui dev`
- [ ] 加入 `build:api` script：`cd apps/api && cargo build --release`
- [ ] 加入 `build:web` script：`pnpm --filter web build`
- [ ] 加入 `build:vnc` script：`pnpm --filter vnc-ui build`
- [ ] 加入 `docker:network` script：`bash scripts/docker-network.sh`
- [ ] 加入 `docker:up` script：`docker compose up -d`
- [ ] 加入 `docker:down` script：`docker compose down`
- [ ] 確認 `turbo.json` 不再 try to build apps/api（Rust 不走 turbo）

### 1.3 啟動編排腳本 — `scripts/dev.sh`
- [ ] 建立 `scripts/dev.sh`
- [ ] Step 1: 執行 `docker-network.sh`
- [ ] Step 2: `cd apps/api && cargo build --release && cargo run --release` → 背景啟動
- [ ] Step 3: 健康檢查 — `until curl -sf http://localhost:3000/health; do sleep 1; done`
- [ ] Step 4: `pnpm --filter web build && pnpm --filter web preview --port 5174` → 背景啟動
- [ ] Step 5: 健康檢查 — `until curl -sf http://localhost:5174/; do sleep 1; done`
- [ ] Step 6: `pnpm --filter vnc-ui build`
- [ ] Step 7: `docker compose up -d`
- [ ] trap EXIT：清理背景進程 + `docker compose down`

---

## Phase 2: apps/api (Rust)

### 2.1 專案初始化
- [ ] `cargo init apps/api --name openworkspace-api`
- [ ] `Cargo.toml` 加入 dependencies：
  - `axum` — HTTP framework
  - `tokio` — async runtime
  - `serde` / `serde_json` — serialization
  - `sqlx` (SQLite) — database
  - `jsonwebtoken` — JWT auth
  - `bcrypt` — password hashing
  - `bollard` — Docker Engine API
  - `tower-http` — CORS, trace
  - `uuid` — unique IDs
  - `chrono` — timestamps
  - `tracing` / `tracing-subscriber` — logging
  - `dotenvy` — env config
- [ ] 建立 `.env.example`：`DATABASE_URL`, `JWT_SECRET`, `API_PORT=3000`

### 2.2 資料庫 Schema (SQLite)
- [ ] `migrations/001_init.sql`:
  ```sql
  CREATE TABLE users (
    id TEXT PRIMARY KEY,
    username TEXT UNIQUE NOT NULL,
    password_hash TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT 'user',  -- 'admin' | 'user'
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
  );

  CREATE TABLE instances (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    container_id TEXT,
    status TEXT NOT NULL DEFAULT 'stopped',  -- 'running' | 'stopped' | 'error'
    owner_id TEXT NOT NULL REFERENCES users(id),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
  );
  ```
- [ ] SQLx migrations 設定

### 2.3 認證模組 — `src/auth.rs`
- [ ] `POST /api/auth/register` — 註冊（僅 admin 可建立新 user）
- [ ] `POST /api/auth/login` — 登入，回傳 JWT
- [ ] `GET /api/auth/me` — 取得目前使用者資訊
- [ ] JWT middleware — 驗證 `Authorization: Bearer <token>`
- [ ] Role extraction — `RequireAdmin` layer

### 2.4 使用者管理 — `src/users.rs`
- [ ] `GET /api/users` — 列出所有使用者（admin only）
- [ ] `GET /api/users/:id` — 取得單一使用者
- [ ] `PUT /api/users/:id` — 更新使用者（admin only）
- [ ] `DELETE /api/users/:id` — 刪除使用者（admin only）
- [ ] `POST /api/users/:id/change-password` — 修改密碼

### 2.5 實例管理 — `src/instances.rs`
- [ ] `GET /api/instances` — 列出所有實例（admin: 全部, user: 自己的）
- [ ] `POST /api/instances` — 建立新實例（建立 KasmVNC container）
- [ ] `POST /api/instances/:id/start` — 啟動實例
- [ ] `POST /api/instances/:id/stop` — 暫停實例
- [ ] `DELETE /api/instances/:id` — 關閉並刪除實例
- [ ] `GET /api/instances/:id` — 取得實例詳情（狀態、端口、URL）

### 2.6 Docker 控制 — `src/docker.rs`
- [ ] 初始化 bollard Docker client
- [ ] `create_instance()` — 複製 kasm 模板建立新 container
- [ ] `start_instance()` — 啟動 container
- [ ] `stop_instance()` — 停止 container
- [ ] `remove_instance()` — 刪除 container
- [ ] `list_instances()` — 列出 managed containers
- [ ] Container naming convention: `ow-{instance_id}`
- [ ] 自動加入 `openworkspace-engine` network

### 2.7 Traefik 路由管理 — `src/traefik.rs`
- [ ] 建立 container 時加入 Docker labels 讓 Traefik 自動發現路由：
  ```yaml
  labels:
    traefik.enable: "true"
    traefik.http.routers.{id}.rule: "PathPrefix(`/{instance_name}/`)"
    traefik.http.routers.{id}.service: "{instance_name}"
    traefik.http.services.{id}.loadbalancer.server.port: "6901"
    traefik.http.routers.{id}-ws.rule: "Path(`/{instance_name}/websockify`)"
    traefik.http.routers.{id}-ws.service: "{instance_name}"
    traefik.http.routers.{id}-ws.entrypoints: "websecure"
    traefik.http.services.{id}-ws.loadbalancer.server.port: "6901"
  ```
- [ ] 刪除 container 時移除 labels（Traefik 自動停止代理）
- [ ] Container 建立/刪除後 Traefik 自動感知，零停機

### 2.8 Health Check
- [ ] `GET /health` — 回傳 `{"status": "ok"}`
- [ ] 包含 database connectivity check

### 2.9 API 路由組態
```rust
// src/main.rs
let app = Router::new()
    .route("/health", get(health))
    .nest("/api/auth", auth_routes())
    .nest("/api/users", user_routes())      // admin only for most
    .nest("/api/instances", instance_routes())
    .layer(auth_middleware)
    .layer(cors_layer);
```

---

## Phase 3: apps/web (SvelteKit 管理介面)

### 3.1 專案初始化
- [ ] `pnpm create svelte apps/web` — 選 Skeleton project
- [ ] `adapter-static` (跟 vnc-ui 一樣)
- [ ] `ssr = false`, `trailingSlash = 'always'`
- [ ] `package.json` name: `web`

### 3.2 API Client — `src/lib/api.ts`
- [ ] Base URL: 從 env 或 `window.location.origin` 取得
- [ ] `apiFetch(method, path, body?)` — 自動帶 JWT token
- [ ] 401 自動 redirect 到 `/login`
- [ ] 錯誤處理 toast

### 3.3 Auth Store — `src/lib/stores/auth.js`
- [ ] `user` state: `{ id, username, role }`
- [ ] `token` state: 存 localStorage
- [ ] `login(username, password)` → POST /api/auth/login
- [ ] `logout()` → 清除 token
- [ ] `isAuthenticated` / `isAdmin` derived

### 3.4 路由
| 路由 | 功能 | 權限 |
|------|------|------|
| `/login` | 登入頁面 | 公開 |
| `/` | Dashboard — 實例列表 | 已登入 |
| `/instances/new` | 建立新實例 | 已登入 |
| `/instances/:id` | 實例詳情（start/stop/delete） | 已登入 |
| `/admin/users` | 使用者管理 | admin |

### 3.5 頁面元件
- [ ] `src/routes/login/+page.svelte` — 登入表單
- [ ] `src/routes/+page.svelte` — Dashboard（實例卡片列表）
- [ ] `src/routes/+layout.svelte` — Auth guard + 導航列
- [ ] `src/routes/instances/new/+page.svelte` — 建立實例表單
- [ ] `src/routes/instances/[id]/+page.svelte` — 實例詳情（start/stop/delete）
- [ ] `src/routes/admin/users/+page.svelte` — 使用者管理（admin only）

### 3.6 共用元件
- [ ] `src/lib/components/Navbar.svelte` — 頂部導航列
- [ ] `src/lib/components/InstanceCard.svelte` — 實例卡片（狀態、操作按鈕）
- [ ] `src/lib/components/StatusBadge.svelte` — 狀態徽章
- [ ] `src/lib/components/Modal.svelte` — 確認對話框
- [ ] `src/lib/components/Toast.svelte` — 通知提示

### 3.7 樣式
- [ ] 沿用 vnc-ui 的 CSS variables（`--bg-primary`, `--accent` 等）
- [ ] 深色/淺色主題（複用 theme store 邏輯）
- [ ] 響應式 layout

---

## Phase 4: apps/vnc-ui 整合

### 4.1 Auth Guard
- [ ] 建立 `src/lib/stores/auth.js` — 與 web 共用 API client 邏輯
- [ ] 檢查 localStorage 中的 JWT token
- [ ] 無 token → redirect 到 `/login`（或 web app 的 login page）
- [ ] `+layout.js` 加入 auth check
- [ ] token 過期自動 redirect

### 4.2 Login 頁面（可選）
- [ ] 決策：vnc-ui 自帶 login 頁面，或是 redirect 到 web app
- [ ] 若自帶：`src/routes/login/+page.svelte` — 簡易登入表單
- [ ] 若 redirect：`+layout.js` 中直接 `window.location = '/web/login'`

### 4.3 API 串接
- [ ] 實例資訊從 API 取得（而非 URL path 硬編）
- [ ] 可選：URL path 仍可用，但需驗證 token

---

## Phase 5: Docker Compose & Traefik

### 5.1 `docker-compose.yml` 重構
- [ ] network 使用 `openworkspace-engine: external: true`（✅ 網路已建立）
- [ ] 移除 nginx service
- [ ] 加入 Traefik service:
  ```yaml
  traefik:
    image: traefik:v3.0
    container_name: ow-traefik
    <<: *default-opts
    command:
      - "--providers.docker=true"
      - "--providers.docker.exposedbydefault=false"
      - "--providers.docker.network=openworkspace-engine"
      - "--entrypoints.web.address=:80"
      - "--entrypoints.websecure.address=:443"
      - "--api.insecure=true"
    ports:
      - "80:80"
      - "443:443"
      - "8080:8080"  # Traefik dashboard
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock:ro
    labels:
      traefik.enable: "true"
  ```
- [ ] 加入 `api` service:
  ```yaml
  api:
    build: ./apps/api
    container_name: ow-api
    <<: *default-opts
    ports:
      - "3000:3000"
    environment:
      - DATABASE_URL=sqlite:///data/openworkspace.db
      - JWT_SECRET=${JWT_SECRET}
    volumes:
      - api-data:/data
    labels:
      traefik.enable: "true"
      traefik.http.routers.api.rule: "PathPrefix(`/api/`)"
      traefik.http.routers.api.entrypoints: "web"
      traefik.http.services.api.loadbalancer.server.port: "3000"
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:3000/health"]
      interval: 5s
      timeout: 3s
      retries: 10
  ```
- [ ] 加入 `web` service:
  ```yaml
  web:
    build: ./apps/web
    container_name: ow-web
    <<: *default-opts
    volumes:
      - ./apps/web/build:/usr/share/nginx/html/web:ro
    labels:
      traefik.enable: "true"
      traefik.http.routers.web.rule: "PathPrefix(`/`)"
      traefik.http.routers.web.entrypoints: "web"
      traefik.http.services.web.loadbalancer.server.port: "80"
  ```
- [ ] 加入 `vnc` service (build vnc-ui):
  ```yaml
  vnc:
    build: ./apps/vnc-ui
    container_name: ow-vnc
    <<: *default-opts
    volumes:
      - ./apps/vnc-ui/build:/usr/share/nginx/html/vnc:ro
    labels:
      traefik.enable: "true"
      traefik.http.routers.vnc.rule: "PathPrefix(`/kasm`)"
      traefik.http.routers.vnc.entrypoints: "web"
      traefik.http.services.vnc.loadbalancer.server.port: "80"
  ```
- [ ] KasmVNC containers 由 API 動態建立，自動帶 Traefik labels
- [ ] volumes: `api-data`
- [ ] traefik depends_on api (condition: service_healthy)

### 5.2 `apps/api/Dockerfile`
- [ ] Multi-stage build: builder (rust:slim) → runtime (debian:slim)
- [ ] 安裝 curl（health check 用）
- [ ] COPY binary + migrations

### 5.3 `apps/web/Dockerfile`
- [ ] Multi-stage: node builder → nginx runtime（或直接 serve static）
- [ ] 輸出靜態檔案

### 5.4 `apps/vnc-ui/Dockerfile`
- [ ] Multi-stage: node builder → nginx runtime（或直接 serve static）
- [ ] 輸出靜態檔案

### 5.5 Health Check 配置
- [ ] api service: `curl -f http://localhost:3000/health`
- [ ] web service: depends_on api (condition: service_healthy)
- [ ] traefik service: depends_on api (condition: service_healthy)
- [ ] docker-compose.yml 的 `restart: unless-stopped` 保留

### 5.6 移除舊 nginx 設定
- [ ] 移除 `nginx/conf.d/default.conf`（Traefik 不需要手動 config）
- [ ] 移除 `nginx/nginx.conf`
- [ ] 保留 `nginx/` 目錄或完全刪除

---

## Phase 6: 整合與清理

### 6.1 AGENTS.md 更新
- [ ] 更新專案架構說明（三個 apps + Traefik）
- [ ] 更新 key commands（加入 api、web 的指令）
- [ ] 更新 build/deploy flow

### 6.2 `.env.example`
- [ ] `DATABASE_URL=sqlite:///data/openworkspace.db`
- [ ] `JWT_SECRET=change-me-in-production`
- [ ] `API_PORT=3000`

### 6.3 Git 清理
- [ ] `.gitignore` 加入 `apps/api/target/`
- [ ] `.gitignore` 加入 `*.db`
- [ ] 確認 `pnpm-workspace.yaml` 包含 `apps/*`

---

## 技術選擇摘要

| 組件 | 技術 | 原因 |
|------|------|------|
| API 框架 | axum | Rust 最成熟 HTTP framework |
| 資料庫 | SQLite (sqlx) | 輕量，無需額外 container |
| Docker 控制 | bollard | Rust Docker Engine API client |
| 認證 | JWT + bcrypt | 標準無狀態認證 |
| 反向代理 | Traefik v3 | Docker labels 自動路由，實例增減零停機 |
| 管理介面 | SvelteKit | 沿用現有技術棧 |
| 建構 | Turborepo + pnpm | 現有 monorepo 工具 |

## 檔案結構（完成後）

```
OpenWorkspace-Engine/
├── apps/
│   ├── api/                    # Rust API server
│   │   ├── src/
│   │   │   ├── main.rs
│   │   │   ├── auth.rs
│   │   │   ├── users.rs
│   │   │   ├── instances.rs
│   │   │   ├── docker.rs
│   │   │   └── traefik.rs
│   │   ├── migrations/
│   │   ├── Cargo.toml
│   │   └── Dockerfile
│   ├── web/                    # SvelteKit admin UI
│   │   ├── src/
│   │   │   ├── routes/
│   │   │   │   ├── login/
│   │   │   │   ├── instances/
│   │   │   │   └── admin/
│   │   │   └── lib/
│   │   │       ├── api.ts
│   │   │       ├── stores/
│   │   │       └── components/
│   │   ├── package.json
│   │   ├── svelte.config.js
│   │   └── Dockerfile
│   └── vnc-ui/                 # SvelteKit VNC viewer（已有）
│       └── ...
├── scripts/
│   ├── docker-network.sh       # ✅ 已完成
│   └── dev.sh
├── docker-compose.yml
├── package.json
├── turbo.json
├── pnpm-workspace.yaml
└── TODO.md
```
