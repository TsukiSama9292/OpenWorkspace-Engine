Status: ready-for-agent

# Keep Time：閒置偵測與自動回收

## 一、問題陳述

這個平台要在一台 Linux 主機上共享運算資源給多位開發者，每個 Instance 都是從 Template 啟動的常駐容器（KasmVNC / ttyd / Jupyter Lab）。目前只有「使用時長」上限（`max_run_seconds`）能防止資源被長期佔用，但它是一個**硬性上限**：即使使用者正看著畫面、正在工作，時間到仍然回收。

實際上更常見的浪費是**閒置**：使用者把遠端畫面開著卻離開座位、把分頁丟在背景、或啟動了 Instance 卻從沒打開過畫面——這些情況下容器仍然持續耗用 CPU、記憶體與 GPU，直到手動停止或刪除。

在多人共享的環境下，閒置 Instance 會綁死資源，導致其他使用者啟動新 Instance 時無資源可用。既有 Auto-Sleep 無法處理「有在使用就延長、沒在使用才回收」的情境。

## 二、解決方案

新增 **Keep Time（閒置保持時間）** 功能，與既有 Auto-Sleep 並存且各自獨立：

- 在 **Template** 上新增兩個欄位：**閒置保持時間**（`keep_time_seconds`）與**閒置回收操作**（`keep_time_action`）。
- 只要遠端畫面在瀏覽器分頁中**開啟、可見且聚焦**，Instance 就持續被「保活」：前端每 10 秒回報心跳，API 記錄 `last_seen_at`，計時器持續重置。
- 一旦瀏覽器分頁**不可見或未聚焦**（或瀏覽器整個關閉），計時器開始倒數；連續閒置超過 `keep_time_seconds` 即執行設定的回收操作（`pause`／`stop`／`remove`）。
- 從未被打開的 Instance 視為從進入 `running` 起就開始閒置，同樣適用回收。

| 角色 | 行為 |
|---|---|
| Template | 新增 `keep_time_seconds`（NULL = 停用）與 `keep_time_action`（預設 `pause`） |
| 前端（遠端畫面頁） | 分頁可見且聚焦時，每 10 秒回報心跳；失焦／隱藏即停止；重新聚焦立即回報 |
| API（心跳端點） | `POST /api/instances/{id}/heartbeat` 更新 `last_seen_at = NOW()` |
| Keep-Time Worker | 每 3 秒掃描 `running` 且 `last_seen_at` 有值的 Instance，`now - last_seen_at >= keep_time_seconds` 即執行 Template 設定的動作 |

## 三、使用者故事

