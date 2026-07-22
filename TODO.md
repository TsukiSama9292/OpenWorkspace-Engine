# OpenWorkspace Engine — 開發計畫

## 目前架構

```
瀏覽器
  │
  ▼
Traefik (port 80)
  ├── /              → apps/web (SvelteKit 管理介面)
  ├── /api/*         → apps/api (Rust REST API)
  └── /kasm{n}/*     → ForwardAuth → api 驗證 → apps/web (vnc-ui build)
                                                    └→ /websockify → KasmVNC
```

**已完成**:
- Traefik v3.7.4 反向代理 (Docker labels 自動發現)
- nginx 靜態檔案服務
- PostgreSQL 18
- Docker 網路 `openworkspace-engin`

**Compose 位置**: `docker/openworkspace/docker-compose.yml`

---

## Phase 1: 基礎設施 ✅

### 1.1 Docker 網路 ✅
- [x] `scripts/docker-network.sh`

### 1.2 Docker Compose ✅
- [x] Traefik — Docker labels 路由，零停機
- [x] PostgreSQL 18
- [x] nginx — serve 靜態檔案
- [x] `docker/openworkspace/nginx.conf` — SPA routing

---

## Phase 2: 認證架構

### 2.1 設計原則
- **JWT in HTTP-only Cookie** — 防 XSS，自動帶入
- **Traefik ForwardAuth** — 集中式驗證，KasmVNC 不需改動
- **未認證** → 重定向到 `/login`

### 2.2 Cookie 設定
```
Name:     ow_token
HttpOnly: true
Secure:   false (dev) / true (prod)
SameSite: Lax
Path:     /
MaxAge:   24h
```

### 2.3 認證流程
```
1. 用戶訪問 /kasm1/
2. Traefik 攔截 → ForwardAuth 到 api:3000/api/auth/validate
3. API 從 cookie 讀取 JWT 驗證
   ├─ 有效 → 返回 200 → Traefik 轉發
   └─ 無效 → 返回 401 → 重定向到 /login
4. 用戶登入 → 設定 cookie → 導回首頁
```

### 2.4 Traefik ForwardAuth Labels (動態實例)
```yaml
labels:
  traefik.http.routers.kasm.rule: "PathPrefix(`/kasm1/`)"
  traefik.http.routers.kasm.middlewares: "kasm-auth@docker"
  traefik.http.middlewares.kasm-auth.forwardauth.address: "http://ow-api:3000/api/auth/validate"
  traefik.http.middlewares.kasm-auth.forwardauth.authResponseHeaders: "X-User-Id,X-User-Role"
```

---

## Phase 3: apps/api (Rust)

### 3.1 專案初始化
- [ ] `cargo init apps/api --name openworkspace-api`
- [ ] Dependencies: axum, tokio, serde, sqlx (PostgreSQL), jsonwebtoken, bcrypt, bollard, tower-http

### 3.2 資料庫 Schema
- [ ] users: id, username, password_hash, role (admin/user), timestamps
- [ ] instances: id, name, container_id, status, owner_id, timestamps

### 3.3 認證端點
- [ ] POST /api/auth/login — 驗證密碼，設定 HTTP-only cookie
- [ ] GET /api/auth/validate — ForwardAuth 用 (200/401)
- [ ] POST /api/auth/logout — 清除 cookie
- [ ] POST /api/auth/register — 建立使用者 (admin only)

### 3.4 使用者管理
- [ ] CRUD /api/users (admin only)

### 3.5 實例管理
- [ ] GET /api/instances — 列出實例
- [ ] POST /api/instances — 建立 KasmVNC container
- [ ] POST /api/instances/:id/start|stop
- [ ] DELETE /api/instances/:id

### 3.6 Docker 控制 (bollard)
- [ ] 建立/啟動/停止/刪除 container
- [ ] Container naming: `ow-{instance_id}`
- [ ] 自動加入 `openworkspace-engin` network
- [ ] 動態加入 Traefik labels (websockify route)

### 3.7 Dockerfile
- [ ] Multi-stage: rust:slim → debian:slim

---

## Phase 4: apps/web (SvelteKit 管理介面)

> 從 apps/vnc-ui 複製修改，apps/vnc-ui 保留為參考用

### 4.1 初始化
- [ ] 複製 apps/vnc-ui → apps/web
- [ ] 更新 package.json name: "web"
- [ ] 更新 svelte.config.js (base path, adapter)
- [ ] 調整路由結構

### 4.2 API Client
- [ ] `src/lib/api.ts` — fetch wrapper，自動帶 cookie
- [ ] 401 response → redirect to /login

### 4.3 Auth Store
- [ ] `src/lib/stores/auth.js` — user state, login/logout
- [ ] +layout.js auth guard

### 4.4 頁面
| 路由 | 功能 | 權限 |
|------|------|------|
| `/login` | 登入頁面 | 公開 |
| `/` | Dashboard — 實例列表 | 已登入 |
| `/instances/new` | 建立實例 | 已登入 |
| `/instances/:id` | 實例詳情 + iframe VNC | 已登入 |
| `/admin/users` | 使用者管理 | admin |

### 4.5 VNC iframe 整合
- [ ] `/instances/:id` 內嵌 `<iframe src="/kasm{n}/">`
- [ ] `Content-Security-Policy: frame-ancestors 'self'` 防劫持
- [ ] iframe 共用 HTTP-only cookie，自動認證

---

## 技術選擇

| 組件 | 技術 |
|------|------|
| 反向代理 | Traefik v3.7.4 |
| API | Rust + axum |
| 資料庫 | PostgreSQL 18 |
| Docker 控制 | bollard |
| 認證 | JWT + bcrypt + HTTP-only cookie |
| ForwardAuth | Traefik middleware |
| 管理介面 | SvelteKit (apps/web) |
| VNC viewer | noVNC (apps/vnc-ui, reference only) |

## 檔案結構

```
OpenWorkspace-Engine/
├── apps/
│   ├── api/                    # Rust API (待建)
│   ├── web/                    # 管理介面 (從 vnc-ui 複製修改)
│   └── vnc-ui/                 # 參考用 (不 active)
├── docker/openworkspace/
│   ├── docker-compose.yml
│   └── nginx.conf
├── scripts/
│   └── docker-network.sh
└── TODO.md
```
