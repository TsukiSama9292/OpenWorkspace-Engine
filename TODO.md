# VNC 自訂前端開發計畫

## 架構概述

```
Browser ←→ nginx
              ├── /kasm1/          → Svelte 靜態 build（自訂 UI）
              ├── /kasm1/websockify → VNC Server 數據流（WebSocket 代理）
              ├── /kasm2/          → Svelte 靜態 build（自訂 UI）
              └── /kasm2/websockify → VNC Server 數據流（WebSocket 代理）
```

## 技術棧

| 層級 | 技術 | 說明 |
|------|------|------|
| 前端框架 | SvelteKit + `adapter-static` | SSG，編譯為純靜態檔案 |
| VNC 協議 | noVNC core（從參考專案取用） | `rfb.js`, `display.js`, `websock.js` 等 |
| 構建工具 | Vite（SvelteKit 內建） | Tree-shaking、Hot reload |
| 部署 | nginx | 靜態資源 + WebSocket 代理 |

---

## TODO

### Phase 1: 專案初始化 ✅

### Phase 2: noVNC 核心整合 ✅

### Phase 3: VNC 元件實作 ✅

### Phase 4: 路由與佈局 ✅

### Phase 5: WebSocket 代理配置 ✅

### Phase 6: 測試 ✅ (21 tests passing)

---

## 已完成

- Phase 1–6 所有基礎工作完成
- WebSocket 連線正常運作
- `mouseButtonMapper` 初始化問題已修復
- `VNCOPTIONS=-disableBasicAuth` 解決 KasmVNC HTTP Basic Auth
- nginx 配置：`alias` + `try_files` + `/_app/` 位置
- Docker Compose 掛載 build 輸出目錄

---

## 待處理 (UI 優化)

### 7.1 VNC 畫面全屏顯示
- [ ] VNC 畫面應佔滿整個網頁畫面
- [ ] Canvas 尺寸動態調整（100vw × 100vh）
- [ ] 移除目前的 margin/padding 限制

### 7.2 側邊控制面板
- [ ] 左側小型控制面板（悬浮/收合設計）
- [ ] 包含：狀態指示、操作按鈕（Clipboard, Ctrl+Alt+Del, Fullscreen, Settings）
- [ ] 面板不影响 VNC 畫面顯示
- [ ] 響應式設計（桌面/行動裝置）

### 7.3 剪貼簿同步
- [ ] 修復剪貼簿同步功能
- [ ] 測試雙向剪貼簿複製/貼上
- [ ] 處理跨域剪貼簿 API 限制

### 7.4 主題與樣式
- [ ] 深色/淺色主題
- [ ] 控制面板樣式優化
- [ ] 動畫效果

### 7.5 E2E 測試
- [ ] 整合測試（與 VNC Server 實際連線）
- [ ] Playwright 測試
- [ ] 效能優化（Bundle 大小分析）

---

## 檔案結構

```
apps/vnc-ui/
├── src/
│   ├── lib/
│   │   ├── vnc/                    # noVNC 核心
│   │   │   ├── rfb.js              # RFB 協議主類
│   │   │   ├── display.js          # 畫面渲染
│   │   │   ├── websock.js          # WebSocket 傳輸
│   │   │   ├── codecs.js           # 編解碼器管理
│   │   │   ├── constants.js        # UI 常數
│   │   │   ├── base64.js           # Base64 編碼
│   │   │   ├── deflator.js         # 壓縮
│   │   │   ├── inflator.js         # 解壓縮
│   │   │   ├── des.js              # DES 加密
│   │   │   ├── encodings.js        # 編碼常數
│   │   │   ├── messages.js         # RFB 訊息定義
│   │   │   ├── mousebuttonmapper.js
│   │   │   ├── decoders/           # 圖像解碼器
│   │   │   ├── renderers/          # Canvas/WebGL 渲染器
│   │   │   ├── input/              # 鍵盤/滑鼠輸入
│   │   │   ├── output/             # printer, smartcard
│   │   │   ├── util/               # 工具函數
│   │   │   ├── assets/             # codec binaries
│   │   │   └── shims/              # 依賴空實作
│   │   │       ├── ui.js
│   │   │       ├── webutil.js
│   │   │       └── port-relay-worker.js
│   │   └── components/             # 自訂 UI 元件
│   │       ├── VncViewer.svelte    # VNC 顯示器元件
│   │       ├── StatusBar.svelte    # 狀態列元件
│   │       ├── Clipboard.svelte    # 剪貼簿面板
│   │       └── Settings.svelte     # 設定面板
│   ├── routes/
│   │   ├── +layout.svelte          # 根佈局
│   │   ├── +layout.js              # ssr=false, trailingSlash
│   │   └── [...path]/+page.svelte  # 主頁面（catch-all 路由）
│   ├── tests/
│   │   ├── setup.ts
│   │   ├── page.test.ts
│   │   ├── vnc-utils.test.ts
│   │   ├── statusbar.test.ts
│   │   ├── clipboard.test.ts
│   │   └── settings.test.ts
│   └── app.html
├── static/
│   └── assets/
│       ├── avc.bin                 # H.264 codec
│       ├── hevc.bin                # H.265 codec
│       ├── av1.bin                 # AV1 codec
│       └── qoi_viewer_bg.wasm      # QOI decoder
├── svelte.config.js
├── vitest.config.ts
├── playwright.config.ts
├── vite.config.js
└── package.json
```

## 測試指令

```bash
# 單元測試
pnpm test              # 執行所有測試（21 tests）
pnpm test:watch        # 監控模式
pnpm test:coverage     # 覆蓋率報告

# E2E 測試
pnpm test:e2e          # 執行 E2E 測試
pnpm test:e2e:ui       # 開啟 Playwright UI

# 建置
pnpm build             # 建置靜態檔案
pnpm preview           # 本地預覽（port 4173）
```

## 參考資源

- `references_repo/KasmVNC/kasmweb/core/` - noVNC 核心程式碼
- `references_repo/KasmVNC/kasmweb/app/` - 原版 UI 實作參考
- SvelteKit 官方文件：https://kit.svelte.dev
- adapter-static 文件：https://github.com/sveltejs/kit/tree/master/packages/adapter-static
