# Sashiki

Terminals for each worktree. All in one place.


## 特徴

### 🪶 軽量
メモリとCPUはAIエージェントに譲る。

### ⚡ 高速
Worktree切替、ターミナル操作、すべてが一瞬。

### 🔀 並列
複数のAIエージェントを同時実行。Git Worktreeで作業空間を分離。

### 👁️ 確認と指示のみ
ファイルパスや行番号はワンクリックでターミナルへ。


## ビルド

libghostty-vt をソースからビルドするため、Rust に加えて Zig が必要。

```sh
mise install
cargo build --release
```

Windows ではこれに加えて MSVC ビルドツールと、git の long path 対応が必要。

```sh
git config --global core.longpaths true
```
