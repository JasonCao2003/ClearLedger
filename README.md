# 清账 · ClearLedger

本地优先的个人账单管理软件，使用 **Rust + Vue 3 + SQLite + Tauri 2** 构建。

ClearLedger is a local-first desktop app for personal transaction management, built with Rust, Vue, SQLite, and Tauri.

> 当前处于早期开发阶段。手工记账、基础收支汇总及旧 HTML 迁移工具已实现；部分导航页面仍是功能占位。

## 功能

| 模块                               | 当前状态                                   |
| ---------------------------------- | ------------------------------------------ |
| 手工记账                           | 新增、编辑、作废，保留审计记录             |
| 交易明细                           | 月份、性质、关键词筛选，游标分页           |
| 收支总览                           | 已确认交易的收入、消费、退款、净支出与结余 |
| 金额处理                           | 整数分存储、字符串 IPC、BigInt 精确展示    |
| 本地存储                           | SQLite、WAL、外键、版本迁移及迁移前快照    |
| 编辑保护                           | revision 冲突检查、重复提交幂等、事务审计  |
| 桌面界面                           | 深浅主题、响应式导航、浏览器预览           |
| 旧账本迁移                         | HTML 内嵌 JSON 解析、证据保存、分组及校验  |
| 账单导入中心、规则、账户、数据质量 | 页面入口已建立，业务待接入                 |
| 完整备份恢复、退款订单关联、趋势图 | 待开发                                     |

默认离线使用，无需云账号。数据库目前不加密；请自行保护本机账单、数据库和备份。

## 技术栈

- **桌面**：Tauri 2
- **前端**：Vue 3、TypeScript、Vite、Vue Router、Pinia、Naive UI
- **后端**：Rust、rusqlite、serde、tracing
- **存储**：SQLite
- **迁移工具**：Python 标准库

## 环境要求

- Node.js 24 LTS：前端最低要求为 22.12，金额测试使用 Node 的 TypeScript 类型擦除功能，推荐统一使用 Node 24。
- pnpm 11.25.0
- Rust stable，Windows 使用 MSVC 工具链
- Windows：Visual Studio C++ Build Tools、Windows SDK、WebView2 Runtime
- Python 3.10+，仅运行旧 HTML 迁移工具和 Python 测试时需要

## 快速启动

```powershell
git clone https://github.com/JasonCao2003/ClearLedger.git
cd ClearLedger
npm.cmd install -g pnpm@11.25.0
pnpm.cmd install --frozen-lockfile
pnpm.cmd desktop:dev
```

只预览前端界面：

```powershell
pnpm.cmd dev
```

浏览器预览不连接 SQLite，不会加载模拟账单。需要桌面应用才能记账。

## 编译

Debug 桌面程序，不生成安装包：

```powershell
pnpm.cmd tauri build --debug --no-bundle
```

Windows 可执行文件：`src-tauri/target/debug/ledger-desktop.exe`。

Release 桌面程序：

```powershell
pnpm.cmd tauri build --no-bundle
```

Windows 可执行文件：`src-tauri/target/release/ledger-desktop.exe`。

产品名为 **清账 / ClearLedger**。内部 Rust crate 与可执行文件暂保留 `ledger-desktop` 名称，正式安装包尚未配置。

如果 Windows 返回错误 4551 或提示 Application Control 阻止构建脚本，需按设备管理策略处理开发产物执行权限；这不表示未安装 Rust。

## 开发检查

```powershell
pnpm.cmd typecheck
pnpm.cmd format:check
pnpm.cmd test:money
pnpm.cmd build
python -m unittest discover -s tests -p "test_*.py" -v
pnpm.cmd rust:fmt
pnpm.cmd rust:check
pnpm.cmd rust:test
pnpm.cmd rust:clippy
```

默认 Python 测试只使用合成数据，不依赖个人账单或仓库外的旧工程。

可选的真实 HTML 回归验证只读取用户自行提供的文件：

```powershell
$env:CLEARLEDGER_LEGACY_HTML = 'D:\private-data\ledger.html'
python -m unittest discover -s tests -p test_legacy_import.py -v
```

前端构建、金额测试及合成数据库/导入测试已通过。Rust 测试用例已编写；桌面端到端行为仍需持续验收，不将前端构建视为完整桌面验收。

## 数据存储与统计

数据位于操作系统应用数据目录，文件名为 `ledger.sqlite3`。为兼容初始版本已有数据，当前保留 Tauri identifier `com.jason.personalledger`；重命名产品不会切换已有数据目录。可在应用“设置与备份”查看实际位置。

统计只计入有效、已确认交易：退款按归属日期冲减支出，本人转账、本金还款和借贷本金不重复记收支。待核实记录不计入正式汇总。期间结余不是账户余额，没有记录也不代表账单覆盖完整。

迁移前自动快照用于保护升级操作，**完整备份恢复界面尚未实现**。

## 旧 HTML 账本迁移

该工具适用于包含 `<script id="ledger-data" type="application/json">` 数据块的旧版账本，不是通用网页导入器。仅解析 JSON，不执行源 HTML 的 JavaScript。

先启动一次桌面程序以创建数据库，再关闭程序并预览导入：

```powershell
python scripts/import_legacy_html.py --html 'D:\private-data\ledger.html'
```

确认后执行，工具先生成一致性备份，再在事务中写入：

```powershell
python scripts/import_legacy_html.py --html 'D:\private-data\ledger.html' --apply --report 'D:\private-data\import-report.json'
```

可通过 `--db` 指定数据库路径。默认路径适配 Windows；其他平台请显式传入该参数。

- 保留源 HTML、原始字段、来源行号、规则分组、关联候选和导入审计。
- 零金额组保留为证据，不生成虚假的正金额交易。
- HTML 不含浏览器 localStorage 人工修正，当前工具不迁移修正备份。
- 自动判断不会变成人工确认，导入交易统一待核实，旧初步统计资格独立保存。
- 同一 HTML 重试不会新增交易或覆盖后续人工修改。
- 改变的文件出现原始 ID 冲突时回滚，避免猜测覆盖。
- 当前仅兼容 schema v2；详细来源兼容说明见 [架构文档](docs/ARCHITECTURE.md)。

## 目录

```text
src/
  app/                 路由与全局样式
  api/                 类型化 IPC 与 DTO
  components/          公共组件、记账表单
  composables/         精确金额处理
  pages/               总览、明细及其他页面
  stores/              应用状态
src-tauri/
  capabilities/        窗口权限
  migrations/          SQL 迁移
  src/commands/        Tauri 命令
  src/application/     应用用例
  src/domain/          业务类型与校验
  src/infrastructure/  SQLite 访问
scripts/               旧数据迁移工具
tests/                 合成样本、金额和数据库测试
docs/                  架构与开发路线
```

## 开发计划

完整备份恢复 → 分类与账户管理 → CSV/XLSX 导入 → 凭证核对与退款关联 → 初步/正式统计切换 → 图表与数据质量 → 安装包发行。

详见 [开发路线](docs/ROADMAP.md) 与 [架构说明](docs/ARCHITECTURE.md)。

仓库仅保存代码、文档和合成测试；个人账单、数据库、导入报告、备份与构建产物不属于版本控制内容。暂未指定开源许可证。
