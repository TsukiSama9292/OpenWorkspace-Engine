# OpenWorkspace Engine — 使命文件（Mission）

> 本文件是專案的「憲法」第一條：闡明我們**為何**存在、要**做什麼**、以及**不做什麼**。
> 任何功能開發、架構決策與取捨，都應能追溯回本文件的某一個價值支柱。

---

## 一句話定位

> **把任何一台閒置的 Linux 伺服器，變成一個多人共享、瀏覽器直連、零基礎安裝的雲端開發環境。**

OpenWorkspace Engine 是輕量級的容器調度平台：按需布建隔離的 Linux 工作環境（KasmVNC 桌面、Jupyter Lab、ttyd 終端），透過 Traefik 反向代理以瀏覽器存取，具備 JWT 驗證、自動休眠、閒置回收、頻寬控管與持久化使用者資料。

---

## 為什麼要建立此專案（The Why）

### 現實痛點

| 痛點 | 影響 |
|---|---|
| **硬體通膨** | DRAM/GPU 價格漲幅遠超預算，實驗室與中小企業無法汰換硬體 |
| **資源浪費** | 2–5 年的舊伺服器（多核 CPU + GB 級記憶體）閒置率超過 90% |
| **環境混亂** | 在主機直接安裝 CUDA/Python 造成驅動衝突、系統崩潰 |
| **資源獨佔** | 一人獨佔整台機器，離峰時間 90%+ 的 CPU/RAM 空轉 |

### 我們的信念

1. **務實的現實主義** — 讓老硬體重獲新生，解決學術與小型團隊的硬體焦慮。
2. **務實的永續性** — 以軟體優化換取最高利用率，而不是無限堆疊硬體。
3. **不妥協的開發體驗** — 硬體也許老舊，但開發體驗必須現代、流暢、開箱即用。

### 為什麼用「瀏覽器」當入口

零客戶端安裝、免 VPN、免 SSH 設定——只要瀏覽器開著就能進入完整 GUI 桌面。
這大幅降低了「把舊機器重新投入使用」的心理與技術門檻。

---

## 目標（The What）

我們要打造的是一個**多租戶、單主機優先、零信任隔離**的容器工作區平台：

1. **hyper-efficiency（高效率）** — Docker 容器取代 15–20% 的 VM 開銷；8GB RAM 的主機可以同時跑多個隔離環境。
2. **動態分配（Dynamic Allocation）** — 需要時建立、閒置時自動停止；硬體永遠不會被空占。
3. **瀏覽器即入口（Browser-as-Entry）** — 零客戶端、免 VPN，完整 GUI 桌面就在瀏覽器裡。
4. **零信任隔離（Zero-Trust Isolation）** — 容器隔離 + JWT 驗證 + 每實例獨立存取憑證 + 每實例獨立 `/30` 網段 + cgroups 資源限制。

### 名詞定義

| 概念 | 定義 |
|---|---|
| **Template（範本）** | 預設設定套件（image、資源、環境變數），使用者由此啟動實例 |
| **Instance（實例）** | 由範本啟動的執行中容器（KasmVNC / ttyd / Jupyter） |
| **User（使用者）** | 擁有帳號的人；授權採**群組制**——權限（旗標、範本白名單、實例額度）落在群組上，每次請求重新解析為 effective context |

---

## 核心功能

### 支援的介面
- **Desktop（KasmVNC）** — 瀏覽器中的完整 Linux GUI 桌面（HTML5 Canvas + WebSocket）。
- **Jupyter Lab** — 資料科學環境，預裝 Python kernel。
- **Terminal（ttyd）** — 輕量瀏覽器終端，快速 CLI 存取。

### 安全與隔離
- **跨租戶隔離** — 每個實例配發獨立的 127 字元隨機存取權杖；所有流量必須經 Traefik 代理並攜帶有效權杖才能到達容器。
- **gVisor 沙箱** — 範本層可選擇 `runsc` runtime，攔截高風險 syscall 保護主機。
- **GPU 透傳（NVProxy）** — gVisor 的 `--nvproxy` 將 NVIDIA ioctl 從沙箱代理到主機驅動（Turing/Ampere/Ada/Hopper）。
- **JWT Cookie 驗證** — `ow_token` cookie + Traefik ForwardAuth 驗證 WebSocket upgrade。
- **無頭實例驗證** — 代理伺服器端注入憑證（KasmVNC/Jupyter/ttyd），瀏覽器永看不到密鑰。
- **群組式 RBAC** — 權限由群組旗標 + 範本白名單 + 實例額度構成，每次請求自 DB 重算（無過期權杖問題）。

### 資源治理
- **Auto-Sleep（執行時限）** — 每範本 `max_run_seconds`，超過即執行 `timeout_action`（remove/stop/pause）；前端倒數警示。
- **Keep Time（閒置回收）** — 瀏覽器分頁開啟、可見且聚焦時每 10 秒心跳；閒置超過 `keep_time_seconds` 即回收。
- **頻寬控管** — 每範本上/下行 Mbps 上限，以核心 `tc`/HTB 在實例 veth 上執行。
- **實例額度（ceiling）** — 群組 `max_instances` + 使用者個人 `direct_max_instances` 共同構成 effective ceiling。