1. 身為 Template 擁有者，我希望能在 Template 上設定閒置保持時間，以便限制「沒有人在看」的 Instance 佔用資源的時間
2. 身為 Template 擁有者，我希望能在 Template 上設定閒置回收操作（pause/stop/remove），以便決定閒置時間到之後資源如何回收
3. 身為 Template 擁有者，我希望閒置回收操作的預設值是 `pause`，以便預設行為最溫和、工作內容留在記憶體中
4. 身為 Template 擁有者，我希望未設定閒置保持時間的 Template 不會被閒置回收，以便保留原本「無閒置上限」的行為
5. 身為 Template 擁有者，我希望修改 Template 的閒置保持時間後立刻對所有從此 Template 啟動的 Instance 生效，以便不需要重啟 Instance 也能調整政策
6. 身為 Instance 使用者，我希望只要遠端畫面開啟且聚焦，Instance 就一直被保活，以便我在工作時計時器不會倒數
7. 身為 Instance 使用者，我希望分頁失焦、隱藏或瀏覽器關閉時計時器才開始倒數，以便「只有沒在看才回收」
8. 身為 Instance 使用者，我希望回到畫面聚焦後計時器重置、重新獲得完整的保持時間，以便間歇性的短暫離開不會被累計
9. 身為 Instance 使用者，我希望啟動後從未打開過畫面的 Instance 也從進入 running 起開始計時，以便遺忘啟動的 Instance 一樣會被回收
10. 身為 Instance 使用者，我希望被 `remove` 的 Instance 從列表消失、容器被完全刪除，以便確認資源已被釋放
11. 身為 Instance 使用者，我希望被 `stop` 的 Instance 狀態變成 `stopped` 並可再次啟動，以便之後還能繼續使用（重新啟動後重新計時）
12. 身為 Instance 使用者，我希望被 `pause` 的 Instance 狀態變成 `paused` 並可恢復，以便工作內容保持在記憶體中、恢復成本最低
13. 身為 Instance 使用者，我希望閒置計時只適用於 `running` 的 Instance，以便暫停／停止期間不會被額外計時
14. 身為 Instance 使用者，我希望 Keep Time 與使用時長（Auto-Sleep）各自獨立、先到先觸發，以便兩種政策可以同時生效且行為可預期
15. 身為 Instance 使用者，我希望在畫面可見但瀏覽器視窗未聚焦時能看到閒置倒數與將執行的動作，以便有機會點回視窗取消回收
16. 身為管理者，我希望閒置回收不需使用者或管理者介入，以便資源持續被自動釋放
17. 身為管理者，我希望功能上線前就已存在的 `running` Instance 不會被立刻回收，以便舊工作階段不會無預警消失
18. 身為 Template 擁有者，我希望透過 API 設定閒置保持時間時有最小下限（60 秒）驗證，以便避免誤設過短時間造成 Instance 立刻被回收
19. 身為 Template 擁有者，我希望在建立與編輯 Template 的表單中都能設定閒置保持時間與回收操作，以便完整管理
20. 身為 Template 擁有者，我希望編輯既有 Template 時表單能顯示已儲存的閒置設定，以便確認與修改現有設定
21. 身為 Instance 使用者，我希望閒置計時與 Template 目前的設定一致，以便政策調整立即反映在執行中的 Instance 上
22. 身為 Instance 使用者，我希望在執行中的 Instance 上同時存在 Auto-Sleep 與 Keep Time 倒數時，畫面只顯示較早到期的哪一個，以便資訊不重疊
23. 身為 Instance 使用者，我希望 Keep Time 到期而 Instance 離開 `running` 時，畫面自動導回儀表板，以便不會停留在一個已回收的畫面上
24. 身為其他使用者，我希望別人的心跳無法延長我的 Instance 的存活時間，以便閒置回收不會被惡意或誤用規避

## 四、實作決策

### 4.1 計時語義

「活躍」的定義為：瀏覽器分頁**可見且聚焦**（`document.visibilityState === 'visible' && document.hasFocus()`），且 Instance 狀態為 `running`。不要求遠端連線狀態、也不要求滑鼠／鍵盤輸入。

- 分頁可見且聚焦期間：前端持續回報心跳，`last_seen_at` 不斷刷新為 `NOW()`，計時器**重置**（非暫停、非累計）——每一次聚焦都重新獲得完整的保持時間。
- 分頁失焦、隱藏或瀏覽器關閉期間：不再有心跳，`last_seen_at` 凍結，倒數開始。
- 判定為 `now - last_seen_at >= keep_time_seconds`，即「連續閒置超過 keep_time」才觸發。
- 從未被打開的 Instance：`last_seen_at` 初始化為進入 `running` 的時刻（等同 `started_at`），因此從啟動起即開始計時。

Keep Time 與既有 Auto-Sleep 各自獨立：兩者皆可由 Template 設定，**先到先觸發**。各自使用自己的動作欄位（`keep_time_action` 與 `timeout_action` 不共用），使同一 Template 可同時有「硬性時長上限 remove」與「溫和閒置 pause」兩種政策。

### 4.2 資料庫變更

新增一支 migration：

- `workspace_templates` 新增欄位：
  - `keep_time_seconds BIGINT NULL` — 閒置保持時間（秒）。`NULL` = 停用（既有 Template 遷移後皆為 `NULL`，保持原有行為）
  - `keep_time_action VARCHAR(20) NOT NULL DEFAULT 'pause'` — 閒置回收動作，合法值 `remove`／`stop`／`pause`（加 DB CHECK 約束）
