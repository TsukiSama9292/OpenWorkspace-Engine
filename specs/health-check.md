## 一、問題陳述

啟用 Instance（Launch 或 Start）後，Container 內的服務（KasmVNC / ttyd / Jupyter Lab）需要數秒到數十秒才能完全啟動。目前 API 在 Container 建立或啟動後立即將狀態設為 `running`，並寫入 Traefik 路由，前端隨即導向 `/{remote_type}/{token}/`。使用者會在瀏覽器看到空白頁面、連線錯誤或「啟動中…」的過渡狀態，直到服務真正就緒。

KasmVNC 的 VNC Viewer 有內建 30 次重試（每次 1 秒），因此最終能自動連上。但 ttyd 與 Jupyter Lab 沒有相應的重試機制，使用者只能手動重新整理，體驗不一致且容易產生困惑。

## 二、解決方案

引入「健康檢查等待」機制：API 在 Container 啟動後將 Instance 狀態設為 `starting`，由一個背景工作程序（Background Worker）定期探測 Container 內部的服務埠，確認服務已回應後才將狀態更新為 `running`。前端在 Launch 或 Start 後導向 Instance 詳細頁面，該頁面輪詢 Instance 狀態，直到 `running` 後自動重新導向至遠端 URL。

| 角色 | 行為 |
|---|---|
| API（launch/start） | 狀態設為 `starting`，不回堵等待 |
| Background Worker | 每 3 秒掃描所有 `starting` 的 Instance，探測服務埠，成功 → `running`，逾時 120 秒 → `error` |
| 前端（詳細頁面） | 每 2 秒輪詢 `GET /instances/{id}`，顯示「啟動中…」動畫，`running` 後自動跳轉 |

## 三、使用者故事

1. 身為使用者，我希望 Launch 或 Start Instance 後能在同一頁面看到「啟動中」的狀態提示，以便了解 Instance 正在初始化
2. 身為使用者，我希望 Instance 的服務真正就緒後頁面自動導向遠端桌面/終端機/Jupyter，以便不需要手動重新整理
3. 身為使用者，我希望如果啟動逾時（120 秒），頁面顯示錯誤訊息並提供「重試」按鈕，以便知道出了什麼問題
4. 身為使用者，我希望 Instance 列表頁面正確顯示 `starting` 狀態，以便辨別哪些 Instance 正在啟動中
5. 身為管理者，我希望在 Instance 詳細頁面也能對 `starting` 狀態的 Instance 執行 Stop 操作，以便終止異常啟動的程序
6. 身為管理者，我希望 Launch 後導向 Instance 詳細頁面而非直接連遠端 URL，以便在啟動期間可看到進度與取消選項

## 四、實作決策

### 4.1 狀態機擴展

Instance 的 `status` 欄位新增 `starting` 狀態。完整狀態轉換：

```
stopped → starting → running → paused → stopped
                              ↘ error
starting → error (逾時)
```

原有 `"running"` / `"stopped"` / `"paused"` / `"error"` 不變。

### 4.2 Background Worker（新增 `health_worker.rs`）

在 API 行程中啟動一個 Tokio task，負責探測 Container 服務就緒狀態：

```
loop {
    // 每 3 秒執行一次
    tokio::time::sleep(Duration::from_secs(3)).await;

    // 查詢所有 status = "starting" 的 Instance
    let instances = db.query("SELECT * FROM instances WHERE status = 'starting'");

    for each instance {
        // 取得 template → remote_type → port (6901 / 7681 / 8888)
        let template = template_repo.find_by_id(instance.template_id);
        let port = match template.remote_type {
            "kasmvnc" => 6901,
            "ttyd" => 7681,
            "jupyter" => 8888,
        };

        // 從 Docker 取得 Container IP
        let ip = docker.get_container_ip(instance.container_id);

        // 以 reqwest + danger_accept_invalid_certs(true) 探測
        let url = format!("https://{ip}:{port}/");
        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .timeout(Duration::from_secs(5))
            .build();
        match client.get(&url).send().await {
            Ok(_) => {
                // 任何 HTTP response 都視為健康
                update_status(instance.id, "running");
            }
            Err(_) => {
                // 逾時判斷：檢查 updated_at + 120s < now()
                if instance.updated_at + 120s < now() {
                    update_status(instance.id, "error");
                }
                // 否則下次再試
            }
        }
    }
}
```

Worker 透過 `AppState` 取得 DB connection 與 DockerService trait。

### 4.3 API 變更（`instances.rs`）

