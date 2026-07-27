## 一、 整個問題的本質 (The Root Problem)

希望實現 **「使用者只需在網頁登入一次（SSO），就能存取後端多個 KasmVNC 桌面」**。

在這個架構下，產生了三個互相衝突的核心矛盾：

1. **單一簽入 (SSO) vs. 雙重登入**：為了不讓使用者登入 Traefik 後，又被 KasmVNC 彈出視窗要求輸入第二次密碼，直覺的做法是**關閉 KasmVNC 的內建驗證**（例如使用 `-disableBasicAuth`）。
2. **Docker 預設網路的橫向連通性 (Lateral Movement)**：在 Docker 的預設 Bridge 網路中，同網段內的所有容器是**預設互通**的。
3. **安全真空**：當你把 KasmVNC 的密碼關掉，代表它陷入「裸奔」狀態。此時若有惡意使用者登入了 Container A，他不需要經過 Traefik，就能直接在內部網路連線到 Container B 的 `6901` 埠，**無痛操控別人的桌面**。

---

## 二、 錯誤 / 有瑕疵的解決方案 (Flawed Solutions)

在討論過程中，有幾種看似合理但實際上不可行或有安全漏洞的作法：

| 方案 | 做法 | 為什麼有瑕疵 / 失敗原因 |
| --- | --- | --- |
| **瑕疵 1：完全關閉驗證** | KasmVNC 設定 `-disableBasicAuth -SecurityTypes None` | **放棄應用層防禦**。容器之間只要網路通，就能隨意連線到對方的 KasmVNC，產生嚴重的橫向移動漏洞。 |
| **瑕疵 2：監聽 Localhost** | KasmVNC 設定 `-interface 127.0.0.1` 或 `-localhost` | **阻斷代理連線**。Docker 容器各自擁有獨立的網路命名空間（Network Namespace），KasmVNC 聽自己的 `127.0.0.1`，位在另一個容器的 Traefik 會完全連不進來。 |
| **瑕疵 3：共享 Traefik 網路** | 容器設定 `network_mode: "service:traefik"` | **造成通訊埠（Port）衝突**。多個 KasmVNC 容器同時綁定 Traefik 的 `6901` 埠會導致容器啟動失敗（Crash）；且容器間依然能透過 `localhost:6901` 互相存取。 |

---

## 三、 正確的解決方案 (The Correct Solutions)

要兼顧 **「使用者免輸入二次密碼」** 與 **「容器間無法橫向攻擊」**，主要有兩種正確的架構改進方向：

### 最佳解：Traefik 自動注入 Basic Auth 標頭 (Header Injection)

這是最標準的零信任實作方式。**不要關閉 KasmVNC 的認證，而是讓 Traefik 替使用者代填密碼。**

* **運作機制**：
1. KasmVNC 保留密碼驗證，並設定一組內部亂數強密碼，並且在容器啟動的時候用 `-e VNC_PW=password` 注入強密碼。
2. 使用者向 Traefik 進行身份驗證（OAuth2 / OIDC / Basic Auth）。
3. Traefik 驗證通過後，在轉發請求給 KasmVNC 時，透過 Middleware **自動附加 `Authorization: Basic <base64>` 標頭**。
4. KasmVNC 收到請求，比對標頭正確，順利放行。


* **安全效益**：
* **外部使用者**：感知不到 KasmVNC 密碼的存在，達到 SSO 體驗。
* **內部容器**：KasmVNC 依然保有門鎖。即使 Container A 嘗試連線 Container B，因為沒有 Traefik 注入的認證標頭，會被 KasmVNC 直接回絕（`401 Unauthorized`）。


* **Docker 設定 keypoint**：Traefik 與 KasmVNC 放在同一個普通 Bridge 網路，Traefik 透過容器名稱與內部 Port 代理（如 `http://kasmvnc1:6901`）。

---

### 備選 / 雙重防護解：網路層微隔離 (Micro-segmentation)

如果不希望在 Traefik 設定認證標頭，就必須**從 Docker 網路層直接切斷容器之間的連線**。

* **運作機制**：
1. 為每一個 KasmVNC 容器建立獨立的 Docker Bridge 網路（例如 `net_kasm1` 與 `net_kasm2`）。
2. Traefik 同時加入 `net_kasm1` 與 `net_kasm2`。
3. `kasmvnc1` 屬於 `net_kasm1`，`kasmvnc2` 屬於 `net_kasm2`。


* **安全效益**：
* 在 Layer 3 網路層上，`kasmvnc1` 與 `kasmvnc2` 處於完全封閉的獨立網段，封包根本無法到達對方，從物理（邏輯）層面上杜絕了橫向移動。



---

## 總結對照

| 比較項目 | 裸奔方案 (錯) | Traefik Header 注入 (最佳解) | 網路層微隔離 (備選解) |
| --- | --- | --- | --- |
| **外部使用者體驗** | 免二開密碼 (SSO) | **免二開密碼 (SSO)** | **免二開密碼 (SSO)** |
| **KasmVNC 驗證** | 關閉 (`-disableBasicAuth`) | **開啟 (長密碼)** | 關閉 (`-disableBasicAuth`) |
| **容器間橫向攻擊** | ⚠️ 高風險（無阻擋） | **無風險 (擋在 401 驗證)** | **無風險 (L3 網路無法連通)** |
| **維護複雜度** | 低（但極不安全） | **中（設定檔加 Middleware 標頭）** | 中（需建立多個 Docker 網路） |

> **最終建議：** 採用 **「Traefik Header 注入」** 是維護性最好、最符合現代雲原生（Cloud-Native）安全規範的標準做法。