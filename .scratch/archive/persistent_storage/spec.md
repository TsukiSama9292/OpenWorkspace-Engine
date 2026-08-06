Status: completed

# 資料持久化：Instance 使用者資料的 Persistent Volume 設計

## 問題陳述

這個平台在一台 Linux 主機上共享資源給多位開發者，每個 Instance 都是從 Template 啟動的常駐容器（KasmVNC / ttyd / Jupyter Lab）。目前 Instance 的生命週期是無狀態的：stop / remove 之後容器內的檔案就消失，使用者無法保留工作進度（例如 Jupyter notebook、IDE 設定、套件安裝），每次重新啟動都要重來。

現有的持久化欄位是**半成品且不安全**：

- Template 的 `persistent_storage_path` 是死欄位 — 從未在容器啟動時被讀取，純粹儲存與回傳。
- Instance 的 `mount_persistent` 與 `resolved_volume_host_path` 由 **client 直接提供**，API 完全沒有驗證——任何登入使用者都能送 `mount_persistent: true` + `resolved_volume_host_path: "/"`（或 `/etc`、`/home`、docker socket），把 host 任意路徑以 rw 掛載進自己的 Instance，等於直接讀寫整台 host 檔案系統。
- 前端 launch 時根本不會送出這兩個欄位，所以持久化功能目前完全不可達。
- 容器內 mount 目標寫死 `/home/kasm_user/persistent`，但三個映像的使用者與 home 都不同（kasmvnc = `kasm-user`、ttyd/jupyter = `ow_user`），掛載位置對大多數 Instance 沒有意義。

**核心技術挑戰 — Host Bind Mount 的「遮蔽 effect」**：把 host 目錄直接 bind mount 到容器使用者的 home 目錄（如 `/home/kasm-user`）時，Docker 以 **host 目錄的內容為準直接覆蓋** 容器路徑，不會複製任何檔案。若 host 目錄是空的，容器 Image 內建的 `.bashrc`、`.config`、X11 啟動腳本、VNC 權限檔、Jupyter 設定等全部被蓋掉，容器啟動時因找不到必要的桌面與環境設定檔而**崩潰或黑屏**。因此「直接掛載整個 home」的簡單做法不可行。

此外，同一 (Template, 使用者) 可以無限制啟動多個持久化 Instance，共享同一份資料時會互相干擾；也沒有任何「清空重來」的機制。

## 解決方案

建立一套**由 API 主導、路徑可預測、一個 (Template, 使用者) 只有一個持久化 Instance** 的資料持久化設計。關鍵機制是 **Local Bind-mounted Named Volume**：Named Volume 第一次掛載到容器內非空目錄時，**Docker Daemon 會自動把 Image 內該目錄的原始檔案複製（Populate）進 Volume**——既保留內建環境，後續修改又被安全持久化；同時用 `o=bind` 把 Volume 的實際儲存位置指到 host 上 API 完全控管的固定路徑。

1. **Template 設定持久化根目錄**（`persistent_storage_path`）：Web 端在 Template 表單設定一個 host 上的 root 目錄（例如 `/mnt/ow_dir`），由管理員預先建立並授權。此欄位不再是死欄位。
2. **API 解析出 instance 專屬路徑**：啟動 Instance 時，API 以 `{persistent_storage_path}/{template_name}/{owner_user_id}` 解析出唯一路徑，並以正則驗證（絕對路徑、禁止 `..` 穿越、禁止注入）。**不信任 client 提供的路徑**——launch request 不再接受 `resolved_volume_host_path`。
3. **Launch 時由使用者選擇持久化模式**：在啟動選單（「當前分頁 / 新分頁」旁）新增資料持久化選項（**僅當 Template 設定了 `persistent_storage_path` 時顯示**；預設為使用持久化）：
   - **使用資料持久化（Use persistent storage）**：以 Named Volume 掛載解析出的 host 目錄。若該 (Template, 使用者) 已存在持久化 Instance → 拒絕啟動（409）。remove 後資料保留，可再次以本選項重用。
   - **不使用資料持久化（No persistent storage）**：不掛載，無限啟動。
   - **重置資料持久化（Reset persistent storage）**：清除既有的 host 資料目錄與 Volume 宣告，再以全新的空 Volume 重新啟動（重新觸發 Image 檔案複製）。同樣受「一個持久化 Instance」限制，已有 Instance → 拒絕。前端選到它時跳出確認警告（會清除資料）。
