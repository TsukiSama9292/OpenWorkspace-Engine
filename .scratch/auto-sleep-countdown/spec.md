Status: ready-for-agent

# Auto-Sleep 倒數計時器：進入實例後的剩餘時間顯示

## 一、問題陳述

平台已有 Auto-Sleep（見 `specs/auto-sleep.md`）：Template 設定 `max_run_seconds`，Instance 每次進入 `running` 即開始計時，到期後由背景工作程序執行 `pause`／`stop`／`remove`。但目前使用者只有在實例被回收的那一刻才知道這件事——進入實例之後，畫面上沒有任何剩餘時間的提示。

VNC 的介面是平台自家寫的網頁，疊一個計時器很容易；但 ttyd 與 Jupyter Lab 的路徑（`/ttyd/{token}/`、`/jupyter/{token}/`）整段被 Traefik 反向代理轉給容器自己的網頁，平台無法注入任何 JavaScript，也無法在那條路徑放平台自己的頁面。結果是使用者在 ttyd／Jupyter 中工作到一半，實例可能無預警被回收，造成工作進度與流程中斷。

## 二、解決方案

在 Instance 的 API 回傳中加入**伺服器計算的截止時間** `auto_sleeps_at` 與 `timeout_action`，前端依此顯示倒數：

- **VNC**：在自家頁面上疊一個倒數 overlay。
- **ttyd／Jupyter**：新增平台控制的 wrapper 頁面 `/open/{token}/`，以 `<iframe>` 嵌住原本的介面，在 iframe 上方疊同一個倒數 overlay（`pointer-events: none`，完全不攔截操作）。
- **Dashboard**：實例列表每一列顯示精簡剩餘時間（`剩 23:45`）。

倒數僅為**資訊性警告**（v1 不做「延長」），到期後照現行 Auto-Sleep 機制執行，不新增任何回收行為。

| 角色 | 行為 |
|---|---|
| API | Instance JSON 新增 `auto_sleeps_at`（running 且設有時長時 = `started_at + max_run_seconds`，否則 `null`）與 `timeout_action` |
| VNC 頁面 | 疊倒數 overlay（自家頁面，沿用既有進入流程） |
| ttyd／Jupyter | 新增 wrapper 頁面 `/open/{token}/`（iframe + 倒數 overlay）；Open 按鈕與詳情頁跳轉改指 wrapper |
| Dashboard | 實例列表每列顯示 `剩 23:45` |
| 到期後 | 不新增動作，由既有 Auto-Sleep Worker 執行 `pause`／`stop`／`remove` |

## 三、使用者故事

1. 身為 Instance 使用者，我希望進入 VNC 實例後畫面顯示倒數計時，以便知道還剩多少時間才會被 Auto-Sleep
2. 身為 Instance 使用者，我希望進入 ttyd 實例後畫面顯示倒數計時，以便無論哪種遠端介面都能掌握剩餘時間
3. 身為 Instance 使用者，我希望進入 Jupyter Lab 實例後畫面顯示倒數計時，以便同樣掌握剩餘時間
4. 身為 Instance 使用者，我希望倒數計時浮在畫面右上角且不攔截任何滑鼠與鍵盤操作，以便完全不影響我操作 VNC／ttyd／Jupyter
5. 身為 Instance 使用者，我希望剩餘時間以 `23:45` 格式顯示（超過一小時為 `1:23:45`），以便一眼讀出
6. 身為 Instance 使用者，我希望剩餘時間低於 10 分鐘時計時器轉為琥珀色，以便提前警覺接近時限
7. 身為 Instance 使用者，我希望剩餘時間低於 60 秒時計時器轉為紅色，以便獲得立即性的警示
8. 身為 Instance 使用者，我希望計時器旁以小字註明到期後將執行的動作（暫停／停止／移除），以便知道會發生什麼
9. 身為 Instance 使用者，我希望倒數歸零後顯示「已到期」狀態，以便知道 Auto-Sleep 正在執行
10. 身為 Instance 使用者，我希望未設定時長上限的實例不顯示任何倒數，以便不會造成誤導
11. 身為 Instance 使用者，我希望暫停或停止中的實例不顯示倒數，以便與「只有 running 時間計入時長」的既有語義一致
12. 身為 Instance 使用者，我希望分頁從背景切回時倒數自動重新對時，以便長時間停在背景的分頁能校正剩餘時間
13. 身為 Instance 使用者，我希望實例被提前暫停或停止時，wrapper 頁面顯示「已暫停／已停止」並提供回 dashboard 的連結，以便知道實例已不可用
14. 身為 Instance 使用者，我希望實例仍在 `starting` 時 wrapper 頁面顯示等待並自動輪詢，以便不用手動重新整理
15. 身為 Instance 使用者，我希望從 dashboard 開啟 ttyd／Jupyter 時進入 wrapper 頁面，以便倒數能在其中顯示
16. 身為 Instance 使用者，我希望從實例詳情頁開啟 ttyd／Jupyter 時同樣進入 wrapper 頁面，以便進入方式一致
17. 身為 Instance 使用者，我希望 VNC 仍從原本的自家頁面進入，以便不改變既有 VNC 體驗
18. 身為管理者，我希望 dashboard 實例列表每列顯示剩餘時間，以便一眼看出哪些實例即將到期
19. 身為 Instance 使用者，我希望顯示的截止時間由伺服器計算，以便與 Auto-Sleep 實際判定的時間一致
20. 身為 Instance 使用者，我希望 ttyd／Jupyter 的原始介面在 wrapper 的 iframe 中照常運作（含既有認證機制），以便無需重新登入或改變使用方式

