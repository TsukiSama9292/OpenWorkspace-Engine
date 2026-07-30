## 一、問題陳述

目前平台建立的工作容器完全交由 Docker daemon 決定使用哪個 OCI runtime（預設為 `runc`），管理員無法指定容器應在更強的沙箱隔離環境中執行。對於需要更高安全性的工作負載（例如處理不可信的程式碼、多租戶共用同一台機器），現有架構無法在不 fork 基礎設施的前提下啟用 gVisor（`runsc`）或其他 runtime。runtime 在環境變數、資料庫、API、前端 UI 四個層級皆不可見。

## 二、解決方案

在 `workspace_templates` 新增 `container_runtime` 欄位，由 `OW_CONTAINER_RUNTIME` 環境變數提供系統預設值。Template 可以指定 `"docker"`（不指定 runtime，使用 Docker daemon 預設）或 `"runsc"`（啟用 gVisor）。前端在進階表單區提供下拉選單，Rust API 將該值傳入 Docker 的 `HostConfig.runtime`。

## 三、使用者故事

1. 身為**基礎架構管理員**，我希望在 API Server 設定 `OW_CONTAINER_RUNTIME=runsc`，讓所有 workspace 容器預設使用 gVisor，不需逐一修改 template
2. 身為**工作區管理員**，我希望在特定 Template 將 `container_runtime` 改為 `"runsc"`，讓高敏感度工作負載獲得額外沙箱隔離，即使系統預設為 `"docker"`
3. 身為**工作區管理員**，我希望下拉選單只顯示「Default」與「runsc」兩個選項，不需要知道 Docker runtime 的實際名稱
4. 身為**工作區管理員**，我希望 `container_runtime` 放在進階表單區塊（Advanced），避免主表單過於擁擠
5. 身為**工作區管理員**，我希望在建立 Template 時可以設定 `container_runtime`，建立後也可以編輯修改
6. 身為 **API 使用者**，我希望每個 Template 的 JSON 回應都包含 `container_runtime`，以便統一稽核所有 Template 的 runtime 設定
7. 身為 **API 使用者**，我希望建立或更新 Template 時可以傳入 `container_runtime`，以便程式化管理 runtime
8. 身為**平台開發者**，我希望 runtime 對應到 `HostConfig.runtime` 的邏輯是一個純函數，不需 Docker daemon 或資料庫即可測試
9. 身為**平台開發者**，我希望環境變數的解析方式與現有 `Settings` 欄位一致，保持程式碼風格統一

## 四、實作決策

### 4.1 環境變數

- 名稱：`OW_CONTAINER_RUNTIME`
- 預設值：`"docker"`
- 在 `Settings::from_env()` 中解析，與 `DOCKER_NETWORK` 同等級
- 值為 `"docker"` 表示「不設定 `HostConfig.runtime`，讓 Docker daemon 使用其預設值」

### 4.2 Schema 變更（Migration `000009`）

**workspace_templates** 新增欄位：
- `container_runtime VARCHAR(64) NOT NULL DEFAULT 'docker'`

此欄位為第一-class 的 template 屬性，**不**放在 `run_config` JSON blob 中。

### 4.3 Rust 資料模型變更

**`ContainerConfig` struct** 新增欄位：
```
pub runtime: Option<String>,
```
- `None` → 使用 `"docker"` 行為（省略 `HostConfig.runtime`）
- `Some("runsc")` → 設 `HostConfig.runtime` 為 `"runsc"`
- `Some("docker")` → 省略 `HostConfig.runtime`（明確指定使用 daemon 預設，與「繼承環境變數」不同）

**`WorkspaceTemplate`（public type）** 新增欄位：
```
pub container_runtime: String,
```
預設值 `"docker"`，從 SeaORM entity 對應。

**`Settings` struct** 新增欄位：
```
pub container_runtime: String,
```
預設值 `"docker"`，從 `OW_CONTAINER_RUNTIME` 讀取。

### 4.4 API 合約

- `POST /api/templates` — 接受選填 `container_runtime`（省略時預設 `"docker"`）
- `PUT /api/templates/{id}` — 接受 `container_runtime`
- `GET /api/templates` 與 `GET /api/templates/{id}` — 回應包含 `"container_runtime": "docker"|"runsc"`

### 4.5 Runtime 解析邏輯

```
template.container_runtime
  if set and non-empty  →  直接使用
  if empty/unset        →  使用 settings.container_runtime（環境變數）
```