4. **容器內 mount 目標依 remote_type 固定**為使用者的**整個 home 目錄**，讓 Image 內建環境首次自動複製、之後所有修改持久化：

   | remote_type | 容器內 mount 目標 |
   |---|---|
   | `kasmvnc` | `/home/kasm-user` |
   | `ttyd` | `/home/ow_user` |
   | `jupyter` | `/home/ow_user` |

5. **API 是容器，由一次性 helper container 建立 host 目錄**：ow-api 透過 Bollard 先拉起一個短命（`--rm`）的 alpine 容器，把 root 目錄 bind mount 進去執行 `mkdir`（與 `chown`），確保 host 出現乾淨的空資料夾，之後才建立 Named Volume 與啟動租戶容器。helper 容器執行完即自動銷毀。

## User Stories

1. 身為 **Instance 使用者**，我希望啟動 Instance 時可以選擇「使用資料持久化」，以便容器被 stop / remove 後我的檔案仍然保留。
2. 身為 **Instance 使用者**，我希望啟動 Instance 時可以選擇「不使用資料持久化」，以便臨時性的、不需要保留資料的工作不會留下任何 host 檔案。
3. 身為 **Instance 使用者**，我希望啟動 Instance 時可以選擇「重置資料持久化」，以便清掉舊的持久化資料、用乾淨狀態重新開始。
4. 身為 **Instance 使用者**，我希望持久化資料存放在我自己的使用者目錄下，以便與其他使用者的資料隔離、不會互相看到。
5. 身為 **Instance 使用者**，我希望同一個 Template 只能有一個使用持久化的 Instance，以便我不會在同一個資料目錄上同時跑兩個 Instance 而互相覆蓋。
6. 身為 **Instance 使用者**，如果我選擇「使用資料持久化」但該 Template 已經有持久化 Instance，我希望啟動被明確拒絕，以便我知道要先移除舊 Instance 才能再啟動。
7. 身為 **Instance 使用者**，如果我選擇「重置資料持久化」但該 Template 已經有持久化 Instance，我希望啟動被明確拒絕，以便重置不會破壞正在使用的資料。
8. 身為 **Instance 使用者**，我希望不使用持久化啟動的 Instance 數量不受限制，以便我可以隨意開多個臨時環境。
9. 身為 **Instance 使用者**，我希望停用持久化的 Template（未設定 root 目錄）即使勾選持久化也能正常啟動，以便舊 Template 的行為不被破壞。
10. 身為 **Instance 使用者**，我希望第一次使用持久化啟動時容器仍保有 Image 內建的 `.bashrc`、桌面設定、X11 / VNC / Jupyter 設定，以便容器不會黑屏或崩潰。
11. 身為 **Instance 使用者**，我希望持久化啟動後我對 home 目錄做的任何修改（新增檔案、改設定、裝套件）都會被保存下來，以便重新啟動後依然存在。
12. 身為 **Instance 使用者**，我希望「重置資料持久化」後再次啟動會回到 Image 內建的初始環境，以便我能在乾淨狀態下重新開始。
13. 身為 **Template 擁有者**，我希望在 Template 表單可以設定持久化根目錄路徑，以便定義持久化資料要放在 host 的哪裡。
14. 身為 **Template 擁有者**，我希望編輯既有 Template 時表單能回填已設定的持久化根目錄，以便確認與修改。
15. 身為 **Template 擁有者**，我希望設定的根目錄被 API 驗證（絕對路徑、無 `..`、無非法字元），以便誤設的設定在送出時就失敗而非啟動時才爆炸。
16. 身為 **Instance 使用者**，我希望重啟已停止的持久化 Instance 時自動沿用原本的 Volume 與資料目錄，以便不需要重新選擇或重新指定。
17. 身為 **Instance 使用者**，我希望 `remove` 我的持久化 Instance 時只刪除容器、route 與 DB record，**保留** host 資料目錄與 Volume 宣告，以便我之後重新啟動同一 Template 時可以直接接續之前的資料。
18. 身為 **系統管理員**，我希望持久化資料目錄由 API 依固定規則解析，而不是由使用者指定，以便沒有使用者可以把自己容器掛載到 host 任意路徑。
19. 身為 **系統管理員**，我希望 API 對解析後的路徑做正則驗證（防 `..` 穿越、防路徑注入），以便即使 Template 設定錯誤也不會越界。
20. 身為 **系統管理員**，我希望啟動失敗（例如 host 目錄建立失敗、Volume 建立失敗、權限不足）時 Instance 進入 `error` 狀態，以便我能從儀表板看到並排除。
21. 身為 **系統管理員**，我希望 host 上每個持久化 Instance 的資料路徑與 Volume 名稱是穩定的（Template 改名前的舊 Instance 仍用舊路徑與舊 Volume），以便重啟時不會指向不同的目錄。
22. 身為 **系統管理員**，我希望資料目錄的建立（mkdir / chown）與清除（rm -rf）都透過一次性 helper 容器執行，以便 API 自己不必直接存取 host 檔案系統。
23. 身為 **Instance 使用者**，我希望持久化目錄掛載在容器內使用者的整個 home（kasmvnc = `/home/kasm-user`，ttyd/jupyter = `/home/ow_user`），以便我能直覺地在自己的家目錄工作。
24. 身為 **系統管理員**，我希望在 Template Image 升級後，既有租戶的資料目錄**不會**被自動覆寫，以便使用者資料不會因升級而遺失（已知行為，需在文件說明）。

