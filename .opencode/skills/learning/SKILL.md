---
name: learning
description: Use at the start of any session, when encountering unfamiliar code/past decisions, or when discovering a hard-won insight during work. Reads and writes docs/learned/*.md to prevent repeated rabbit holes across sessions.
---

## 讀取學習紀錄

`docs/learned/*.md` 記錄了前幾個 session 中付出代價才學到的經驗，這些**從 code 或 papers 中看不出來**。開始任何新工作前**必須先讀取**，避免重蹈覆轍。

現有文件：
- `traject-bench-acc-mismatch.md` — TRAJECT-Bench 的 Acc 指標衡量的是資料品質偵測，不是 tool-use 能力。應聚焦 Traj-Satisfy、Inclusion、Tool Usage、token efficiency。
- `mcp-zero-tool-calls.md` — MCP agent 出現 0 tool calls 的根因（system-prompt 缺少指令、tool JSON 檔名大小寫），以及哪些不是 bug（all-tools-at-once 是設計意圖、MCP 不用 bash）。
- `wall-time-design.md` — Wall time 測量設計：MCP server 在計時前預熱（不計入 MCP wall time），CLI container 建立時間則計入。
- `cli-vs-mcp-fairness.md` — CLI vs MCP 公平性相關經驗。
- `satisfy-skip-for-traject.md` — Traject-Bench 中 Satisfy 指標的跳過策略。
- `traject-simple-vs-hard.md` — TRAJECT-Bench 簡單 vs 困難任務的差異。

## 寫入學習紀錄

**當你在工作中發現以下情況，必須主動將其寫入 `docs/learned/`：**

1. **踩到坑** — 花了超過 15 分鐘才找到的 root cause，或反覆嘗試才解決的問題。
2. **非直覺的發現** — 結果與預期相反，或需要看原始碼/論文才能理解的行為。
3. **設計決策的 WHY** — 某個做法背後的原因從 code 中看不出來，但對未來修改很重要。
4. **工具/框架的陷阱** — API 行為與文件不符、hidden assumption、容易犯的錯誤。

### 檔案格式

```
docs/learned/<kebab-case-title>.md
```

### 檔案模板

```markdown
# <標題>

## 現象

<觀察到什麼 — 用一句話描述>

## Root Cause

<為什麼會這樣 — 技術層面的解釋>

## 結論 / 行動

<下次遇到類似情況該怎麼做，或不該做什麼>
```

### 注意事項

- 檔名使用 kebab-case（例如 `mcp-timeout-issue.md`）
- 內容以「現象 → Root Cause → 結論」結構撰寫，保持簡潔
- 一個文件記錄一個獨立的經驗，不要混杂多個不相關的主題
- 寫完後回報已新增至 `docs/learned/`