## 四、實作決策

### 4.1 API 資料契約

`instance_to_json`（list 與 detail 共用）新增兩個欄位：

- `auto_sleeps_at`：ISO 8601 時間戳，或 `null`。計算規則與 Auto-Sleep Worker 的判定使用同一時脈：僅在 `status == 'running'`、`started_at` 有值、且其 Template 的 `max_run_seconds` 有值時，回傳 `started_at + max_run_seconds`；其餘情況一律回傳 `null`。
- `timeout_action`：Template 的 `timeout_action` 值（`remove`／`stop`／`pause`），Template 不存在時為 `null`。

實作上，`list_instances` 既有對每個 Instance 的 template lookup（目前帶出 name 與 remote_type）擴充為同時帶出 `max_run_seconds` 與 `timeout_action`；`get_instance` 已做 template lookup，一併擴充。截止時間的計算集中在單一 helper，供兩者共用，避免兩處邏輯漂移。

前端 `Instance` type 新增 `auto_sleeps_at: string | null` 與 `timeout_action: 'remove' | 'stop' | 'pause' | null`。

### 4.2 前端倒數模組

新增集中的倒數模組，內含純函式：

- `remainingMs(auto_sleeps_at, now)` → `number | null`（`null` = 不顯示倒數）
- `formatRemaining(ms)` → `MM:SS`，超過一小時 `H:MM:SS`
- `severity(ms)` → `normal` | `warning`（剩 < 10 分鐘）| `critical`（剩 < 60 秒）
- `wrapperUrl(remote_type, token)` 與 `iframeSrc(remote_type, token, password)`：組出 wrapper 頁面網址與 iframe 的原始網址（ttyd → `/ttyd/{token}/`；jupyter → `/jupyter/{token}/lab?token=…`）

`CountdownOverlay` 元件：固定於畫面右上角，`pointer-events: none`（完全不攔截操作）；顯示剩餘時間與「到期將{動作}」小字（`timeout_action` 為 `null` 時只顯示時間）；依 severity 切換顏色；歸零後顯示「已到期」。

### 4.3 三個使用點

- **VNC 頁面**（`/kasmvnc/{token}/`）：沿用既有以 `access_token` 查 Instance 的寫法，疊 `CountdownOverlay`。
- **Wrapper 頁面**（新路由 `open/[token]`）：以 `access_token` 查 Instance → 非 `running` 時依狀態顯示 waiting（`starting`，沿用輪詢模式）或「已暫停／已停止」+ 回 dashboard 連結；`running` 時渲染 iframe（src 由 `iframeSrc` 組出）+ `CountdownOverlay`。
- **Dashboard**：實例列表每列顯示精簡 `剩 23:45`（由同一模組的純函式產生）。
- **進入點調整**：dashboard 的 Open 按鈕與實例詳情頁的自動跳轉，對 ttyd／Jupyter 改指向 `wrapperUrl`；VNC 維持指向 `/kasmvnc/{token}/`。

