# AGENTS.md

给在本仓库中编写代码的 AI 代理看的约定。这里写的是**硬性要求**，不是建议；与现有代码风格不符的写法一律视为错误。

## 工作边界

- **只写代码。** 不要运行 `cargo check` / `cargo test` / `cargo clippy` / `cargo fmt`，也不要运行 `pnpm typecheck` / `pnpm build` / `pnpm test:*` / `pnpm tauri dev`。验证与编译由维护者本地执行。
- 不要启动开发服务器、预览服务器或桌面窗口，不要截图验收。
- 只格式化本次改动的文件，不要对整仓执行 `prettier --write .`。
- 不要新增第三方依赖（含图表库、UI 库、工具库）。确有必要时先说明理由并等待确认。
- 不要提交、推送、改写 git 历史，不要 `git stash` / `git reset` / `git checkout --`。
- 不要修改 `src-tauri/migrations/` 下**已经发布**的迁移文件。schema 变更一律新增编号更大的迁移文件，并同步提升 `src-tauri/src/infrastructure/db/mod.rs` 里的 `SCHEMA_VERSION` 与升级分支。
- 不要改动 `.git/`、`src-tauri/target/`、`node_modules/`、`dist/`、`logs/`。

## 项目速览

清账 / ClearLedger：本地优先的个人账单桌面应用，Tauri 2 + Vue 3 + TypeScript + Rust + SQLite，全离线，数据只存在本机。

- 前端：Vue 3 `<script setup lang="ts">`、Vue Router hash 路由、Pinia、Naive UI。
- 后端：Rust 同步逻辑跑在 `spawn_blocking` 线程里，单个 SQLite 连接由 `Arc<Mutex<Connection>>` 串行化。
- 文档：`docs/ARCHITECTURE.md`（分层与口径）、`docs/REFUNDS.md`（退款规则）、`docs/ROADMAP.md`（完成度）。

## 分层与文件地图

Rust（`src-tauri/src/`）：

| 路径 | 职责 |
| --- | --- |
| `domain/ledger.rs` | 交易结构体、校验函数、IPC 输入输出类型 |
| `domain/refunds.rs` | 退款关联的纯领域逻辑，不碰数据库 |
| `domain/mod.rs` | `AppError` 与 `ApiResult` 错误信封 |
| `infrastructure/db/ledger.rs` | 交易读写、汇总、月明细、审计 |
| `infrastructure/db/refunds.rs` | 退款扫描、关联持久化、统计口径判定 |
| `infrastructure/db/mod.rs` | 连接初始化、迁移执行、备份 |
| `commands/mod.rs` | 薄薄一层 Tauri 命令，只做 `spawn_blocking` 转发 |
| `lib.rs` | `invoke_handler` 注册表，新命令必须在这里登记 |

前端（`src/`）：

| 路径 | 职责 |
| --- | --- |
| `api/client.ts` | `desktopAvailable`、应用状态、IPC 可用性判断 |
| `api/ledger.ts` / `api/refunds.ts` | 每个命令一个 camelCase 包装函数 + 类型 |
| `app/router.ts` | `navigation` 数组同时驱动侧边栏和路由 |
| `app/styles.css` | 全站样式与主题变量，**唯一**的样式来源 |
| `components/` | 可复用组件（编辑器、导航图标、图表） |
| `pages/` | 路由页面 |
| `composables/money.ts` | 金额格式化与解析、上海时区日期 |
| `stores/app.ts` | 全局状态（主题、连接状态） |

## 后端约定

**错误信封。** 所有命令返回 `ApiResult<T>`：成功是 `{ ok: true, data }`，失败是 `{ ok: false, error: { code, message, traceId } }`。命令函数签名固定为 `Result<ApiResult<T>, ()>`，外层 `Result` 永远是 `Ok`。新增错误分类时在 `AppError` 与 `ApiResult::from` 的 `match` 中同时登记，不要在前端硬编码新的 code。

**命令落地四件套。** 新增一个命令必须同时改：`domain/*.rs` 的输入输出类型、`infrastructure/db/*.rs` 的实现、`commands/mod.rs` 的薄包装、`lib.rs` 的 `generate_handler!` 列表；前端再补 `api/*.ts` 的类型与包装函数。

**输入校验。** IPC 输入结构体统一 `#[derive(Serialize, Deserialize)]` + `#[serde(rename_all = "camelCase", deny_unknown_fields)]`，并提供 `validate()` 做长度、格式、范围检查。校验失败返回 `AppError::Validation("中文提示")`。

**金额。** 一律用 `i64` 整数分存储，SQLite 列是 `INTEGER` 且带 `> 0` 约束；通过 IPC 时序列化为**字符串**，前端用 `BigInt` 运算，禁止浮点。

**写入必须在一个事务里完成**，且同时产生 `audit_events` 与 `command_requests` 记录：

- 乐观锁：输入带 `expected_revision`，与库中不一致返回 `AppError::RevisionConflict`，每次写入 `revision = revision + 1`。
- 幂等：输入带 `request_id`（UUID v4），用 `replay()` 比对 `payload_json`；同 id 同内容返回上次结果，同 id 不同内容返回 `RequestConflict`。
- 审计动作名用英文动词：`create` / `update` / `void` / `confirm`。
- 交易改动后调用 `super::refunds::after_edit(&tx, Some(&before), &saved)`，让退款关联的 revision 跟随。批量写入时逐条调用。

