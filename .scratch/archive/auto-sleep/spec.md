Status: completed

# Auto-Sleep：Template 使用時長與超時操作

## 一、問題陳述

這個平台要在一台 Linux 主機上共享運算資源給多位開發者，每個 Instance 都是從 Template 啟動的常駐 VNC 容器。目前沒有機制防止 Instance 無限期佔用資源：使用者在工作結束後若直接關掉瀏覽器分頁，容器仍會持續耗用 CPU、記憶體與 GPU，直到被手動停止或刪除。

在多人共享的環境下，這會導致資源被少數閒置的 Instance 綁死，其他使用者啟動新 Instance 時無資源可用，管理者也必須人工巡視並清理。

## 二、解決方案

在 **Template** 上新增兩個欄位，讓 Template 擁有者設定這個 Template 的 Instance 能連續執行多久、時間到之後執行什麼動作：

- **使用時長**（`max_run_seconds`）：只計入 `running` 時間，暫停與停止時間皆不計入。從 Instance 進入 `running` 的那一刻起計時，時間到達上限即觸發超時操作；每個 Instance 每次進入 `running` 都會重新計時（重新獲得完整時長）。
- **超時操作**（`timeout_action`）：`remove`（預設）／`stop`／`pause`，由一個背景工作程序（Auto-Sleep Worker）在掃描時執行。

Instance 進入 `running` 時由 API 記錄 `started_at` 時間戳；背景工作程序定期掃描所有 `running` 且已記錄 `started_at` 的 Instance，若 `now - started_at >= max_run_seconds` 則依照 Template 設定的動作回收資源。

| 角色 | 行為 |
|---|---|
| Template | 新增 `max_run_seconds`（NULL = 停用）與 `timeout_action`（預設 `remove`） |
| API（進入 running 時） | 記錄 `started_at = NOW()`；離開 running（pause/stop）時清除 |
| Auto-Sleep Worker | 每 3 秒掃描 `running` 且 `started_at` 有值的 Instance，超過上限即執行 Template 設定的動作 |
| 前端（Template 表單） | 新增「使用時長」設定（可停用；啟用時輸入秒數）與「超時操作」下拉 |

## 三、使用者故事

1. 身為 Template 擁有者，我希望能在 Template 上設定使用時長，以便限制從此 Template 啟動的 Instance 佔用資源的時間
2. 身為 Template 擁有者，我希望能在 Template 上設定超時操作（remove/stop/pause），以便決定時間到之後資源如何回收
3. 身為 Template 擁有者，我希望超時操作的預設值是 `remove`，以便在沒有特別選擇時閒置資源會被完全釋放
4. 身為 Template 擁有者，我希望未設定使用時長的 Template 不會被自動回收，以便保留原本「無使用上限」的行為
5. 身為 Template 擁有者，我希望修改 Template 的使用時長後立刻對所有從此 Template 啟動的 Instance 生效，以便不需要重啟 Instance 也能調整政策
6. 身為 Instance 使用者，我希望 Instance 每次進入 `running` 都重新計時，以便一次完整的工作階段從啟動開始享有完整的時長
7. 身為 Instance 使用者，我希望暫停再恢復後重新計時，以便暫停期間不消耗使用時長
8. 身為 Instance 使用者，我希望 Instance 在達到時長上限時被自動停止、暫停或移除，以便我不需要記得手動關閉
9. 身為 Instance 使用者，我希望被 `remove` 的 Instance 從列表消失、容器被完全刪除，以便確認資源已被釋放
10. 身為 Instance 使用者，我希望被 `stop` 的 Instance 狀態變成 `stopped` 並可再次啟動，以便之後還能繼續使用（重新啟動後重新計時）
11. 身為 Instance 使用者，我希望被 `pause` 的 Instance 狀態變成 `paused` 並可恢復，以便工作內容保持在記憶體中、恢復成本最低
12. 身為管理者，我希望超時回收不需使用者或管理者介入，以便資源持續被自動釋放
13. 身為管理者，我希望即使有使用者正連線到 VNC 工作階段，時長上限仍然執行，以便「硬性上限」能真正防止資源被長期佔用
14. 身為 Instance 使用者，我希望功能上線前就已存在的 `running` Instance 不會被立刻回收，以便舊工作階段不會無預警消失
15. 身為 Template 擁有者，我希望透過 API 設定時長時有最小下限（60 秒）驗證，以便避免誤設過短時長造成 Instance 立刻被刪除
16. 身為 Template 擁有者，我希望在建立與編輯 Template 的表單中都能設定使用時長與超時操作，以便完整管理
17. 身為 Template 擁有者，我希望編輯既有 Template 時表單能顯示已儲存的使用時長與超時操作，以便確認與修改現有設定
18. 身為 Instance 使用者，我希望 Instance 的時長計時與 Template 目前的設定一致，以便政策調整立即反映在執行中的 Instance 上
19. 身為 Instance 使用者，我希望只有 `running` 時間會被計入使用時長（暫停與停止期間不消耗），以便暫停/停止期間不會縮短我下次工作階段的可用時間