## Implementation Decisions

### 1. 路徑解析：`persistent_storage_path` 從死欄位變成真正的 root 目錄

- Template 的 `persistent_storage_path` 語意改為 **host 上的持久化根目錄**（root dir），不再需要也不接受 `{template_name}` / `{user_id}` 等 placeholder（placeholder 由 API 補齊）。表單與 API 文件同步更新語意。
- 每個 Instance 的 host 路徑由 API 以固定規則解析：

  ```
  {persistent_storage_path}/{template_name}/{owner_user_id}
  ```

- **Template 改名不影響已解析的路徑**：Instance 在 launch 時把解析好的 host 路徑寫回 `resolved_volume_host_path`（沿用既有欄位），之後 start / restart 一律使用 DB 中已存的解析結果，而不是每次重新解析 Template 名稱。因此 `template_name` 只影響該 Instance 第一次啟動時的路徑。
- launch request 移除 `resolved_volume_host_path` 輸入欄位（不再信任 client）。`mount_persistent` 保留為 boolean，由使用者在 launch 選單的選擇推導而來。
- Template 的 `persistent_storage_path` 為 `NULL` 時，持久化對該 Template 停用：任何持久化選項都退化成「不使用持久化」。

### 2. 純函式模組：路徑解析、驗證、Volume 命名、mount 目標

新增一個純函式模組（比照 `network_qos.rs`：純邏輯、無 Docker、可單元測試），負責：

- `resolve_persistent_host_path(root, template_name, owner_user_id) -> Result<String, PathError>`，驗證規則：
  - `root` 必須是絕對路徑（以 `/` 開頭）
  - 拒絕 `..` 片段（避免穿越到 root 外）
  - 拒絕空片段、拒絕以 `/` 結尾以外的非法字元組合
  - `template_name` 與 `user_id` 是資料值（template_name 來自 DB、user_id 是 UUID），但仍在組合後整體以正則重新驗證，防注入
- `persistent_volume_name(resolved_host_path) -> String`：由解析後的路徑推導出**穩定、唯一、合規**的 Docker Volume 名稱：`ow-persist-<FNV-1a 64-bit hex>`（實作採用 FNV-1a 64 位元摘要、hex 編碼，全小寫、無 `/`、長度 < 255）。名稱由 host 路徑推導，因此 Template 改名不會改變既有 Instance 的 Volume；reset 時用同一規則重算即可定位（remove 保留 Volume，不需重算）。
- `persistent_container_target(remote_type) -> &'static str`：回傳容器內 mount 目標（見解決方案 #4 的表格）。

### 3. Launch 模式選擇與「一個持久化 Instance」規則

