# OpenWorkspace Engine — 路線圖（Roadmap）

> 本文件是專案的「憲法」第三條：**依實作順序分解各階段與預計開發的功能**。
> 原則：先完成、再完美；每一階段結束時都有可運作的成果，且**單主機永遠是可運作的最低門檻**。

---

## 規劃原則

1. **垂直切片優先** — 每一階段交付「可從瀏覽器使用」的完整功能，不做半成品。
2. **單主機優先** — 多主機/叢集是加分項，但任何時候單台主機都能完整運作。
3. **安全不落後** — 每個階段都要維持 Zero-Trust 隔離（獨立網段、獨立憑證、群組 RBAC）。
4. **狀態標記**：✅ 已完成（含 commit 對照）／🔵 進行中／📋 規劃中／💡 構想（未排程）。

---

## 已完成階段（回顧）

### ✅ 階段 1：核心基礎設施

單主機最基本的「可以跑起來」：一個瀏覽器管理介面 + 一個可啟動的容器 + 一個路由。

| 交付 | 內容 |
|---|---|
| 控制平面 API | Rust + Axum，bollard 控制 Docker，sqlx + PostgreSQL |
| 動態路由 | API 寫入 Traefik 路由 YAML，inotify 熱載入，新實例秒級可用 |
| JWT 驗證 | `ow_token` cookie、登入/登出、`/auth/me` |
| 容器生命週期 | 建立/啟動/停止/暫停/刪除 + 健康檢查 + 狀態機 |
| SvelteKit 靜態 SPA | adapter-static、單頁儀表板、VNC 檢視器（noVNC 整合） |
| gVisor 準備 | runsc runtime 註冊腳本 + 逐範本 runtime 選擇 |

### ✅ 階段 2：多介面與資源治理

從「一個桌面」擴展為「三種介面」，並開始把資源當作需要治理的對象。

| 交付 | 內容 |
|---|---|
| Jupyter Lab 與 ttyd 終端 | 三種 remote_type、各自的路由/認證模式（Basic 注入 vs URL token） |
| Auto-Sleep（執行時限） | `max_run_seconds` + `timeout_action`，前端倒數警示與重導向 |
| Keep Time（閒置回收） | 聚焦心跳 + `keep_time_seconds`，超過即回收 |
| 網路頻寬控管 | `tc`/HTB 上/下行 Mbps 上限，veth 配對定位 + fail-open |
| 持久化使用者資料 | 整個 home 目錄持久化、三種啟動模式、重啟回填 |
| 單頁式儀表板 | 所有管理集中一頁，無頁面切換延遲 |
| Docker-in-instance（`_dini`） | 實例內可再操作 Docker（`--privileged` + tmpfs 設定檔） |

### ✅ 階段 3：網路隔離與並發安全

把「多租戶」真正變安全：實例彼此、實例與控制平面徹底隔離；並發啟動不會互相踩踏。

| 交付 | 內容 |
|---|---|
| 每實例 `/30` 網段 | 東西向攻擊結構性不可能；控制平面與實例網路分離 |
| 主機埠池 | `OW_HOST_PORT_START–END` 分配，衝突重試 |
| flock 跨程序仲裁 | 埠與 `/30` 以 lockfile 分配，任意多個 API 程序並發啟動不衝突（`host_port.rs` / `instance_net.rs`） |
| runsc DNS 修正 | 使用者定義 bridge 在 runsc 下無法用 Docker embedded resolver → `OW_DNS` 注入 + entrypoint 改寫 |

### ✅ 階段 4：群組制 RBAC 與管理面

將權限從「角色」升級為「群組」，並補齊管理介面。

| 交付 | 內容 |
|---|---|
| 平坦化群組 RBAC | 五旗標（`can_create_template` / `can_manage_users` / `can_manage_group_instances` / `can_manage_docker` / `can_manage_registry`）+ 範本白名單 + 實例額度；每次請求自 DB 重算 effective context |
| 系統群組 | Admin / Manager / User 三種系統群組（kind 固定、旗標釘住） |
| 範本可見性 | `public` / `private` / `hidden` + 群組白名單（admin 無白名單豁免） |
| 實例額度 | 群組 `max_instances` + 個人 `direct_max_instances` → effective ceiling，精確 `FOR UPDATE` 計數 |
| 主機實例上限 | `admin_settings` 全域上限（`host_instance_limit`，admin 也算入） |
| 管理介面 | Groups / Users / Volumes / Settings tab、密碼變更、登出 |
| 範本白名單 UI | 群組 ↔ 範本關聯編輯 |

### ✅ 工程品質門檻（Quality Gates）

非產品階段，屬開發者體驗／工程紀律里程碑（`.scratch/quality-gates/`）。

| 交付 | 內容 |
|---|---|
| Rust Clippy 硬閘 | `check.sh` 與 `apps/api` `check` 皆跑 `cargo clippy --all-targets --all-features -- -D warnings`；既有警告全數修掉（無 `#[allow]`） |
| 禁止 unsafe | `#![forbid(unsafe_code)]` 於 crate root；`set_var` 測試改為注入式來源 |
| 軟性分析報表 | `analysis:rust`（too_many_lines 100 / cognitive_complexity 25）、`analysis:unsafe`（cargo geiger）、`analysis:bloat`（cargo llvm-lines）——exit 0 |
| Web lint | `apps/web` ESLint flat config；`lint` / `check` / `analysis:web` |
| 獨立 E2E | `e2e/` Playwright 套件（smoke + full）、root `test:e2e` / `test:e2e:full` |
| Turbo 修復 | `turbo.json` 宣告 `test` task，`pnpm test` 恢復可用 |
| 無 sudo dev | `pnpm run dev:nosudo`（跳過 gVisor 註冊 + `network:allow`） |

