# OpenWorkspace Engine — 技術棧與部署（Tech Stack）

> 本文件是專案的「憲法」第二條：**明確定義採用的技術決策**，以及**部署與更新流程**。
> 任何技術選型的變更都應先在此記錄理由，再動工。

---

## 技術決策總覽

| 層 | 技術 | 版本 | 決策理由 |
|---|---|---|---|
| **控制平面 API** | Rust + Axum | stable / axum 0.8 | 記憶體安全、零成本抽象、<35MB RAM、高併發非阻塞 I/O |
| **前端** | SvelteKit 2 + Svelte 5（靜態 SPA） | 2.x / 5.x | Runes 響應式、adapter-static 全靜態、主機零 SSR CPU 成本 |
| **CSS / UI** | Tailwind CSS v4 + Skeleton | v4 | 工具類快速開發、既有元件庫 |
| **反向代理** | Traefik | v3.7.4 | File Provider + inotify 熱載入路由；**不掛 Docker socket** |
| **靜態資產** | Nginx | latest | HTTP 快取、SPA 靜態托管 |
| **容器調度** | bollard（Rust Docker API） | 0.18 | 非同步容器/網路生命週期控制 |
| **容器 Runtime** | Docker OCI（runc） | ≥ 24 | 標準、快速建立實例 |
| **容器 Runtime（強化）** | gVisor（runsc） | latest | 使用者空間核心攔截 syscall；逐範本可選 |
| **資料庫** | PostgreSQL | 18-alpine | 持久化唯一狀態源；sqlx 編譯期檢查 + 自動 migration |
| **記憶體快取** | DashMap | — | O(1) VNC 權杖查詢，省去每次 WebSocket handshake 的 DB 往返 |
| **網路 QoS** | Linux `tc`/HTB + `nsenter` | — | 核心層每實例上/下行頻寬上限 |
| **套件管理** | pnpm + Turborepo | pnpm 9 / turbo 2 | monorepo workspace 依賴 + 任務調度與快取 |
| **實例影像** | KasmVNC / Jupyter Lab / ttyd（自建 + `_dini` 變體） | — | Docker Hub 亦可拉取預建影像 |

---

## 逐層決策與理由（ADR 式紀錄）

### 1. 控制平面用 Rust（不是 Go / Node / Python）

- **安全**：所有權模型在編譯期消除記憶體漏洞（use-after-free、資料競爭）——這是控制整個主機 Docker socket 與網路的程序，不能靠 GC 或型別妥協。
- **效能**：零成本抽象、非阻塞 I/O（tokio）；極低的 RAM 與 CPU 佔用，與「復活老硬體」的使命一致。
- **Axum 0.8**：型別安全路由、萃取器、與 tokio 生態整合良好。
- **bollard**：Rust 生態最成熟的 Docker API 客戶端，非同步控制容器/網路。

### 2. 前端用 SvelteKit 靜態站（不是 SSR / 不是 React）

- **adapter-static + `ssr = false`**：建置產物是純靜態檔案，由 Nginx 直接提供——**主機零 SSR CPU 成本**，這對共享主機至關重要。
- **Svelte 5 runes**：細粒度響應式，bundle 小、載入快。
- **SPA 多實例共存**：catch-all route（`[...path]`）依 `window.location.pathname` 偵測 `/kasmvnc/{token}/` 等路徑，單一 build 服務整個平台。
- **Tailwind v4 + Skeleton**：統一設計語言，快速迭代管理介面。

### 3. 反向代理用 Traefik File Provider（不用 Docker Provider / 不用 nginx 動態）

- **熱載入路由**：API 將每實例的路由 YAML 寫入被監看的目錄，Traefik 透過 inotify 偵測後**立即生效，零重啟、零停機**——新增實例秒級可用。
- **不掛 Docker socket**：Traefik 容器不掛載 `/var/run/docker.sock`，縮小攻擊面；路由完全由 API（唯一有權限控制容器的角色）決定。
- **每權杖 Basic 注入**：JS `WebSocket` 無法自訂 header，因此由 Traefik 中間件在伺服器端注入 `Authorization: Basic`——瀏覽器永遠看不到實例密鑰。
- **規模特性**：每個實例 = 1 個 ~250 bytes 的 YAML；路由匹配成本與實例總數無關，Traefik 無狀態，狀態全在 PostgreSQL。