### 4.4 重新同步

- 倒數每秒由前端自己扣；以**載入時**、**`visibilitychange` 切回分頁時**、以及**每 30 秒**一次，重新向 Instance API 對時。
- 歸零後 overlay 轉為「已到期」狀態，等待下一次 resync 帶回真實狀態（Auto-Sleep Worker 最慢 3 秒內執行）。
- resync 發現實例已離開 `running`：wrapper 顯示「已暫停／已停止」+ 回 dashboard 連結；VNC 依既有 websocket 斷線機制自然呈現。

### 4.5 Wrapper 路由與 Proxy

- 新增 SvelteKit 頁面 `open/[token]`，**不需新增 Traefik／nginx 設定**（該路徑未被任何動態 route 佔用，走預設 `/` → nginx → SvelteKit）。
- ttyd 的認證由 Traefik middleware 注入 Authorization header、Jupyter 的認證走 URL query token，兩者在 iframe（同源路徑）下照常生效。

## 五、測試決策

### 5.1 測試原則

只測試外部行為。API 測「使用者從 endpoint 拿到的 JSON 合約」；前端測「計時模組的輸出與 overlay 的渲染」。不測 resync 的 interval 細節、iframe 內容器網頁的行為、或 Docker 實作。

### 5.2 API 接縫（最高接縫）

沿用 `tests/common` Postgres 測試 harness 與既有 `instances_test.rs` 模式，透過 endpoint response 斷言：

- `running` + `started_at` + Template 有 `max_run_seconds` → `auto_sleeps_at` 等於 `started_at + max_run_seconds`
- Template 無 `max_run_seconds` → `auto_sleeps_at` 為 `null`
- `paused`／`stopped` → `auto_sleeps_at` 為 `null`
- `timeout_action` 正確透傳
- list 與 detail 兩個 endpoint 都包含這兩個欄位

### 5.3 前端接縫（vitest）

- 純函式：`formatRemaining` 各格式、`severity` 臨界值、`remainingMs` 的 `null` 處理（先例：`format.test.ts`）
- `CountdownOverlay` 元件：剩餘時間文字、severity 對應 class、歸零「已到期」、`pointer-events: none`（先例：`template-panel.test.ts`）
- `wrapperUrl`／`iframeSrc` 純函式（先例：`template-form.test.ts`）

頁面層（`open/[token]`）為薄的資料載入與組件組裝，不測試。

## 六、不在此限

- 延長（extend）功能：重設 `started_at` 的 API 與「+N 分鐘」按鈕（v1 純資訊型，到期照現行機制執行）
- 超時前的推播、音效等額外提醒機制
- 活躍連線保護、平台級全域上限（沿用 `specs/auto-sleep.md` 的 out of scope）
- 直接在 ttyd／Jupyter 容器網頁內注入計時器（以 wrapper 方案替代）
- VNC 改走 wrapper（維持自家頁面）
- 客戶端與伺服器時鐘 skew 的校正（接受 skew，以 30 秒 resync 緩解）
- 進度條等其他視覺形式
- dashboard 依剩餘時間排序／篩選

## 七、補充說明

- 此 spec 是 `specs/auto-sleep.md`「不在此限」所列「前端倒數計時顯示」的正式設計。
- 已知限制：倒數為資訊性且依賴客戶端時鐘，與伺服器可能存在 skew；到期執行權仍在 Auto-Sleep Worker。
- 風險：JupyterLab 內嵌 iframe 的相容性（目前無 X-Frame-Options 阻擋的疑慮），驗證時若發現問題需回報並另行處理。
- `timeout_action` 為 `null`（Template 不存在）時，overlay 只顯示時間、不顯示動作文字。
- 使用者可隨時從 dashboard 對實例 pause／stop／resume 自救，倒數僅是警告。