---

## 進行中／規劃中階段

### 🔵 階段 5：可觀測性與營運（Next）

> 現況：已有 Session 桌面的基本狀態，但「營運者看不到發生什麼事」。

| 項目 | 說明 | 優先級 |
|---|---|---|
| 審計日誌（Audit Logging） | 記錄登入、啟動/停止/刪除實例、權限變更、範本修改等管理事件；寫入 DB 並可在 UI 查詢 | 高 |
| 資源監控面板 | 主機 CPU/RAM/磁碟、每實例資源使用、實例歷史（透過 cgroups / `/proc` / Docker stats） | 高 |
| 營運 Logs 頁面 | 補齊現有的 Logs 佔位 tab（目前無內容） | 中 |
| 每群組/使用者資源配額 | 除實例數量外，加上 CPU / 記憶體 / GPU 的群組級配額 | 中 |

### 📋 階段 6：可靠性與備份

| 項目 | 說明 | 優先級 |
|---|---|---|
| 持久化資料備份/快照 | 定期備份 `server-pgdata` 與使用者 home 目錄；簡單還原流程 | 高 |
| 空閒資料夾清理 | 移除已不存在於 DB 的 orphaned 持久化資料夾（現有 UI 的「Thorough Cleanup」） | 中 |
| 優雅關機/開機 | 重開機後實例狀態恢復、路由重建、volume 重新宣告 | 中 |
| 健康自我檢查 | API/Traefik/DB 的健康端點聚合，供外部監控（uptime 檢查） | 低 |

### 📋 階段 7：身份與安全強化

| 項目 | 說明 | 優先級 |
|---|---|---|
| 登入失敗鎖定 | 連續失敗後暫時鎖定帳號（防暴力破解） | 高 |
| 2FA（TOTP） | 使用者啟用一次性密碼 | 中 |
| SSO（OIDC / LDAP） | 對接既有企業身份提供者（非基本承諾，路線圖可選） | 低 |
| 密碼原則 | 強度要求、定期輪換提醒 | 低 |

### 📋 階段 8：多主機與叢集

> 與使命一致：這是「未來可選」，不是承諾。多主機不應破壞單主機體驗。

| 項目 | 說明 | 優先級 |
|---|---|---|
| 多主機排程 | 多台主機由單一控制平面管理，實例可指定/遷移主機 | 高（在叢集內） |
| Tailscale mesh | 主機間以 Tailscale 建立安全覆蓋網路，統一路由 | 中 |
| 分散式狀態 | 每實例路由/資源分配進入獨立 Table（解決單程序快取與記憶體狀態的上限） | 中 |
| 主機故障轉移 | 主機失效時的實例恢復策略 | 低 |

### 💡 構想（未排程，僅紀錄）

- **GPU 配額** — 按群組分配 GPU 數量與型別。
- **範本市集/分享** — 匯出/匯入範本設定，跨主機分享。
- **實例快照/回復** — 持久化資料的多時間點快照。
- **WebRTC 低延遲** — 取代/補強目前 WebSocket 傳輸的編碼方案。
- **行動端適配** — 儀表板與檢視器的行動瀏覽器最佳化。
- **自動化 E2E（Playwright + 活容器）** — 讓現有 E2E 設定進入 CI 而非手動執行。

---

## 已完成功能總覽（Checklist）

> 對照 [mission.md](mission.md) 的核心功能，逐項確認狀態。

### 介面
- ✅ KasmVNC 桌面（HTML5 Canvas + WebSocket）
- ✅ Jupyter Lab
- ✅ ttyd 終端

### 安全與隔離
- ✅ 每實例獨立存取權杖（127 字元）
- ✅ 每實例 `/30` 網段（東西向隔離）
- ✅ gVisor（runsc）逐範本沙箱 + NVProxy GPU 透傳
- ✅ JWT cookie + ForwardAuth
- ✅ 伺服器端 Basic 注入（瀏覽器不見密鑰）
- ✅ 群組制 RBAC（旗標 + 白名單 + ceiling，逐請求重算）

### 資源治理
- ✅ Auto-Sleep（執行時限 + timeout_action）
- ✅ Keep Time（閒置回收 + 聚焦心跳）
- ✅ 頻寬控管（tc/HTB）
- ✅ 實例額度（群組 + 個人 + 主機全域）

### 持久化
- ✅ 整個 home 目錄持久化
- ✅ 三種啟動模式（use / no / reset）
- ✅ 刪除保留資料、重啟回填

### 管理
- ✅ 單頁儀表板
- ✅ 群組/使用者/額度管理
- ✅ 範本可見性 + 白名單
- ✅ 密碼變更與登出

---

## 每一階段的「定義完成」（Definition of Done）

任何階段宣告完成前，必須全部滿足：

1. **功能**：階段內所有交付項可從瀏覽器實際操作，非僅 API 存在。
2. **測試**：`cd apps/api && bash scripts/check.sh`（零警告）+ `bash scripts/run_tests.sh`（nextest，Docker）；`cd apps/web && pnpm check && pnpm test`（287 測試）全綠。
3. **文件**：對應的 docs（architecture / api-reference / rbac / frontend / mission / tech-stack）同步更新，無過時敘述。
4. **部署**：`pnpm run docker:up` 在乾淨主機上可成功部屬並建立第一個實例。
5. **安全**：新功能未破壞 Zero-Trust 隔離（網段、憑證、RBAC 三層仍生效）。

---

## 相關文件

- [mission.md](mission.md) — 使命與核心功能（憲法第一條）
- [tech-stack.md](tech-stack.md) — 技術決策、部署與更新（憲法第二條）
- [docs/architecture.md](docs/architecture.md) — 系統架構與 DB schema
