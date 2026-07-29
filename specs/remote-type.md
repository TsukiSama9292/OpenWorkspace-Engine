## 一、問題陳述

目前系統只能啟動 KasmVNC 桌面容器，無法支援其他遠端存取類型。使用者需要能透過同一平台啟動網頁終端機（ttyd）或 Jupyter Lab 等不同型態的工作環境。現有資料庫與容器建立邏輯都將「VNC」寫死——欄位名稱（`vnc_token`、`vnc_password`）、環境變數（`KASM_VNC_PORT=6901`）、Traefik 路由產出，全都無法擴展到非 VNC 類型。

## 二、解決方案

在 `workspace_templates` 新增 `remote_type` 欄位，值為 `kasmvnc` / `ttyd` / `jupyter`，決定容器啟動時的環境變數、暴露埠號、以及 Traefik 路由規則。Instance 層級的 `vnc_token` / `vnc_password` 改名為 `access_token` / `access_password`，供所有類型共用。

## 三、使用者故事

1. 身為管理者，我希望在建立 Template 時可以選擇遠端類型（KasmVNC / ttyd / Jupyter Lab），以便建立不同用途的開發環境
2. 身為管理者，我希望既有 Template 若未指定 remote_type 則預設為 `kasmvnc`，以避免向後相容問題
3. 身為使用者，我希望啟動 ttyd 類型的 Instance 時，容器內自動帶入 `TTYD_USERNAME=ow_user` 與 `TTYD_PASSWORD=` 環境變數，以便 ttyd 可以直接讀取完成 Basic Auth 設定
4. 身為使用者，我希望啟動 Jupyter Lab 類型的 Instance 時，容器內自動帶入 `JUPYTER_PASSWORD=` 環境變數，以便 Jupyter 可以直接讀取完成密碼驗證
5. 身為使用者，我希望啟動 KasmVNC 類型的 Instance 時，保留現有行為（`KASM_VNC_PORT=6901`、`DISPLAY=:1`、`VNC_PW=`、注入 kasmvnc.yaml）
6. 身為使用者，我希望 Instance 的 routing token 透過 `/vnc/{token}/`（KasmVNC）、`/ttyd/{token}/`（ttyd）、`/jupyter/{token}/`（Jupyter Lab）路徑存取，以便每個類型有獨立的 router 前綴
7. 身為管理者，我希望 Traefik 自動為不同 remote_type 產生對應的 routing YAML，以便使用者不需手動設定代理規則
8. 身為使用者，我希望 KasmVNC Instance 的 WebSocket 路由保持 `https://{ip}:6901`（`kasm-insecure` transport），以便保留加密連線
9. 身為使用者，我希望 ttyd Instance 的路由指向 `http://{ip}:7681`，以便瀏覽器直接以 HTTP 連線到網頁終端機
10. 身為使用者，我希望 Jupyter Lab Instance 的路由指向 `http://{ip}:8888`，以便瀏覽器直接以 HTTP 連線到 Jupyter
11. 身為管理者，我希望 Instance CRUD API 的回傳中包含 `access_token` 與 `access_password`（取代 `vnc_token` / `vnc_password`），以便前端泛化處理
12. 身為開發者，我希望透過 `access_token` 可反向查詢對應的 `remote_type`，以便決定前端連線時的 UI 類型（VNC Viewer / ttyd iframe / Jupyter iframe）
13. 身為管理者，我希望刪除 Instance 時一併清除對應的 Traefik 路由檔案，避免殘留

## 四、實作決策

### 4.1 Schema 變更（Migration `000008`）

**workspace_templates** 新增欄位：
- `remote_type VARCHAR(32) NOT NULL DEFAULT 'kasmvnc'`

**workspace_instances** 欄位改名（僅 SQL 層 rename，不影響資料）：
- `vnc_token` → `access_token`
- `vnc_password` → `access_password`

### 4.2 Sea-ORM Entity 變更

- `workspace_template::Model` 新增 `remote_type` 欄位
- `workspace_instance::Model`: `vnc_token` → `access_token`、`vnc_password` → `access_password`
- 對應的 `WorkspaceTemplate` / `WorkspaceInstance` public struct 同步更新

### 4.3 Container 建立邏輯 (`docker.rs`)

`create_container_from_template` 新增 `remote_type` 參數，根據值決定行為：

| remote_type | env vars | exposed ports | 額外行為 |
|---|---|---|---|
| `kasmvnc` | `KASM_VNC_PORT=6901`, `DISPLAY=:1`, `VNC_PW={password}` | `6901/tcp` | 注入 kasmvnc.yaml |
| `ttyd` | `TTYD_USERNAME=ow_user`, `TTYD_PASSWORD={password}` | `7681/tcp` | 無 |
| `jupyter` | `JUPYTER_PASSWORD={password}` | `8888/tcp` | 無 |