### 持久化使用者資料
- **整個 home 目錄持久化** — 停止、重啟、刪除後資料仍在；首次掛載自動填入影像內建的環境設定。
- **伺服器端解析路徑** — 主機路徑由 API 解析驗證為 `{root}/{template_name}/{user_id}`；客戶端永不提供路徑。
- **三種啟動模式** — 使用持久化 / 不使用（暫時性）/ 重設（清空重來，前端有確認警示）。
- **一範本一持久實例** — 同（範本, 擁有者）的第二次持久啟動回 409，直到舊實例移除。

### 管理介面
- **單頁儀表板** — 實例卡片、範本編輯、Sessions、Volumes、Groups、Users、Settings 全部集中一頁。
- **群組與使用者管理** — 建立帳號、指派群組成員、設定個人額度。
- **範本可見性** — `public` / `private` / `hidden` 三態 + 群組白名單。

---

## 設計哲學：Security · Stability · Performance

我們追求三者之間的**平衡最優**——任何一項都不以犧牲其他兩項為代價：

| 層 | 技術 | 我們換到什麼 |
|---|---|---|
| 控制平面 API | Rust | **Security + Performance**——記憶體安全 + 零成本抽象；<35MB RAM、高併發非阻塞 I/O |
| 前端 | SvelteKit | **Performance + DX**——輕量靜態 SPA，又不犧牲開發便利 |
| 反向代理 | Traefik | **Stability + Performance**——file provider + inotify 熱載入，路由增減**零停機** |
| 靜態資產 | Nginx | **Performance**——HTTP 快取消除重複請求的 I/O 瓶頸 |
| 容器 Runtime | Docker OCI + runC | **Performance**——標準 OCI runtime 快速建立實例 |
| 容器 Runtime（強化） | gVisor（runsc） | **Security**——使用者空間核心攔截 syscall，大幅降低逃逸風險；可逐範本選擇 |
| 實例網路 | 每實例 `/30` + 主機發布埠 | **Security（網路隔離）**——見下 |

### 為什麼每實例獨立 `/30` 網段

單一扁平子網（如一個 `/16`）很方便，但同時是**橫向移動的攻擊面**：被攻陷的使用者可掃描共享網段、攻擊其他實例。

- 實例的服務埠**直接發布到 Docker bridge gateway 的主機埠**（`<host_gateway_ip>:<host_port>`）——Traefik 經 `host.docker.internal:<host_port>` 到達，永不使用容器 IP。
- 對外網際網路使用**每實例專屬 `/30` 子網**：只有 gateway 與實例兩個可用 IP，自成一個 L2 網段，**東西向攻擊在結構上不可能發生**。

這就是把網路層隔離推到極致：每個容器活在各自的泡泡裡，唯一的入口是那一個受 Traefik 控管的發布埠。

---

## 我們刻意不做什麼（Anti-Goals / Out of Scope）

憲法必須同時定義界線，避免範圍蔓延：

1. **不做通用 PaaS** — 我們不是 Heroku/Vercel；專注於「互動式開發工作區」，不做無狀態 web app 托管。
2. **不做多主機高可用（現階段）** — 目前是**單主機優先**；多主機編排（Tailscale mesh、叢集）列為路線圖，但單主機永遠要是可運作的最低門檻。
3. **不做虛擬機** — 容器隔離是核心；KVM/QEMU 不在藍圖內。
4. **不做 GPU 之外的硬體加速抽象** — NVProxy 只涵蓋 NVIDIA；AMD/Intel GPU 暫不承諾。
5. **不追新版本不追新技術** — 任何依賴升級必須有明確理由（安全 / 效能 / 必須功能），不為升級而升級。
6. **不做無 SSO 的企業級身份整合** — LDAP/OIDC/2FA 列為可選路線圖項目，不做為基本承諾。

---

## 成功指標（如何知道我們成功了）

- **資源利用率**：一台 8GB 主機能同時服務多個活躍且互相隔離的開發環境，閒置實例自動釋放資源。
- **啟動延遲**：從「點擊啟動」到「瀏覽器進入介面」在秒級內完成（容器建立 + 路由熱載入）。
- **穩定安全**：網路層東西向隔離、每實例獨立憑證、gVisor 沙箱可選——預設設定下無已知攻擊路徑。
- **DX 門檻**：一個 `pnpm run docker:up` 就能從原始碼部署整套平台；新使用者不需要設定指南就能建立帳號並啟動第一個實例。

---

## 相關文件

| 文件 | 內容 |
|---|---|
| [tech-stack.md](tech-stack.md) | 技術決策、部署與更新流程（憲法第二條） |
| [roadmap.md](roadmap.md) | 階段與時程規劃（憲法第三條） |
| [docs/architecture.md](docs/architecture.md) | 系統架構、路由、生命週期、DB schema |
| [docs/rbac.md](docs/rbac.md) | 權限模型（群組制） |