- Launch 請求新增持久化模式（enum）：`use_persistent` / `no_persistent` / `reset_persistent`。
- **限制規則**：對同一 `(template_id, owner_id)`，若已存在任何 `mount_persistent = true` 的 Instance（**非 `error` 狀態**、未刪除），則 `use_persistent` 與 `reset_persistent` 都被拒絕（409 Conflict，含明確錯誤訊息）。`no_persistent` 不受限制；`error` 狀態的 record 是例外（見下）。
- 使用者必須先 `remove` 舊的持久化 Instance（或直接以新 launch 取代 `error` 的壞 record），才能再次使用 `use_persistent` 或 `reset_persistent` 啟動。
- 啟動失敗（helper container、Volume 建立或容器建立任一環節）時，Instance 進入 `error` 狀態（沿用現有 launch 錯誤路徑），DB record 保留。
- **取代壞掉的 Instance**：`error` 狀態的持久化 record **不觸發** 409——直接以 `use_persistent` / `reset_persistent` 重新 launch，API 會刪除舊 record、清除（wipe）既有 Volume 後重新準備，以乾淨狀態替換壞掉的實例。
- **remove 之後重用**：remove 只刪除容器與 DB record、保留資料（見 §5），因此 record 刪除後 slot 釋放，之後以 `use_persistent` 啟動同一 (Template, 使用者) 會重用被保留的 Volume 與資料。

### 4. 容器內 mount 目標（整個 home 目錄）

`create_container_from_template` 目前的寫死綁定 `/home/kasm_user/persistent` 改為依 `RemoteType` 對應的**整個 home 目錄**：

| remote_type | 容器內 mount 目標 |
|---|---|
| `kasmvnc` | `/home/kasm-user` |
| `ttyd` | `/home/ow_user` |
| `jupyter` | `/home/ow_user` |

此為寫死對應（不開放 Template 覆寫），由 `persistent_container_target` 純函式提供。

### 5. Volume 生命週期：建立、重置、移除

**建立（launch 時 `use_persistent`）**，API 依序執行：

1. **計算 host 路徑**：`resolve_persistent_host_path` 得出 `/mnt/ow_dir/{template_name}/{user_id}`，並以 `persistent_volume_name` 得出 Volume 名稱。
2. **建立乾淨的空 host 資料夾**：由於 API 是容器，沒有 host 檔案系統存取權，透過 Bollard 拉起一個一次性 helper container：

   ```
   docker run --rm \
     -v /mnt/ow_dir:/storage \
     alpine sh -c 'mkdir -p /storage/{template_name}/{user_id} && chown 1000:1000 /storage/{template_name}/{user_id}'
   ```

   - helper 以 root 執行（alpine 預設），綁定的 host 目錄在 docker daemon 所在的主機上，因此 API 不需要把 root 目錄 mount 進自己。
   - `chown 1000:1000` 讓資料夾 owner 等於容器使用者（kasmvnc `kasm-user` 與 ttyd/jupyter `ow_user` 都是 UID 1000），確保 home 頂層在複製後仍可被使用者寫入。
   - helper 執行完畢即自動銷毀（`--rm`），無需網路。
3. **建立 Named Volume**：透過 Bollard `CreateVolumeOptions` 建立 local bind 型 Volume：

   ```rust
   let mut driver_opts = HashMap::new();
   driver_opts.insert("type".to_string(), "none".to_string());
   driver_opts.insert("device".to_string(), host_path.to_string());
   driver_opts.insert("o".to_string(), "bind".to_string());
   let config = CreateVolumeOptions {
       name: volume_name.clone(),
       driver: "local".to_string(),
       driver_opts,
       ..Default::default()
   };
   docker.create_volume(config).await?;
   ```

4. **建立容器**：`create_container` 的 `HostConfig.Binds` 使用 **Volume 名稱**（而非 host 路徑）：

   ```
   vec![format!("{}:/home/kasm-user", volume_name)]   // kasmvnc
   vec![format!("{}:/home/ow_user", volume_name)]     // ttyd / jupyter
   ```

   Docker 在第一次掛載這個**全新的空 Volume** 時，自動把 Image 內該目錄的原始檔案複製（Populate）進 Volume——因此保留內建環境。

> **重複啟動（remove 後重用）**：`prepare_persistent_volume` 是冪等的——若 Volume 宣告已存在（例如先前的 Instance 被 remove 但資料保留），直接重用既有 Volume 與資料，不執行 helper、不清除、不重新 populate。

**重置（launch 時 `reset_persistent`）**，API 依序執行：