不覆寫 container 的 CMD/entrypoint。由 image 本身的啟動腳本讀取 env 來啟動對應服務。

### 4.4 Route Writer (`vnc_trafik.rs` → `route_writer.rs`)

根據 `remote_type` 產生不同的 Traefik YAML 檔案：

| remote_type | 檔名 | PathPrefix | target | transport |
|---|---|---|---|---|
| `kasmvnc` | `vnc-{token}-ws.yml` | `/vnc/{token}/websockify` | `https://{ip}:6901` | `kasm-insecure` |
| `ttyd` | `ttyd-{token}-ws.yml` | `/ttyd/{token}/` | `http://{ip}:7681` | 預設 |
| `jupyter` | `jupyter-{token}-ws.yml` | `/jupyter/{token}/` | `http://{ip}:8888` | 預設 |

KasmVNC 需要保留 `kasm-insecure` transport（SSL verify off），ttyd 和 Jupyter 使用一般 HTTP 即可。

### 4.5 Template Create/Update API

- `CreateTemplateRequest` 與 `UpdateTemplateRequest` 新增 `remote_type` 欄位，預設 `kasmvnc`
- JSON response 中的 template 物件新增 `remote_type`
- Template 列表/查詢 API 回傳中納入 `remote_type`

### 4.6 Instance Launch API

- `launch_instance` 讀取 template 的 `remote_type`
- 建立 container 時傳入 `remote_type`
- Container 啟動後，呼叫 route writer 產生對應類型的路由

### 4.7 前端變更

- `TemplateBasics.svelte` 新增 remote_type 下拉選單，選項：KasmVNC、ttyd、Jupyter Lab
- `TemplateFormState` 新增 `remoteType` 欄位
- `Template` type (`types.ts`) 新增 `remote_type`
- Instance type 的 `vnc_token` / `vnc_password` → `access_token` / `access_password`
- 使用者點擊 Instance 時，根據 template 的 `remote_type` 決定開啟 VNC Viewer（kasmvnc）或 iframe 嵌入（ttyd / jupyter）

## 五、測試決策

### 5.1 測試原則

只測試外部行為，不測試實作細節。對於 route writer，測試產出的 YAML 內容是否正確；對於 container config，測試 env vars 和 ports 組合是否正確；對於 repository，測試 remote_type 是否能正確寫入與讀回。

### 5.2 Route Writer 測試（最高接縫）

現有 `vnc_trafik.rs` 已有 13 個測試，涵蓋 YAML 寫入、內容驗證、刪除、邊界案例。重構成 `route_writer.rs` 後擴展：

- `kasmvnc` 產生 `vnc-{token}-ws.yml`，內容含 `https://{ip}:6901`、`kasm-insecure`、`PathPrefix(/vnc/{token}/websockify)`
- `ttyd` 產生 `ttyd-{token}-ws.yml`，內容含 `http://{ip}:7681`、`PathPrefix(/ttyd/{token}/)`
- `jupyter` 產生 `jupyter-{token}-ws.yml`，內容含 `http://{ip}:8888`、`PathPrefix(/jupyter/{token}/)`
- 不同類型獨立檔案、獨立刪除

### 5.3 Repository 測試

現有 `db_test.rs` 測試 `WorkspaceTemplateRepository` 的 create/find/list/update/delete。新增：

- 建立 template 時指定 remote_type，查回後值正確
- 未指定 remote_type 時預設為 `kasmvnc`
- 更新 remote_type 後值正確

### 5.4 Container Config 測試

透過 `DockerService` trait 的 `mockall` mock，測試 `create_container_from_template`：

- `kasmvnc` 的 env 包含 `KASM_VNC_PORT=6901` / `VNC_PW=`，exposed ports 包含 `6901/tcp`
- `ttyd` 的 env 包含 `TTYD_USERNAME=ow_user` / `TTYD_PASSWORD=`，exposed ports 包含 `7681/tcp`
- `jupyter` 的 env 包含 `JUPYTER_PASSWORD=`，exposed ports 包含 `8888/tcp`

## 六、不在此限

- 前端 ttyd / Jupyter 的具體 iframe 嵌入實作（僅定義 API 回傳欄位，UI 層另案處理）
- ttyd / Jupyter 對應的 Docker image 管理（假設使用者的 image 已經內建啟動腳本）
- Traefik 的 SSL 憑證設定（沿用現有 `kasm-insecure` + HTTP 混合）
- OAuth2 / OIDC 整合（維持現有驗證機制）
- Instance 的密碼輪替或管理功能

## 七、補充說明

Instance `access_password` 的生成方式沿用現有 KasmVNC 的亂數密碼生成邏輯，三種 remote_type 共用同一機制。`TTYD_USERNAME` 固定為 `ow_user`，不可由使用者自訂。