呼叫端（`instances.rs`）在建構 `ContainerConfig` 時解析：
```
container_config.runtime = if template.container_runtime.is_empty() {
    Some(state.settings.container_runtime.clone())
} else {
    Some(template.container_runtime.clone())
}
```

`create_container_from_template` 內轉換為 Docker API：
```
host_config.runtime = match config.runtime.as_deref() {
    None | Some("docker") => None,      // 省略 → daemon 預設
    Some(other) => Some(other.to_string()),
}
```

### 4.6 純函數輔助

```
fn runtime_to_host_config(value: &str) -> Option<String>
```
封裝 `"docker"/空字串 → None`、`"runsc" → Some("runsc")` 的對應邏輯。位於 `docker.rs`。

### 4.7 前端變更

- `TemplateFormState` 新增 `containerRuntime: string`（預設 `""`）
- 下拉選單兩個選項：「Default」（值 `""`）與「runsc」（值 `"runsc"`）
- 放在進階表單區塊（Advanced），大約在 `networkMode` 附近
- `submitTemplate()` 將 `container_runtime` 以頂層欄位送出（不在 `run_config` 內）
- `Template` TypeScript type 新增 `container_runtime: string`

## 五、測試決策

### 5.1 測試原則

只測試外部行為，不測試實作細節。重點測試：
- runtime 字串對應到 `HostConfig.runtime` 的純函數轉換
- API 正確暴露 `container_runtime` 欄位
- 資料庫正確儲存與讀回該欄位

### 5.2 測試接縫（由高至低）

| 順位 | 位置 | 類型 | 測試內容 |
|---|---|---|---|
| 1 | `docker.rs` (`#[cfg(test)]`) | 純函數單元測試 | `runtime_to_host_config("docker") → None`、`runtime_to_host_config("runsc") → Some("runsc")`、`runtime_to_host_config("") → None` |
| 2 | `core/settings.rs` (`#[cfg(test)]`) | Settings 單元測試 | `OW_CONTAINER_RUNTIME` 預設值 `"docker"`、自訂值解析、缺欄位時使用預設值 |
| 3 | `db_test.rs`（整合測試） | Repository 測試 | `WorkspaceTemplateRepository::create()` 含 `container_runtime`、`update()` 更新值、`find_by_id()` 回傳正確 |
| 4 | `db_test.rs`（整合測試） | Entity From 實作測試 | `workspace_template::Model → WorkspaceTemplate` 轉換包含 `container_runtime`，測試 `Some("runsc")` 與 `None` 兩種情況 |
| 5 | `templates_test.rs`（整合測試） | API 整合測試 | `POST /api/templates` 回應 JSON 包含 `container_runtime`、`GET /api/templates/{id}` 包含、`PUT /api/templates/{id}` 可更新 |

### 5.3 既有測試參考

- Settings 環境變數測試：`settings.rs` lines 59–230
- Entity `From` 實作測試：`db_test.rs` lines 679–806（`config_model_from_converts_all_fields`、`config_model_from_null_optionals`）
- Repository 測試：`db_test.rs` lines 147–321（create、find、update、delete template）
- API template 測試：`templates_test.rs`

## 六、不在此限

- **Per-instance runtime 覆寫**：runtime 為 template 層級屬性，instance 繼承 template 設定，不可單獨覆寫
- **對 Docker daemon 做 runtime 預先驗證**：若 Docker daemon 未註冊 `runsc`，Docker 本身會拒絕建立容器，API 層不做預檢
- **`docker_raw.rs` 的 runtime 支援**：`POST /api/docker/containers/create` 為純除錯用途的原始 proxy，不納入本變更
- **`runsc` 以外的 runtime 支援**（如 `kata`、`nvidia`）：下拉選單僅提供「Default」與「runsc」，但環境變數與 API 接受任意字串，可透過直接呼叫 API 使用其他 runtime
- **kasmvnc.yaml 的 `runtime_configuration` 區段**：KasmVNC YAML 內的 `runtime_configuration` 是給 VNC Server 用的，與 Docker runtime 無關

## 七、補充說明

`"docker"` 這個值在本功能中表示「不指定任何 Docker runtime，交給 Docker daemon 決定」。雖然 Docker 的預設 OCI runtime 名稱是 `runc`，但使用 `"docker"` 一詞的原因是：(a) 使用者要求此命名、(b) 語意直覺對應「標準 Docker 行為，無沙箱繞道」、(c) Docker daemon 管理員可在 `/etc/docker/daemon.json` 設定不同的 `default-runtime`，系統不與之衝突。