1. 以 helper container 清除既有 host 資料目錄（`rm -rf /storage/{template_name}/{user_id}`）。
2. 呼叫 `docker.remove_volume(volume_name)` 刪除舊 Volume 宣告（若存在），否則下次建立同名 Volume 時 Docker 會誤判為舊 Volume 而跳過自動複製。
3. 再走 `use_persistent` 建立流程（mkdir + chown → create_volume → create_container）。

**移除（remove Instance）**，API 依序執行：

1. 停止並移除容器、刪除 route 檔、清理 DB record（沿用現有 remove 流程）。
2. **保留** host 資料目錄與 Volume 宣告——remove 只銷毀實例，不銷毀資料。之後以 `use_persistent` 重新啟動同一 (Template, 使用者) 時，`prepare_persistent_volume` 偵測到 Volume 已存在就直接重用（不清除、不重新 populate），資料接續先前內容。只有 `reset_persistent` 才會清空資料。

**重啟（start / restart 已停止的持久化 Instance）**：直接沿用 DB 中已存的 `resolved_volume_host_path` 與對應 Volume 名稱，不重新解析。若 Volume 宣告遺失，先以 `ensure_persistent_volume` 重新以 `create_volume` 補建（不重新 populate）。對 legacy 實例（`mount_persistent = true` 但 `resolved_volume_host_path` 為空），先以現行解析規則補填並持久化該路徑，再補建 Volume 與掛載。

### 6. 前端

- 整個 dashboard 為**英文介面**；所有持久化相關字串均為英文。
- Template 表單的「持久化路徑」欄位語意改為「持久化根目錄」（`Persistent Root Directory`），placeholder/hint 為 `/data/persistent`，不再標示 `{template_name}` / `{user_id}` 變數（root 目錄僅一層，其餘由 API 補齊）；create + edit 皆回填。
- Launch 選單（「當前分頁 / 新分頁」下拉旁）新增「資料持久化」（`Data Persistence`）下拉，**僅在 Template 設定了 `persistent_storage_path` 時顯示**（`showPersistenceSelect`）；未設定的 Template 一律以 `no_persistent` 啟動。
- 選項由上而下：`Use persistent storage`（**預設**）→ `No persistent storage` → `Reset persistent storage`（最下方）。選到 reset 時跳出 `window.confirm` 警告（「會清除既有資料、以全新環境啟動」），取消則還原為先前選項。
- 由該選擇推導送出 `persistence` 與 `mount_persistent`（`use_persistent` / `reset_persistent` → true）；不送出任何 client host path。
- 持久化 Instance 卡片顯示 `persist` badge，方便使用者辨識哪個 Instance 佔用持久化名額。

## Testing Decisions

### 測試原則

只測試外部行為。核心是**路徑解析、驗證、Volume 命名與 mount 目標的純函式**（不依賴 Docker），輔以既有 route 測試模式覆蓋「一個持久化 Instance」拒絕規則與 helper/Volume 呼叫順序。**Volume 的自動複製（Populate）是 Docker daemon 的行為**，不是我們要測試的邏輯——以真實 Docker 的整合測試驗證一次即可，不寫 mock。

### 主要 seam：純函式模組（單一主要 seam）

比照 `network_qos.rs` 的測試模式（`apps/api/tests/docker_test.rs` 內有純邏輯單元測試、不啟動容器）：

- 給定 `persistent_storage_path = "/mnt/ow_dir"`、`template_name`、`owner_user_id` → 正確組出 `/mnt/ow_dir/{template_name}/{user_id}`
- root 為相對路徑 → 拒絕
- root 含 `..` 片段 → 拒絕
- template_name / user_id 含注入字元 → 拒絕
- `persistent_storage_path = NULL` → 解析失敗／停用
- `persistent_volume_name`：同一 host 路徑 → 相同名稱；不同路徑 → 不同名稱；名稱不含 `/`、全小寫、長度 < 255
- `persistent_container_target`：kasmvnc → `/home/kasm-user`，ttyd/jupyter → `/home/ow_user`
- Template 改名後，已解析路徑與 Volume 名稱不變（以 DB 的 `resolved_volume_host_path` 為準）

### 次級 seam：route 層級（instances_mock_test.rs 模式）

沿用 `apps/api/tests/instances_mock_test.rs`（mockall `DockerService` + Postgres TestContext）：

