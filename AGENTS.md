# AGENTS.md — vxapo-cli

本文件面向在本仓库工作的**人**与 **AI**。核心只有一条：**格式不由人决定。**

---

## 1. 格式：`cargo fmt` 是唯一权威

- 风格配置在 `rustfmt.toml`（`style_edition = "2021"`、`newline_style = "Unix"`）。
  折行策略就是 rustfmt 默认值，**没有任何自定义 width 参数**——这是刻意的。
- 工具链钉在 `rust-toolchain.toml`（`channel = "1.97.1"`）。**不要**本地随意升级，见 §4。
- 本仓是 workspace 根，成员为 `protocol/`（包名 `vxapo-protocol`）。
- **不要手工微调格式**，也不要为局部观感引入新的格式化工具。

### 本仓库实测过的 rustfmt 行为（复审 diff 时不要误判为逻辑改动）

`cargo fmt` 除调整空白外，还会做这几类**等价**的机械改动：

1. **删除文件开头的 UTF-8 BOM**（U+FEFF）。
2. **补齐多行构造的尾随逗号**。
3. **重排 `use` 列表与 `mod` 声明**（按字典序）。
4. **给单表达式分支加/去花括号**（如 `=> expr` 展开成 `=> { expr }`）。
5. **给尾表达式补分号**（如 `else { return }` → `else { return; }`）。

---

## 2. ⚠️ 本仓最大陷阱：`cargo fmt --all` 会连带动别的仓库

本仓通过 **path 依赖**引用 `../vxapo-driver`：

```toml
vxapo-driver = { path = "../vxapo-driver" }
```

因此 **`cargo fmt --all` 会把 driver 的文件也一起格式化**。实测（2026-10）：

| 命令 | 总 hunk | 其中 driver 的文件 |
|---|---|---|
| `cargo fmt --all` | 589 | **445** |
| `cargo fmt -p vxapo-cli -p vxapo-protocol` | 141 | 0 |

后果：driver 的文件会出现在**本仓的提交**里，破坏仓库边界，
让 blame-ignore 与回滚都变复杂。

**所以本仓一律用显式 `-p`：**

```sh
cargo fmt -p vxapo-cli -p vxapo-protocol
cargo fmt -p vxapo-cli -p vxapo-protocol -- --check
```

`.githooks/pre-commit` 已按此写好。driver 的文件**只在 driver 仓库里改**。

---

## 3. 提交前：必须通过格式检查

```sh
cargo fmt -p vxapo-cli -p vxapo-protocol -- --check    # 退出码 0、无输出才算通过
```

**新克隆的机器必须先执行一次**（hook 不随 clone 传播）：

```sh
git config core.hooksPath .githooks
```

---

## 4. 纪律：fmt 提交必须纯净

**逻辑改动不得与格式化放进同一提交**——`.git-blame-ignore-revs` 依赖「这是纯格式化」
这一事实才能安全跳过。提交后把**完整** hash 追加进 `.git-blame-ignore-revs`，
并执行一次 `git config blame.ignoreRevsFile .git-blame-ignore-revs`。

---

## 5. 升级 rustc 的流程（**独立事件**）

1. 先单独完成升级并提交。
2. 升级后单独跑 `cargo fmt -p vxapo-cli -p vxapo-protocol`，若有变化：
   单独成 `style:` 提交 → 追加 hash 到 `.git-blame-ignore-revs` → 更新
   `rust-toolchain.toml`。
3. 无变化则只更新 `rust-toolchain.toml`。

> driver 与本仓各自钉同一版本，升级时两个仓的 `rust-toolchain.toml` 要一起改。

---

## 6. 行尾与编辑器

- 行尾由 `.gitattributes` 统一为 **LF**（`*.cmd` / `*.bat` 例外）。
- `.editorconfig` 管非 Rust 文件；`.vscode/settings.json` 已入库（保存即格式化）。

---

## 7. 质量基线

```sh
cargo test                                          # 22 passed; 0 failed
cargo fmt -p vxapo-cli -p vxapo-protocol -- --check # 退出码 0
```

### ⚠️ clippy 当前**不是**绿的（既有问题，非格式所致）

```sh
cargo clippy --all-targets -- -D warnings   # 当前退出码 101
```

本仓 clippy 长期未收口，约有 12 类告警（`doc_lazy_continuation`、`needless_borrow`、
`if_same_then_else` 等）。**这一状态在格式化之前就已存在**——已用 stash 回到 pre-fmt
状态实证，pre-fmt 与 post-fmt 的错误集合完全一致，格式化未引入任何新 lint。

因此：**不要**把 clippy 通过当作本仓的提交门槛（会永远提交不了），
但也**不要**因为顺手就大规模清理它——那属于独立任务，应与格式化分开。
新代码尽量不要新增告警。