## 四、實作決策

### 4.1 計時語義

使用時長只計入 `running` 時間：**暫停與停止時間皆不計入**。採「每次進入 `running` 重新計時」的語義（非累計、非首次執行起算的牆鐘時間、不累加多次執行區間）。Instance 每次轉入 `running`（啟動完成經健康檢查提升、或暫停後恢復）時，API 將 `started_at` 設為目前時間；轉出 `running`（pause／stop）時清除為 `NULL`。因此：

- 暫停期間不計入使用時長（pause→unpause 重新計時，恢復後重新獲得完整時長）
- 停止期間不計入使用時長（停止後重新啟動重新計時，獲得完整的新時長）
- 多次執行區間不累加；任何一次進入 `running` 都是全新計時
- `remove` 直接刪除 Instance，無重新計時問題

時間計算為 `now - started_at >= max_run_seconds` 即觸發，不另做累計或扣除暫停區間。

### 4.2 資料庫變更

新增一支 migration：

- `workspace_templates` 新增欄位：
  - `max_run_seconds BIGINT NULL` — 時長上限（秒）。`NULL` = 停用（既有 Template 遷移後皆為 `NULL`，保持原有行為）
  - `timeout_action VARCHAR(20) NOT NULL DEFAULT 'remove'` — 超時動作，合法值 `remove`／`stop`／`pause`（可加 DB CHECK 約束）
- `workspace_instances` 新增欄位：
  - `started_at TIMESTAMPTZ NULL` — 目前工作階段進入 `running` 的時間。`NULL` = 目前不在計時

DB Entity（`workspace_template`、`workspace_instance`）與公開 struct（`WorkspaceTemplate`、`WorkspaceInstance`）同步新增對應欄位。

### 4.3 Repository 變更

- `WorkspaceTemplateRepository::create` / `update` 增加參數：`max_run_seconds: Option<i64>`、`timeout_action: &str`
- `WorkspaceInstanceRepository` 新增 `update_started_at(id, Option<DateTimeUtc>)`，用於設定／清除 `started_at`
- 新增查詢：列出 `status = 'running'` 且 `started_at IS NOT NULL` 的 Instance，供 Auto-Sleep Worker 掃描

### 4.4 Auto-Sleep Worker（`health_worker.rs`）

在既有 3 秒背景迴圈中，除了現有的 `check_instances`（`starting` 探測）之外，新增 `check_auto_sleep` 掃描。掃描僅處理**同時滿足**下列條件的 Instance：

1. `status == 'running'`
2. `started_at IS NOT NULL`（`NULL` 的舊 Instance 一律跳過，直到被停止後重新啟動）
3. 其 Template 的 `max_run_seconds IS NOT NULL`

判斷 `now - started_at >= max_run_seconds` 成立即執行 Template 的 `timeout_action`。Template 設定每個 tick 讀取現值——中途修改時長/動作立即生效，不採 snapshot。動作與既有手動路徑的清理序列一致：

- **remove**（預設）：`delete_route` → `vnc_cache.remove` → stop 容器 → remove 容器 → 刪除 Instance row（Template 的持久化 host 目錄檔案保留在磁碟，不刪除）
- **stop**：stop 容器 → `delete_route` → `vnc_cache.remove` → 更新狀態為 `stopped`
- **pause**：pause 容器 → 更新狀態為 `paused`（保留 Traefik route，與手動 pause 一致）

狀態更新在 Docker 動作執行後進行，即使 Docker 呼叫失敗仍更新（與現有 `stop_instance`「updating DB anyway」一致），避免 Instance 卡在掃描迴圈。`check_auto_sleep` 設計為 `pub` 函式、時鐘由參數注入（見 5.2），與現有 `check_instances` 同一個測試接縫。

### 4.5 `started_at` 的設定與清除（`instances.rs`）

- 健康檢查提升：`check_single_instance` 探測成功轉為 `running` 時，一併設定 `started_at = NOW()`
- `unpause_instance`：恢復為 `running` 時設定 `started_at = NOW()`
- `pause_instance`：pause 成功後清除 `started_at = NULL`
- `stop_instance`：停止後清除 `started_at = NULL`
- `delete_instance` 與 auto-sleep remove：刪除 row，無需清理

### 4.6 Template API

`POST /templates`、`PUT /templates/{id}` 的 request 增加：

- `max_run_seconds`：可空整數（秒）。驗證：若提供則 ≥ 60（下限 60 秒）
- `timeout_action`：字串，限 `remove`／`stop`／`pause`，未提供時預設 `remove`

Template 的 response 一律包含這兩個欄位，供前端表單回填。

### 4.7 前端（Template 表單）