- 同一 (template, user) 已有 `mount_persistent = true` 的 Instance，再以 `use_persistent` 或 `reset_persistent` launch → 409
- `no_persistent` 不受此限制 → 200
- launch request 傳入 `resolved_volume_host_path` → 被忽略（不回傳、不影響）
- `use_persistent` 成功 → Instance `mount_persistent = true` 且 `resolved_volume_host_path = {root}/{template_name}/{user_id}`
- 啟動順序（以 mock 驗證）：helper container（mkdir + chown）→ `create_volume` → `create_container`（`Binds` 用 Volume 名稱，mount 目標依 remote_type）
- helper container / `create_volume` / `create_container` 任一失敗 → Instance 進入 `error` 狀態（DB record 保留）
- reset_persistent 成功 → 先 `remove_volume` + helper 清除，再重新建立（以 mock 驗證呼叫順序）
- remove → `remove_persistent_volume` **不被呼叫**，Volume 宣告與 host 資料目錄保留（以 mock 驗證 `.never()`）；之後 `use_persistent` 重啟重用既有 Volume

### 真實 Docker 整合測試（docker_test.rs，`--features docker`）

沿用 `apps/api/tests/docker_test.rs` 模式（需真實 Docker，不 mock）：

- 建立空 host 目錄 → local bind Named Volume → 啟動容器掛載 → 驗證 Image 內建檔案存在（Populate 生效）
- 同一個 Volume 重啟容器 → 驗證先前寫入的檔案保留
- 驗證「Volume 已非空時掛載不會重新複製」的行為（確認 reset 才會回到初始環境）

### 前端（vitest）

沿用 `apps/web/src/tests/template-form.test.ts`、`template-panel.test.ts` 模式：

- launch 選單的持久化選擇與 `mount_persistent` 送出的對應關係
- Template 表單「持久化根目錄」欄位的回填與送出

## Out of Scope

- Host 資料目錄的 garbage collection / 空間配額（Template 改名後留下的舊目錄屬於已知行為，不做自動清理）
- 多節點 / 叢集環境的持久化（local bind volume 不具跨主機可移植性）
- 將持久化目錄備份、匯出、下載給使用者
- Template Image 升級後將預設檔案同步進既有租戶 home（屬已知限制）
- 持久化 Instance 之間的合作編輯（同一資料多實例同步）
- 對 root 目錄的定期權限稽核

## 補充說明

- **Volume 自動複製的條件**：Docker 只在 Volume **第一次建立且內容完全為空**時觸發複製。host 目錄若有殘留隱藏檔（例如 `.DS_Store`）就會被視為非空而跳過。因此 launch 的 helper 容器必須確保目錄是全新且乾淨的；「回到初始環境」的唯一途徑是 reset（清除目錄 + 刪除 Volume 宣告後重建）。
- **remove 會保留資料**：此設計中 `remove` Instance 只刪除容器、route 與 DB record，**保留** host 資料目錄與 Volume 宣告（U-17），資料可被下一次 `use_persistent` 啟動重用。只有 `reset_persistent`（或替換 broken `error` 記錄）才會清空資料。因此不會有「remove 誤刪使用者資料」的情況。
- **Image 升級情境**：Image 內建檔案只在首次啟動複製一次；Template Image 升級後，既有租戶的 home 不會自動收到更新檔案（屬持久化機制的正常現象，需在文件說明）。
- **API 容器的 host 存取**：API 自己不需要、也不應 mount host 的持久化 root 目錄——所有 host 檔案系統操作（mkdir / chown / rm -rf）都透過一次性 alpine helper container 在 docker daemon 所在的主機上執行。此設計也維持「API 無法直接讀寫 host 任意路徑」的隔離。
- **Migration**：既有資料沿用現有欄位（`mount_persistent`、`resolved_volume_host_path`），不新增 DB 欄位；`persistent_storage_path` 語意改變屬文件層級，不需 schema 變更。既有 `running` 且 `mount_persistent = true` 的 Instance 其 `resolved_volume_host_path` 若原本為空，首次重啟時需以「目前解析規則 + 已存資料」補上，避免重啟後失去掛載。
- **相容性**：client 端既有會送 `resolved_volume_host_path` 的呼叫（若有）將被忽略，不回傳錯誤，避免破壞現有整合。