- `workspace_instances` 新增欄位：
  - `last_seen_at TIMESTAMPTZ NULL` — 最近一次活躍（心跳）時間。`NULL` = 尚未有活躍記錄

DB Entity（`workspace_template`、`workspace_instance`）與公開 struct（`WorkspaceTemplate`、`WorkspaceInstance`）同步新增對應欄位。

### 4.3 Repository 變更

- `WorkspaceTemplateRepository::create` / `update` 增加參數：`keep_time_seconds: Option<i64>`、`keep_time_action: &str`
- `WorkspaceInstanceRepository` 新增 `update_last_seen_at(id, Option<DateTimeUtc>)`，用於設定／清除 `last_seen_at`
- 新增查詢：列出 `status = 'running'` 且 `last_seen_at IS NOT NULL` 的 Instance，供 Keep-Time Worker 掃描

### 4.4 `last_seen_at` 的生命週期

`last_seen_at` 與 `started_at` **同進同出**，設定與清除點完全平行：

- 進入 `running`（健康檢查提升、start、unpause）時：`last_seen_at = NOW()`（與 `started_at` 一起設定）
- 離開 `running`（pause、stop）時：`last_seen_at = NULL`（與 `started_at` 一起清除）

這樣任何一次進入 `running` 都是全新的閒置計時，且與 Auto-Sleep 的 `started_at` 語義一致。

### 4.5 Keep-Time Worker（`health_worker.rs`）

在既有 3 秒背景迴圈中，除了現有的 `check_instances` 與 `check_auto_sleep` 之外，新增 `check_keep_time` 掃描。掃描僅處理**同時滿足**下列條件的 Instance：

1. `status == 'running'`
2. `last_seen_at IS NOT NULL`（`NULL` 的舊 Instance 一律跳過，直到下次重新進入 `running`）
3. 其 Template 的 `keep_time_seconds IS NOT NULL`

判斷 `now - last_seen_at >= keep_time_seconds` 成立即執行 Template 的 `keep_time_action`。Template 設定每個 tick 讀取現值——中途修改時間／動作立即生效，不採 snapshot。回收動作**與 Auto-Sleep 共用同一組 helper**（從 `auto_sleep_remove`／`auto_sleep_stop`／`auto_sleep_pause` 抽出為共享函式），清理序列與既有路徑一致：

- **remove**：`delete_route` → `vnc_cache.remove` → stop 容器 → remove 容器 → 刪除 Instance row（Template 的持久化 host 目錄檔案保留在磁碟，不刪除）
- **stop**：stop 容器 → `delete_route` → `vnc_cache.remove` → 更新狀態為 `stopped`
- **pause**：pause 容器 → 更新狀態為 `paused`（保留 Traefik route，與手動 pause 一致）

狀態更新在 Docker 動作執行後進行，即使 Docker 呼叫失敗仍更新（與現有路徑「updating DB anyway」一致），避免 Instance 卡在掃描迴圈。`check_keep_time` 設計為 `pub` 函式、時鐘由參數注入（見 5.2），與 `check_auto_sleep` 同一個測試接縫。

### 4.6 心跳端點（`instances.rs`）

新增 `POST /api/instances/{id}/heartbeat`：

- 需要登入，且通過 `can_manage_instance`（擁有者或具管理權限者）檢查
- 無 request body；成功後將該 Instance 的 `last_seen_at` 設為 `NOW()`，回傳 200
- Instance 不存在 → 404；無權限 → 403
- 多個分頁同時開啟時自然共用同一 Instance 的心跳，任何一個活躍分頁都能保活

### 4.7 前端心跳（新 keepalive 模組）

新增一個 keepalive 模組，僅在兩個遠端畫面頁（`/kasmvnc/[token]/` 與 `/open/[token]/`）且 Instance 為 `running` 時啟用：