### 4. 容器 Runtime 雙軌：runC（預設）+ gVisor（強化）

- **runC**：標準 OCI runtime，效能最佳，預設選擇。
- **runsc（gVisor）**：使用者空間核心攔截 syscall，大幅降低容器逃逸風險；範本層可選。
- **NVProxy GPU 透傳**：gVisor `--nvproxy` 代理 NVIDIA ioctl，支援 Turing/Ampere/Ada/Hopper（T4、A100/A10G、L4、H100）。
- **已驗證**：Turing（GTX 1650）與 Ampere（RTX 3060）可用；Maxwell（GTX 970）失敗。

### 5. 資料庫用 PostgreSQL + 記憶體快取分層

- **PostgreSQL 是唯一狀態源**：使用者、群組、範本、實例、持久化路徑全部在此；sqlx 編譯期檢查 SQL 正確性，migration 在 API 啟動時自動執行。
- **DashMap 快取**：`access_token → {status}` 的無鎖並發 HashMap，讓每次 WebSocket handshake 的權杖驗證是 O(1) 記憶體查詢（miss 才回 DB）。
- **為什麼不用 Redis/Valkey**：單一 API 程序即可提供完整快取一致性（快取與狀態在同程序），省下一個有狀態服務。若未來多程序/多主機，再評估分散式快取（見 docs/caching-strategy.md）。

### 6. 權杖驗證用 JWT（身份-only）+ 每次請求重算權限

- **JWT 只帶身份**（`sub`、`exp`），**不帶任何權限資訊**。
- 每次請求自 DB 重新解析使用者的 effective context（群組旗標、範本白名單、實例額度）——權限變更**下一個請求即生效**，過期的權杖不可能殘留權限。
- 權杖以 **HttpOnly cookie** 存放（`ow_token`），避免 XSS 讀取；`SameSite=Lax`、`Secure`（HTTPS 環境）。

### 7. 實例網路採「每實例 `/30` + 主機埠發布」

- 每個實例獨占一個 `/30` 網段（2 個可用 IP），自成 L2 網段——**東西向攻擊結構性不可能**。
- 服務埠發布到主機 bridge gateway（`<host_gateway_ip>:<host_port>`），Traefik 經 `host.docker.internal` 到達，永不使用容器 IP。
- 主機埠與 `/30` 都是有限池，以 **flock lockfile**（`host_port.rs` / `instance_net.rs`）在跨程序間仲裁，並以 TCP probe + 有限重試吸收並發快照競態。見 docs/lock-registry.md。

### 8. 持久化用「Local Bind-mounted Named Volume」

- 使用者整個 home 目錄對應到主機固定路徑 `{root}/{template_name}/{user_id}`（API 解析、絕對路徑、防 `..` 注入）。
- 首次（空）掛載自動填入影像內建 home 設定，環境開箱即用。
- 刪除實例**保留資料**，只有「重設」會清除；重啟會重新宣告遺失的 volume。

### 9. 套件管理：pnpm + Turborepo monorepo

- 兩個活躍 app：`apps/web`（SvelteKit 前端）、`apps/api`（Rust API）；`apps/vnc-ui` 為**已棄用的舊 noVNC UI**（僅保留供參考，不參與建置）。
- pnpm workspace + Turborepo 任務調度與快取，統一入口 `pnpm run dev` / `pnpm run build` / `pnpm test`。

### 10. 依賴版本固定策略

- **pako@1 固定** — v3 破壞了 noVNC 內部使用的 import 路徑。
- **pnpm-lock.yaml** 提交入 repo，確保可重現安裝。
- 升級任何依賴須通過完整測試套件（見下）並在此文件註記理由。

---

## 品質門檻（Quality Gates）

開發流程的硬性規定（不可繞過）：

1. **Rust 零警告政策** — `cd apps/api && bash scripts/check.sh` 兩次檢查（default + `docker` feature）**必須完全無輸出**；禁止任何 `#[allow(…)]` 抑制屬性。
2. **測試套件** — `apps/api/scripts/run_tests.sh`（cargo nextest，需 Docker）：158 個單元測試 + 324 個整合測試；`cd apps/web && pnpm test`：23 個檔案 / 287 個測試。
3. **型別檢查** — `cd apps/web && pnpm check`（svelte-check）。
4. **無 lint script** 存在於 vnc-ui；根目錄 `pnpm lint` 只作用於 web（若配置）。