**launch_instance**：Container 建立成功後，狀態改為 `"starting"`（原為 `"running"`）。Container IP 取得與 Traefik route 寫入照常執行（route 可提前寫入，Traefik 會回傳 502/503 直到服務就緒）。

**start_instance**：Container 啟動成功後，狀態改為 `"starting"`（原為 `"running"`）。

**stop_instance / pause / unpause**：不變。若 Instance 處於 `starting` 狀態，Stop 可直接將狀態轉為 `stopped`（Worker 下次掃描時會跳過已不在 `starting` 的 Instance）。

### 4.4 AppState 整合（`main.rs`）

在 API 啟動時 spawn background worker：

```rust
let worker_db = state.db.clone();
let worker_docker = state.docker.clone();
tokio::spawn(async move {
    health_worker::run(worker_db, worker_docker).await;
});
```

Worker 具有 `DockerService` trait 的 `dyn` 存取權，測試時可注入 mock。

### 4.5 前端變更

**types.ts**：Instance.status 型別擴充為 `'running' | 'stopped' | 'paused' | 'error' | 'starting'`。

**+page.svelte** (confirmLaunch)：
- 原本 Launch 完成後直接 `window.location.href = url`；改為 `window.location.href = '/instances/{id}'`

**instances/[id]/+page.svelte**：
- 新增 `starting` 狀態處理：
  - 顯示「啟動中…」spinner（取代原本的 action buttons 區）
  - 每 2 秒執行 `loadInstanceDetail(instanceId)` 重新載入
  - 當 `status === 'running'`，自動執行 `window.location.href = instanceUrl(instance)`
  - 當 `status === 'error'`，顯示錯誤訊息與「重試」按鈕（呼叫 `onAction('start')`）
- 保留 Stop 按鈕，讓使用者在 `starting` 狀態可終止啟動
- 其餘狀態（running / stopped / paused / error）行為不變

**Dashboard 列表（+page.svelte）**：
- 在 Instance 列表的狀態 badge 支援 `"starting"` 值，以不同的顏色/文字顯示

## 五、測試決策

### 5.1 測試原則

只測試外部行為：Worker 對 DB 狀態的更新、Worker 對 Docker IP 的查詢、健康檢查成功/失敗時的正確狀態轉換。不測試 Worker 的 sleep interval 或 reqwest 內部行為。

### 5.2 Worker 測試（最高接縫）

透過 `DockerService` 的 mockall mock 與實際 DB（或 in-memory SQLite）測試 Worker：

- 給定一個 DB 中 status = `starting` 的 Instance + Docker mock 回傳 IP + reqwest mock 成功 → worker 應將 status 更新為 `running`
- 給定一個 DB 中 status = `starting` 的 Instance + Docker mock 回傳 IP + reqwest mock 失敗且未逾時 → worker 不應變更 status（下次再試）
- 給定一個 DB 中 status = `starting` 的 Instance + 已超過 120 秒 + reqwest mock 持續失敗 → worker 應將 status 更新為 `error`

### 5.3 API Route 測試

沿用現有 `templates_test.rs` / `instances_mock_test.rs` 的 mock 測試模式：

- `POST /instances` launch 成功時回傳 `instance.status === "starting"`（而非 `"running"`）
- `POST /instances/{id}/start` 成功時回傳 `status === "starting"`
- `GET /instances/{id}` 回傳的 instance 包含 `"starting"` 狀態

### 5.4 前端測試（vitest）

- `instances/[id]/+page.svelte`：給定 mock instance 的 status = `starting`，確認頁面顯示「啟動中」spinner
- `instances/[id]/+page.svelte`：給定 mock instance 的 status = `running`，確認頁面呼叫 `window.location.href =` 指向正確的遠端 URL
- `+page.svelte`：Launch 完成後確認導向 `/instances/{id}`

## 六、不在此限

- Traefik 路由在服務就緒前回傳 502/503 的處理（由 Traefik 預設行為處理，不影響使用者體驗，因為使用者在健康檢查完成前不會被導向該 URL）
- Container 內部的 startup probe / liveness probe 設定（由 Docker image 自行管理）
- Worker 的多實例衝突處理（只有一個 API 行程，暫不考慮水平擴展）
- Instance launch 後的連線速度測試或效能最佳化

## 七、補充說明

Worker 使用 in-memory 的 `updated_at` 比對來判斷逾時，不另外儲存啟動時間戳記。每次啟動 API 時 Worker 重新開始掃描，若 API 重啟前已有 `starting` 的 Instance，重啟後 Worker 會根據 DB 中的 `updated_at` 判斷是否已逾時。