- 每 10 秒發送一次心跳；分頁**可見且聚焦**時才計時，失焦／隱藏立即停止
- 重新聚焦時立即發送一次心跳（即時重置計時）
- 心跳失敗靜默忽略（下次心跳重試），不阻斷使用者操作

### 4.8 Instance JSON 與倒數顯示

Instance 的 JSON 新增兩個欄位（僅 `running` 且有值時回傳）：

- `keep_time_deadline`：`last_seen_at + keep_time_seconds`（供前端顯示倒數）
- `keep_time_action`：Template 的 `keep_time_action`

倒數覆蓋層沿用既有 `CountdownOverlay`，與 Auto-Sleep 合併為**單一徽章**，顯示較早到期的那個 deadline：

- 有 `auto_sleeps_at`（Auto-Sleep）時，徽章照常顯示（聚焦時也顯示，與現況一致）
- 只有 `keep_time_deadline` 時，僅在分頁**可見但未聚焦**時顯示（聚焦時沒有倒數可顯示）
- 兩者同時存在時顯示較早到期者
- 到期（歸零）時沿用既有 `onResync`／導回儀表板流程（`hadDeadline && status !== 'running'` 時導向 `/`）；背景分頁計時器被瀏覽器節流不影響正確性，伺服器獨立執行回收

### 4.9 Template API

`POST /templates`、`PUT /templates/{id}` 的 request 增加：

- `keep_time_seconds`：可空整數（秒）。驗證：若提供則 ≥ 60（下限 60 秒）
- `keep_time_action`：字串，限 `remove`／`stop`／`pause`，未提供時預設 `pause`

Template 的 response 一律包含這兩個欄位，供前端表單回填。

### 4.10 前端（Template 表單與詳細頁）

- Template 表單新增「閒置保持時間」設定：勾選啟用後以數字輸入秒數（下限 60 秒）；未勾選 = 停用，送出 `null`
- 新增「閒置回收操作」下拉：`pause`（預設）／`stop`／`remove`
- 建立與編輯共用同一表單狀態；編輯時從 Template response 回填
- Instance 詳細頁新增一行顯示已設定的閒置設定（例如「閒置 15 分鐘後暫停」），無設定時不顯示
- 儀表板 Instance 卡片不顯示 Keep Time 資訊，保持列表乾淨

## 五、測試決策

### 5.1 測試原則

只測試外部行為：給定 DB 中的 Instance／Template 狀態與 Docker mock 的呼叫預期，Keep-Time 掃描產生正確的狀態轉換與 Docker 呼叫。不測試掃描的 tick interval、心跳網路細節或 Docker 實作。前端只測試心跳排程的行為與倒數顯示，不測試實際 HTTP。

### 5.2 最高接縫：`check_keep_time`（單一主要接縫）

`check_keep_time` 為 `pub` 函式，簽名與 `check_auto_sleep` 同形：注入 `instance_repo`、`template_repo`、`&dyn DockerService`（mockall mock）、`vnc_cache`，並以參數注入時鐘 `now: DateTime<Utc>`，使「是否超過閒置上限」的判定可以決定性地測試，不需等待真實時間。測試沿用現有 Postgres 測試 harness（`tests/common`）與 mockall DockerService（既有 `health_worker_test.rs` 模式）。

案例：

- `running` + `last_seen_at` 早於 `now - keep_time_seconds` + 動作 `remove` → Docker 呼叫 stop + remove，route 刪除、`vnc_cache` 清空、Instance row 被刪除
- 同上但動作 `stop` → Docker 呼叫 stop、狀態變為 `stopped`、`last_seen_at` 為 `NULL`
- 同上但動作 `pause` → Docker 呼叫 pause、狀態變為 `paused`、`last_seen_at` 為 `NULL`
- `running` 且 `last_seen_at` 在 `keep_time_seconds` 內（最近有心跳）→ 無任何 Docker 呼叫、狀態不變
- `running` 但 `last_seen_at IS NULL`（舊 Instance）→ 跳過
- `running` 但 Template `keep_time_seconds IS NULL`（停用）→ 跳過
- Template 設定中途調高（snapshot 語意）→ 不觸發；調低／清除 → 依現值觸發或不觸發
- 已觸發後（狀態已離開 `running`）→ 下次掃描不重複觸發
- `now` 未注入時由呼叫端傳入 `Utc::now()`，行為一致