---

## 開發環境流程（Dev）

### 一次啟動

```bash
pnpm install          # 安裝 workspace 依賴
pnpm run dev          # 完整開發棧
```

`pnpm run dev` 依序執行：
1. `kill-dev.sh` — 釋放 3000/5173 埠的殘留 dev server。
2. `pnpm run init` — 建立 `ow-network`（自動選空閒 `172.16-31.0.0/16` 網段）+ 註冊 gVisor `runsc` runtime 到 `/etc/docker/daemon.json`。
3. `pnpm run docker:dev:up` — 啟動 dev Traefik + Postgres（`docker/openworkspace_dev/`）。
4. `pnpm run network:allow` — 授權 `nsenter`/`tc` 的 capabilities，讓主機上執行的 API 能做頻寬控管。
5. 以 `concurrently` 執行 Rust API（`:3000`）與 Vite dev server（`:5173`）。

停止：`pnpm run dev:stop`；完整清除（含 volumes）：`pnpm run dev:remove`。

### 開發環境特性
- **純 HTTP**（`http://localhost`）——瀏覽器視 localhost 為 secure context，不需憑證。
- dev Traefik 代理到**主機上執行**的 dev server（`host.docker.internal:5173` / `:3000`）。
- 主機執行的 API 將路由 YAML 寫入 `docker/openworkspace_dev/traefik/dynamic`（`TRAEFIK_DYNAMIC_DIR` 未設定時的編譯期預設）。
- 實例仍由主機執行的 API 透過 Docker socket 建立；dev Postgres 埠 `55432:5432`。

---

## 生產部署流程（Prod）

### 初始部署（首次）

```bash
pnpm run init                                   # ow-network + runsc runtime
pnpm run build:template-images                  # 建置三種實例影像（含 _dini 變體）
docker compose -f docker/openworkspace/docker-compose.yml up -d --build
```

> 平台本身（`ow-web:latest` / `ow-api:latest`）由 compose 直接從原始碼建置，不需另外 push。
> 實例影像可選擇從 Docker Hub 拉取（`tsukisama9292/ow-*-ubuntu*`）。

### 生產架構

| Service | Image | 埠 | 說明 |
|---|---|---|---|
| `traefik` | traefik:v3.7.4 | `80`、`127.0.0.1:8080` | **僅 file provider**（無 Docker socket）；dynamic 目錄唯讀掛載 |
| `api` | ow-api（自建） | — | **root 執行**、`pid: host`、`cap_add: [SYS_ADMIN, NET_ADMIN, SYS_PTRACE]`、`apparmor=unconfined`、rw Docker socket |
| `web` | ow-web（自建） | — | SvelteKit 靜態 build 由 nginx 提供 |
| `postgresql` | postgres:18-alpine | — | Volume `server-pgdata` |

- 生產 Traefik 以 compose 服務名路由（`ow-api:3000`、`ow-web:80`），而非 `host.docker.internal`。
- API 容器將路由寫入 `./traefik/dynamic`（其 `TRAEFIK_DYNAMIC_DIR`，rw 掛入 API、ro 掛入 traefik 的 `/etc/traefik/dynamic`）。
- 每實例路由檔（`*-ws.yml`）已 gitignore。

### HTTPS（TLS 終止在 Traefik 之前）

本棧以純 HTTP 提供一切（前端、`/api`、VNC WebSocket）。**不要在 Traefik 內啟用 TLS**——把 TLS 終止代理放在前面：

- **Cloudflare** — DNS 記錄開啟 **Proxied**；TLS 在 Cloudflare 邊緣終止並轉發到 `:80`，無需管理憑證。
- **Let's Encrypt** — 前方放一個會自動申請憑證的反向代理（Traefik ACME / Caddy / nginx）轉發到 `:80`。

> 自簽/自建 CA 憑證不適用：Chromium 永不忽略 `fetch()` 子資源的憑證錯誤，`/api` 請求會以 `ERR_CERT_AUTHORITY_INVALID` 失敗。

---

## 更新流程（Update Flow）

### 程式碼更新

