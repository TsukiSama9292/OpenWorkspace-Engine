### 參考文獻分析工作流(目前)
```
academic-analysis # 主 Agent - 負責：查詢出所有文獻 itemkey, 維度掃描, 發配 subagent, 撰寫最終分析報告
|
├── academic-analysis-dimensions # Phase 1: 維度掃描, 每個文獻 1 個, 讀全文, 列出可比較維度
|
└── academic-analysis-subagent # Phase 2: 深度分析, 每個文獻 1 個, 讀全文, 回傳研究動機, 研究方法, 關鍵發現, 侷限性, 給與本研究的啟發, 可比較維度
```

### 工作流程
```
1. 查詢文獻取得 itemKey 清單
2. 建立 .tex 模板
3. [Phase 1] 並行維度掃描（academic-analysis-dimensions, 每批 15 篇）
   → 讀全文 + metadata, 從 Methodology/Experiments 章節提取可比較維度
4. 彙整維度、建立比較框架
   → 找出高價值維度（至少 2 篇共有）
   → 設計比較角度
5. [Phase 2] 並行深度分析（academic-analysis-subagent, 每批 10 篇）
   → 傳入比較框架，確保維度對齊
   → 讀全文，撰寫完整分析
6. 彙整所有分析結果，完成 .tex 檔案
7. 編譯 PDF
```
