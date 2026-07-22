---
name: github-repository-reference
description: Use when managing external reference repositories as git submodules under references_repo/. Covers adding, cloning, and syncing submodules.
---

## 參考資料庫

研究用的外部 repo 存放在 `references_repo/`，以 **git submodules** 追蹤（定義在 `.gitmodules`）。這些是參考來源，實際實驗程式碼從 `data/` 讀取處理過的資料。

### 新增參考 repo

```bash
git submodule add <repo-url> references_repo/<name>
git commit -m "ref: add <name>"
```

### Clone 後初始化 submodules

```bash
git submodule update --init --recursive
```

### 更新所有 submodules 至最新 commit

```bash
git submodule update --remote --merge
```

### 移除參考 repo

```bash
git submodule deinit -f references_repo/<name>
git rm -f references_repo/<name>
rm -rf .git/modules/references_repo/<name>
git commit -m "ref: remove <name>"
```