```bash
git pull                                    # 取得最新程式碼
docker compose -f docker/openworkspace/docker-compose.yml up -d --build
```

- `--build` 重建 `ow-web` / `ow-api` 影像。
- **DB migration 自動執行**：API 啟動時 sqlx 自動套用未執行的 migration（當前到 `000021`）——不需手動跑 migration。
- **路由零停機**：Traefik 熱載入動態目錄，新路由秒級生效；更新過程中不需重啟 traefik。

### 實例影像更新

```bash
pnpm run build:template-images              # 重建本機影像，或
docker pull tsukisama9292/ow-*-ubuntu*       # 拉取 Hub 上的最新影像
```

### 環境變數變更

改 `.env`（compose 檔旁）後 `docker compose up -d`。注意：改動 `JWT_SECRET` 會使所有已登入使用者需要重新登入；改動 `POSTGRES_*` 需對應既有 `server-pgdata` volume 的既有認證。

### 版本控制慣例

- 所有設定、compose、docs 皆入 repo；`*-ws.yml`（動態路由）與 build 產物不入 repo。
- 升級任何依賴（尤其 pako、Traefik、Postgres 主版本）必須跑完整測試套件後再部屬。

---

## API 環境變數總表

| 變數 | 預設 | 說明 |
|---|---|---|
| `DATABASE_URL` | *(必要)* | Postgres 連線字串 |
| `JWT_SECRET` | *(必要，生產必須更改)* | `ow_token` JWT 簽章密鑰 |
| `ADMIN_PASSWORD` | `admin` | 種子 admin 的啟動密碼 |
| `SERVER_HOST` / `SERVER_PORT` | `0.0.0.0` / `3000` | API 綁定位址 |
| `DB_MAX_CONNECTIONS` | `5` | sqlx 連線池大小 |
| `OW_CONTAINER_RUNTIME` | `docker` | 新實例預設 runtime（`runsc`、`runc`…） |
| `OW_HOST_GATEWAY_IP` | `172.17.0.1` | 實例發布埠綁定的主機 IP |
| `OW_HOST_PORT_START` / `OW_HOST_PORT_END` | `10000` / `20000` | 主機埠池 |
| `OW_INSTANCE_NET_BASE` | `10.200.0.0/16` | 每實例 `/30` 網段的 CIDR 基底（需網段對齊） |
| `OW_INSTANCE_DNS` | `8.8.8.8,1.1.1.1` | 注入 `OW_DNS` 的 DNS 解析器（image entrypoint 改寫 `/etc/resolv.conf`） |
| `TRAEFIK_DYNAMIC_DIR` | dev 預設 | 每實例路由 YAML 寫入目錄 |

---

## 已棄用／參考項目

| 項目 | 狀態 | 說明 |
|---|---|---|
| `apps/vnc-ui` | 已棄用 | 舊 noVNC UI；僅供參考，不參與建置（21 個測試保留） |
| `references_repo/KasmVNC` | 參考 | 上游 KasmVNC 原始碼（`kasmweb/`） |
| `references_repo/gvisor` | 參考 | 上游 gVisor（shallow clone，僅 `g3doc/`） |
| `references_repo/docker-docs` | 參考 | 上游 Docker 文件（僅 `content/`） |
| `users.role` / `is_system_admin` / `user_templates` | 已移除 | migration `000018`–`000020` 移除，改為群組制 RBAC |

---

## 已知限制（Known Limitations）

- **單一 API 程序** — DashMap 快取與資源分配為程序內一致性；多程序由 flock 解決埠/網段競爭，但快取仍是 per-process。
- **無 CI** — 目前無 `.github` CI；品質門檻靠本機腳本（check.sh / run_tests.sh）維持。
- **GPU 僅 NVIDIA + 特定架構** — NVProxy 支援 Turing/Ampere/Ada/Hopper。
- **tc/HTB 需要 root capabilities** — 需 `network:allow` 授權 `nsenter`/`tc`；失敗為 fail-open（log 但不殺 session）。

---

## 相關文件

- [mission.md](mission.md) — 使命與核心功能（憲法第一條）
- [roadmap.md](roadmap.md) — 階段規劃（憲法第三條）
- [docs/development.md](docs/development.md) — 完整開發指南、除錯、環境變數
- [docs/architecture.md](docs/architecture.md) — 系統架構與 DB schema