### 5.3 Route 層級測試（次級接縫）

沿用 `instances_mock_test.rs` 模式：

- 心跳端點：登入後呼叫成功更新 `last_seen_at`；未登入 → 401；非擁有者且無管理權限 → 403；Instance 不存在 → 404
- 進入 `running`（unpause）後 `last_seen_at` 被設定
- `pause`／`stop` 成功後 `last_seen_at` 被清除
- `POST /templates` 與 `PUT /templates/{id}` 接受並回傳 `keep_time_seconds`／`keep_time_action`
- `keep_time_seconds < 60` 或非法 `keep_time_action` → 400

### 5.4 前端測試（vitest）

沿用 `apps/web/src/tests/` 既有模式（`countdown.test.ts`、`template-form.test.ts`、jsdom + fake timers）：

- keepalive 模組：分頁可見且聚焦時每 10 秒發送心跳；失焦／隱藏即停止；重新聚焦立即發送一次
- `countdown.ts`：`keep_time_deadline` 的剩餘時間計算與格式化；合併倒數顯示選擇較早 deadline 的邏輯
- `template-form.ts`：表單狀態包含閒置設定，未勾選（停用）送出 `keep_time_seconds = null`，勾選後送出輸入的秒數；`buildTemplateBody` 正確輸出兩個欄位（預設動作 `pause`）；從既有 Template 回填時正確解析

## 六、不在此限

- 輸入活動偵測：以滑鼠／鍵盤輸入重置閒置計時（v1 以「分頁可見且聚焦」為唯一活躍定義；分頁聚焦但人離開座位的「螢幕保護程式漏洞」是已知限制）
- 連線狀態偵測：以遠端連線（RFB／websocket）狀態作為活躍條件（v1 不檢查）
- 全域管理上限：由管理員設定的平台級閒置政策（v1 每個 Template 自行設定）
- 回收前警告／通知：任何形式的提醒機制（v1 靜默執行並以 tracing 記錄；僅在分頁可見未聚焦時顯示倒數徽章）
- 儀表板卡片顯示剩餘閒置時間：v1 僅 Template 表單與 Instance 詳細頁一行說明
- 手動「立即回收」按鈕：在 Instance 上觸發回收動作的按鈕
- 累計／暫停計時語義：以「每次聚焦重置、連續閒置達標才觸發」為唯一語義
- 水平擴展：僅單一 API 行程執行 Keep-Time Worker，多實例衝突處理不在此限
- `remove` 時清理持久化 host 目錄檔案：僅刪除容器與 Instance row，host 檔案保留
- 心跳的安全額外強化（如 per-instance 心跳 token、限流）：沿用現有登入＋權限檢查

## 七、補充說明

- 功能上線時，既有 `running` 的 Instance `last_seen_at` 為 `NULL`，會被掃描跳過（安全預設，不會把舊工作階段立刻回收），直到下次停止後重新啟動／暫停恢復才開始計時
- Template 的 `keep_time_action` 在 `keep_time_seconds` 為 `NULL` 時不具效果；「有動作但未設時間」等同停用
- 每 3 秒全量掃描 `running` Instance 的成本可忽略（與 Auto-Sleep 共用同一掃描迴圈）；若未來 Instance 數量大增，可再導入獨立的較長間隔
- 心跳端點為輕量寫入（每 10 秒、每個被觀看的 Instance 一次），負載可忽略；多分頁共用同一 Instance 心跳，天然無衝突
- 背景分頁的 `setInterval` 會被瀏覽器節流，但伺服器獨立執行回收，前端倒數僅為警告性質，正確性不受影響
