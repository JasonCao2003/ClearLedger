# 清账 · ClearLedger

Windows 单机账本。桌面改版沿用 `ledger` 网页版的界面与记账规则，使用 Tauri 2 窗口、Rust 命令和本地 SQLite。

## 日常使用

1. 双击 `ledger-desktop.exe` 打开账本。
2. 在「设置 → 导入与备份」选择支付宝 CSV 或微信 XLSX。可以多选文件，导入后可查看重复记录和跳过条数。
3. 在消费日历、明细、待还和设置之间切换。明细支持按月、按年或查看全部。商户分类与备注可在「设置 → 分类管理」中管理。
4. 在「待还」点击「手工添加」，选择消费或分期，录入花呗或白条记录。

数据库位于 Tauri 的本机应用数据目录，Windows 默认路径为 `%LOCALAPPDATA%\com.jason.personalledger\desktop\ledger.sqlite3`。桌面改版与旧版使用不同的数据库位置。「设置」可管理消费分类，也可导出包含流水和设置的完整 JSON 账本，或只导出核对设置。备份包含个人交易信息，请妥善保存。

## 从 ledger 网页版迁移

先在原网页的「导入核对」导出核对设置。若只迁移支付宝和微信，可在新版导入原始 CSV / XLSX，再恢复核对设置。

若还需要保留原网页读取的工行、美团、京东历史流水，在原网页服务运行时，使用 PowerShell 生成历史数据快照。先将输出路径指向专门保存个人备份的目录，并确认目标文件不存在。

```powershell
$backup = Join-Path $env:LOCALAPPDATA 'ClearLedger-历史流水.json'
if (Test-Path -LiteralPath $backup) { throw '备份文件已存在，请指定另一个文件名' }
(Invoke-WebRequest -Uri 'http://127.0.0.1:8765/api/ledger').Content |
  Set-Content -LiteralPath $backup -Encoding utf8
```

然后在桌面版选择「导入历史数据或账本备份」，再选择「恢复核对设置」。桌面版日常导入支持支付宝和微信。金额、退款绑定、分期和统计规则沿用原网页。

## Windows 构建

需要 Node.js、pnpm、Rust MSVC 工具链、Windows C++ Build Tools / SDK 以及 WebView2。构建不需要个人账单文件。

```powershell
pnpm install --frozen-lockfile
pnpm tauri build --no-bundle
```

可执行文件位于 `src-tauri/target/release/ledger-desktop.exe`。运行目标电脑需要 WebView2，应用不需要 Python 或本地 HTTP 服务。

桌面入口为根目录 `index.html`、`src/desktop.js` 和 `src-tauri/src/desktop.rs`。仓库此前的 Vue 页面、Rust 模块及迁移文件用于保留旧版源码，桌面改版入口只加载新版模块。