**查询与分页。** 列表走键集游标：`ORDER BY business_date DESC, id DESC LIMIT 51`，取 50 条并回填 `nextCursorDate` / `nextCursorId`。筛选条件用具名参数 `?1..?n`，拼接 SQL 只拼接常量列名，绝不拼接用户输入。

**迁移。** `000X_name.sql` 只写幂等 SQL（`CREATE ... IF NOT EXISTS`、`INSERT OR IGNORE`），并以 `INSERT OR IGNORE INTO schema_migrations` 收尾。已有的 `user_version` 分支写法不要重构。

## 前端约定

**组件风格。** 一律 `<script setup lang="ts">`，Composition API，`<template>` 在脚本之后。组件用 PascalCase 文件名。

**UI 组件。** Naive UI 只用基础件（`NButton`、`NModal`、`NPagination` 等）；表格、表单控件、图表用原生 HTML + `styles.css` 里的类，保持与现有页面一致。

**请求竞态。** 页面里凡是会随筛选条件重发的请求，都要用 `requestVersion` 计数器丢弃过期响应（参考 `TransactionsPage.vue`、`OverviewPage.vue`），并在 `onBeforeUnmount` 里自增以作废在途请求。筛选变更用 `watch` 自动重载，不要加"查询"按钮。

**深链参数。** 从 `route.query` 取到的值必须先用正则校验（如 `/^\d{4}-(0[1-9]|1[0-2])$/`），非法值回退到默认，不要把原始字符串直接塞进请求。

**金额与日期。** 只用 `src/composables/money.ts` 里的 `formatMoney` / `parseYuan` / `editableYuan` / `todayShanghai`。不要自己写 `toFixed` 或 `new Date().toISOString()`。

**样式。** 全部写进 `src/app/styles.css`，组件内不放 `<style>`。颜色只用 CSS 变量：主题色 `--primary` / `--text` / `--muted` / `--border` / `--surface` / `--soft` / `--hover`，图表色 `--chart-1` / `--chart-2` / `--chart-3` / `--chart-grid` / `--chart-axis` / `--chart-muted`。新增变量时**必须**在 `:root` 和 `:root[data-theme='dark']` 两处同时定义。响应式断点是 1150px / 850px / 650px。

**代码风格。** Prettier 配置为无分号、单引号、尾逗号 all、2 空格缩进。Rust 用 rustfmt 默认风格。注释写"为什么"，不写"是什么"；不要留 `TODO` 或注释掉的旧代码。

**文案与可访问性。** 界面文案用中文，标识符用英文。交互控件要有 `aria-label`；加载/结果提示用 `role="status"` + `aria-live="polite"`；错误用 `role="alert"`。破坏性操作（作废、批量核实）必须先 `window.confirm`。

## 图表

图表是自绘的（CSS + 内联 SVG），不引入图表库。

- 单一数值用指标卡片，不要画成一根柱子；以"这个月/这年"为主语时用柱状图。
- 单系列不需要图例，标题说明画的是什么；两个及以上系列必须有图例，且不能只靠颜色区分。
- 永远不要双 Y 轴。网格线 1px 实线、贴近背景色；柱宽不超过 24px，圆角只加在数据端；面积填充不超过 10% 不透明度。
- 分类色按 `--chart-1/2/3` 的固定顺序分配，跟随实体而不是排名，筛选后不许重新上色。超过 8 类折叠成"其他"。
- 每个图表都要有等价的可读文本：悬停读数（`role="status"`）或表格视图，不能让数值只能靠 hover 获取。
- 改动图表配色后，提醒维护者重新做色觉可辨性校验，不要自行判断"看起来够区分"。

## 业务口径（不可违背）

- 只有 `review_status = 'confirmed'`、`lifecycle = 'active'` 的交易计入统计；`pending` 只计笔数不计金额。
- `nature = 'unknown'` 不能置为 confirmed，必须先编辑出确切性质。
- 退款按**发生日**冲减当期支出；`transfer`（本人转账）、`repayment`（本金还款）、`loan`（借贷往来）不计入收支。
- 退款关联只能靠渠道限定的强标识（订单号、原交易单号）判定，**绝不允许**用金额、时间、商户相似度作为同一性证据；校验不通过只是暂停计入统计，不自动改金额、不改确认状态。

## 新增功能自查

1. 输入输出类型加了 `deny_unknown_fields` 和 `validate()` 吗？
2. 写入是否在同一事务里同时写了 `audit_events` 与 `command_requests`，并处理了 `expected_revision` 与 `request_id`？
3. 命令是否在 `lib.rs` 注册、在 `api/*.ts` 有类型与包装？
4. 页面是否处理了请求竞态、空状态、错误态和加载态？
5. 新样式是否只用 CSS 变量，且深浅两套都定义了？
6. 是否误把测试/编译/格式化整仓的命令带进了改动？