- Template 表單新增「使用時長」設定：勾選啟用後以數字輸入秒數（下限 60 秒）；未勾選 = 停用，送出 `null`
- 新增「超時操作」下拉：`remove`（預設）／`stop`／`pause`
- 建立與編輯共用同一表單狀態；編輯時從 Template response 回填
- Instance 列表、Sessions 頁面、詳細頁面均不改動

## 五、測試決策

### 5.1 測試原則

只測試外部行為：給定 DB 中的 Instance／Template 狀態與 Docker mock 的呼叫預期，Auto-Sleep 掃描產生正確的狀態轉換與 Docker 呼叫。不測試掃描的 tick interval、reqwest 內部行為或 Docker 實作。

### 5.2 最高接縫：`check_auto_sleep`（單一主要接縫）

`check_auto_sleep` 為 `pub` 函式，簽名與 `check_instances` 同形：注入 `instance_repo`、`template_repo`、`&dyn DockerService`（mockall mock）、`vnc_cache`，並以參數注入時鐘 `now: DateTime<Utc>`，使「是否超過上限」的判定可以決定性地測試，不需等待真實時間。測試沿用現有 Postgres 測試 harness（`tests/common`）與 mockall DockerService（既有 `instances_mock_test.rs` / `templates_test.rs` 模式）。

案例：

- `running` + `started_at` 早於 `now - max_run_seconds` + 動作 `remove` → Docker 呼叫 stop + remove，route 刪除、`vnc_cache` 清空、Instance row 被刪除
- 同上但動作 `stop` → Docker 呼叫 stop、狀態變為 `stopped`、`started_at` 為 `NULL`
- 同上但動作 `pause` → Docker 呼叫 pause、狀態變為 `paused`、`started_at` 為 `NULL`
- `running` 但未達上限 → 無任何 Docker 呼叫、狀態不變
- `running` 但 `started_at IS NULL`（舊 Instance）→ 跳過
- `running` 但 Template `max_run_seconds IS NULL`（停用）→ 跳過
- Template 時長中途調高（snapshot 語意）→ 不觸發；調低/清除 → 依現值觸發或不觸發
- 已觸發後（狀態已離開 `running`）→ 下次掃描不重複觸發
- `now` 未注入時由呼叫端傳入 `Utc::now()`，行為一致

### 5.3 Route 層級測試（次級接縫）

沿用 `instances_mock_test.rs` 模式：

- `unpause` 成功後 Instance 的 `started_at` 被設定
- `pause` 成功後 `started_at` 被清除
- `stop` 成功後 `started_at` 被清除
- `POST /templates` 與 `PUT /templates/{id}` 接受並回傳 `max_run_seconds`／`timeout_action`
- `max_run_seconds < 60` 或非法 `timeout_action` → 400

### 5.4 前端測試（vitest）

沿用 `apps/web/src/tests/template-form.test.ts` 模式：

- 表單狀態包含使用時長與超時操作，未勾選（停用）送出 `max_run_seconds = null`，勾選後送出輸入的秒數
- `buildTemplateBody` 正確輸出 `max_run_seconds`／`timeout_action`，預設動作 `remove`
- 從既有 Template 回填時正確解析兩個欄位

## 六、不在此限

- 活躍連線保護：有使用者正連線的 VNC 工作階段仍然在時長上限時觸發（v1 為硬性上限，且目前無連線追蹤基礎設施）
- 全域管理上限：由管理員設定的平台級最大時長（v1 每個 Template 自行設定，後續可疊加）
- 超時前警告／通知：任何形式的提醒機制（v1 靜默執行並以 tracing 記錄）
- 前端倒數計時顯示：Instance 列表／詳細頁顯示剩餘時間（可在前端以 `started_at` + Template 時長計算，另行設計）
- 手動「立即睡眠」按鈕：在 Instance 上觸發超時動作的按鈕
- 累計／牆鐘計時語義：以「每次進入 running 重新計時」為唯一語義
- 啟動逾時（`starting` 卡死）的回收：沿用既有 120 秒 probe timeout → `error` 機制，不在本功能範圍
- 水平擴展：僅單一 API 行程執行 Auto-Sleep Worker，多實例衝突處理不在此限
- `remove` 時清理持久化 host 目錄檔案：僅刪除容器與 Instance row，host 檔案保留

## 七、補充說明

- 功能上線時，既有 `running` 的 Instance `started_at` 為 `NULL`，會被掃描跳過（安全預設），直到下次停止再啟動才開始計時
- Template 的 `timeout_action` 在 `max_run_seconds` 為 `NULL` 時不具效果；「有動作但未設時長」等同停用
- 每 3 秒全量掃描 `running` Instance 的成本可忽略（有 `status` index 且規模小）；若未來 Instance 數量大增，可再導入獨立的較長間隔
- `remove` 沿用 `delete_instance` 的清理順序（route → cache → stop → remove → 刪 row），確保與手動刪除行為一致
