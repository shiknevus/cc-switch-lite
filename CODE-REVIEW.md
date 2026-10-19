# CC Switch Lite 代码审查报告

> 审查对象：`D:\ccswitch\cc-switch-lite`（Tauri v2 + React 18 + TS + Vite 7 + Rust + rusqlite）
> 审查方式：通读全部 `src/` 前端源码、`src-tauri/src/` 后端源码（约 30,215 行 Rust / 14,181 行 TS），
> 实际运行 `tsc --noEmit`、`cargo check`、`cargo clippy`、`vite build`，并核对产物与配置。
> **本报告只做分析，未修改任何源码。**

---

## 0. 总体评价

| 维度 | 评价 |
|---|---|
| 架构选型 | 合理。Lite 版砍掉代理/供应商/MCP 后，只保留"读本地会话 → 落库 → 看板"，职责清晰 |
| 后端测试 | **很好**。318 个 `#[test]`，测试代码 15,263 行 ≈ 生产代码 14,952 行，比例 1:1 |
| 后端错误处理 | **很好**。生产路径 0 处 `unwrap()/expect()/panic!`（已脚本全量核验） |
| 注释质量 | **很好**。大量"为什么这么写"的注释，说明了取舍与历史坑 |
| 数据正确性 | **有严重缺陷**。存在多条"静默错账且不可逆"的路径（见 P0） |
| 前端功能完整性 | **有严重缺陷**。核心的"自动扫描间隔"设置完全未生效（见 P0-4） |
| 前端测试 | **缺失**。0 个测试文件，0 个测试依赖 |
| 打包体积 | **偏大**。首屏 2.15 MB JS，其中约 1.1 MB 是 4 份语言包全量内联 |
| 工程化 | **有缺口**。无 ESLint 配置（但代码里留着 eslint-disable 注释）、无前端测试、CI 两端 Rust 版本不一致 |

**一句话结论**：这是一个"工程素养高、但数据口径与前后端契约缺乏端到端验证"的项目。后端解析层写了大量精细的回归测试，却没能覆盖住"跨模块组合"产生的错误（游标归属判定、语义列、单连接锁竞争）；前端则有几处"注释承诺了行为、实现却没写"的确定性缺陷。

---

## 1. P0 — 必须优先修复

### P0-1【数据永久虚高·不可逆】`is_gemini_cursor_path` 子串匹配过宽，重建 Gemini 会误删 Claude 的同步游标

**位置**：`src-tauri/src/services/session_usage_gemini.rs:721-727`、`:766-774`

```rust
fn is_gemini_cursor_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.contains("transcript.jsonl")
        || lower.contains("session-")        // ← 命中 Claude 的项目目录
        || lower.contains(".gemini")
        || lower.contains("antigravity")
}
```

`reset_gemini_usage_on_conn` 用它逐行 `DELETE FROM session_log_sync`，触发点是暴露给前端的
`rebuild_gemini_usage` / `rebuild_agy_usage`（`commands/usage.rs:417-440`）。

**为什么一定会命中**：Claude Code 的项目目录名是 **cwd 把路径分隔符替换成 `-`**，例如
`~/git/session-manager` → `~/.claude/projects/-Users-x-git-session-manager/`。
这个字符串包含 `session-` → 判定为 Gemini 游标 → **该项目的 Claude 游标行被删除**。

**后果链条**（真正致命的地方）：

1. 下一轮 Claude 解析器读到 `cursor = None` → `start_byte = 0` → **从文件头全量重读**；
2. 30 天内的明细还在 → `request_id` 主键 + 指纹去重能兜住，只是白跑；
3. **超过 30 天的明细已被 `rollup_and_prune` 汇总并删除**（`dao/usage_rollup.rs:175-180`）→
   查不到旧行 → 重新 INSERT（`session_usage.rs:1311-1347`，`created_at` 用日志原始时间戳）
   → 下一轮 rollup 再把它们 **`decimal_add` 累加一次**到同一天的 `usage_daily_rollups`
   → **历史用量永久虚高，且明细已删、无法回溯修正。**

**建议**：不要按路径子串猜归属。给 `session_log_sync` 加一列 `importer`
（`'claude' | 'codex' | 'gemini' | …`），由写入方自己填，重建时按列删；或退一步，
对已知根目录做精确前缀匹配（`~/.gemini/antigravity-cli/...` 的完整父路径）。

---

### P0-2【系统性静默漏记】Claude 增量同步只按 mtime 判"文件未变化"，完全不看文件大小

**位置**：`src-tauri/src/services/session_usage.rs:659-671`

```rust
let file_modified = metadata_modified_nanos(&metadata);
let file_size = metadata.len() as i64;          // ← 算出来了
let last_byte_offset = cursor.and_then(|c| c.last_byte_offset);

// 文件未变化则跳过
if file_modified <= last_modified {             // ← 但判定里没用 file_size
    return Ok(ClaudeFileSync::default());
}
```

`file_size` 之后只用于"游标越界 = 文件被截断"的判定，**从未参与"要不要读"的决策**。

**为什么是 bug**（两条真实触发路径）：

1. **Windows 上写入方长期持有句柄时 mtime 可能滞后**。这不是推测——同仓库的 Codex 解析器
   自己写了这条注释并为此加了 size 兜底（`session_usage_codex.rs:1972-1976`）：
   ```rust
   // Windows may keep mtime unchanged while Codex holds its write handle open.
   let unchanged = file_modified == last_modified
       && last_byte_offset == i64::try_from(file_size).ok();
   ```
   Claude Code 同样在整个会话期间持有 JSONL 追加句柄，却用的是纯 mtime 门。
2. **mtime 回退**：从备份/Time Machine 还原、`tar -x` / `unzip` 保留旧 mtime、NTP 校时回拨，
   都会让 `file_modified <= last_modified` 成立，即使文件已经增长了几千字节。

**后果**：这些文件的尾部数据永远不入库，**且零日志**（早退分支没有 `warn`），
用户只看到"统计比实际少"，无法归因。

**建议**：判定改为 `file_modified <= last_modified && last_byte_offset == Some(file_size)`
（legacy 行号游标再回退到 `<=` 语义）。改动约 5 行。

---

### P0-3【静默丢量】Gemini：`message.id` 缺失时所有消息塌缩成同一个 `request_id`

**位置**：`src-tauri/src/services/session_usage_gemini.rs:480`、`:514`、`:637-654`

```rust
let message_id = msg.get("id").and_then(|v| v.as_str()).unwrap_or("unknown");
let request_id = format!("gemini_session:{session_id_str}:{message_id}");
```

写入语句用的是 `ON CONFLICT(request_id) DO UPDATE SET input_tokens = excluded.input_tokens, ...`
—— **整体覆盖，不是累加**。

**后果**：一个会话里若有多条 `type == "gemini"` 的消息缺 `id`（旧版本或被裁剪过的 session
文件很常见），它们全部映射到 `gemini_session:<sid>:unknown`，**只有最后一条的 token 保留**，
其余在同事务内被静默覆盖，连 `skipped` 都不计。

**建议**：`id` 缺失时退回稳定键。**不要**用 `messages` 数组下标——`parse_session_document`
会对消息做 upsert 与 `$rewindTo` 截断（`session_manager/providers/gemini.rs:62-87`），下标不稳定。
可用 `(时间戳, input, output, cache, 内容哈希前 16 字节)`；Pi 的 `pi_request_identity`
（`session_usage_pi.rs:781-849`）已是可复用范式。

---

### P0-4【功能完全失效】前端"扫描间隔"设置无效：定时扫描器从未被创建

**位置**：`src/hooks/useUsageAutoRefresh.ts:44-63`

注释（`:18-24`）承诺：

```
用量页打开期间按所选间隔扫描会话日志：打开即扫一次，之后每隔一段扫一次，
窗口隐藏时停、重新显示时立刻补扫。后台只有 10 分钟一次的兜底扫描。
```

实际 effect 体内**只有一句 `focusManager.subscribe`，没有任何 `setInterval`，也没有首次立即扫描**：

```ts
useEffect(() => {
  if (!sessionAutoSyncEnabled || refreshIntervalMs <= 0 || paused) return;
  let disposed = false;
  const unsubscribe = focusManager.subscribe((focused) => {
    if (focused && !disposed) {
      void queryClient.invalidateQueries({ queryKey: usageKeys.all });
    }
  });
  return () => { disposed = true; unsubscribe(); };
}, [paused, queryClient, refreshIntervalMs, sessionAutoSyncEnabled, syncSessionUsage]);
```

`syncSessionUsage`（`:35-42`）的唯一调用方是手动按钮 `UsageDashboard.tsx:375`。

**三重后果**（已逐条核实）：

1. 页头「扫描间隔 5s / 10s / 30s / 60s」下拉（`UsageDashboard.tsx:540-549`）改了毫无作用；
2. **更严重的是**：`UsageDashboard.tsx` 把所有子组件（Hero / TrendChart / Heatmap /
   RequestLogTable / ProviderStatsTable / ModelStatsTable）的 `refreshIntervalMs` 都传成了
   **字面量 `0`**（`:811, :822, :831, :926, :936, :945`），于是它们的
   `refetchInterval: refreshIntervalMs > 0 ? refreshIntervalMs : false` 全部落到 `false`
   → **轮询被彻底关闭**；
3. 唯一剩下的周期性刷新是 `:65-73` 那个**写死 60 秒、无差别 invalidate 整个 `usageKeys.all`**
   的定时器（含 `detail`、`logs`、`all-time-summary`），即"刷新节奏"与用户设置完全脱钩。

**建议**：在同一 effect 内补"首扫 + 定时扫 + 卸载清理"，并把它与 `rollingRange` 的 60s
重查合并为**一个节拍器**，间隔取 `refreshIntervalMs`。

---

### P0-5【panic → 全库永久不可用】`lock_conn!` 不处理 Mutex 中毒

**位置**：`src-tauri/src/database/mod.rs:14-22`

```rust
macro_rules! lock_conn {
    ($mutex:expr) => {
        $mutex.lock()
            .map_err(|e| AppError::Database(format!("Mutex lock failed: {}", e)))?
    };
}
```

任何线程在**持锁期间** panic，`Mutex<Connection>` 就永久中毒，此后**所有** DB 操作
（读看板、写设置、同步）都返回"锁失败"，UI 全面失效，只能重启进程。

而持锁路径上确实存在可 panic 的算术：`rust_decimal` 的 `Mul`/`Add` 溢出会 panic
（`calculator.rs` 的 `tokens * price_per_million` 中间结果可超 7.9e28，而用户自定义定价
只校验"非负"、无上界）。

**旁证**：作者知道这个坑——`clear_codex_replay_caches` 专门写了
`unwrap_or_else(PoisonError::into_inner)`（`session_usage_codex.rs:534-541`），
但主连接没有同样处理。

**建议**：`lock().unwrap_or_else(|e| e.into_inner())` 并记一次 `error` 日志，
把"一次操作异常"降级，而不是"整应用死锁"。同时给定价加数值上界。

---

## 2. P1 — 本迭代应修

### P1-1【数据错账】`rebuild_rollup_primary_key` 用 `MAX(input_token_semantics)` 混合不同语义聚合

**位置**：`src-tauri/src/database/schema.rs:97-107`

```sql
COALESCE(SUM(input_tokens), 0),
...
COALESCE(MAX(input_token_semantics), 0),
```

旧表主键只有 `(date, app_type, provider_id, model)`，重建时按 6 列 `GROUP BY` 合并，
同一组内可能同时存在 semantics = 0（legacy，input 含 cache_read）、= 1（total）、= 2（fresh）。
`MAX()` 会把整组标成 2，而 `input_tokens` 是**未归一化的裸和**；读侧 `fresh_input_sql`
（`sql_helpers.rs:56`）看到 semantics = 2 就原样返回 → 组内 legacy 行的 cache_read 被重复计入。

这正是"新旧代码混跑一段时间的库"的必然状态，**触发后没有修复路径（明细已删）**。

**建议**：与 `usage_rollup.rs:135` 一致，用 `SUM(fresh_input_sql(...))` 归一化后统一写 `FRESH`。

### P1-2【读命令写库·自我放大】`get_request_logs` / `get_request_detail` 会对合法零成本行反复 UPDATE

**位置**：`usage_stats.rs:2454-2464`（判断）、`:2519-2536`（写回），调用点 `:2219`、`:2256`

```rust
let has_cost = existing_cost > rust_decimal::Decimal::ZERO;
if has_cost || !has_usage { return Ok(false); }
```

三种**合法**的零成本都会每次重算并 `UPDATE`：

1. `cost_multiplier = 0`（免费/包月；前端 `isUnpricedUsage` 明确把它排除，说明团队认可）；
2. 定价行本身就是 0；
3. 真实费用 < 0.0000005 → `format!("{total_cost:.6}")` 就是 `"0.000000"`（**最常见**）。

**后果**：读路径写库 → WAL 暴涨；`total_changes()` 变化 → `LogCountCache`
（`usage_stats.rs:619-674`）立刻失效 → 下次重算 `COUNT(*)`；并与会话导入争抢同一把连接锁。
叠加 P0-4 被关掉的轮询、以及 `usage-log-recorded` 每 200ms 一次的全量 invalidate
（`usage_events.rs:47-72` + `useUsageEventBridge.ts:9-11`），会形成显著的写放大。

**建议**：加 `pricing_status` / `cost_backfilled_at` 列区分"已计价但确实是 0"与"未计价"；
读路径只读，回填挪到写路径或后台任务。

### P1-3【口径不一致】趋势 `total_tokens` 少算缓存 token

**位置**：`usage_stats.rs:1341`（小时）、`:1455`（按天）、`:1538`（rollup 合并）

```rust
COALESCE(SUM({fresh_input} + l.output_tokens), 0) as total_tokens,
```

对比 `sql_helpers.rs:80-83` 的 `real_total_tokens_sql`（fresh + output + cache_creation + cache_read）。
`UsageSummary.realTotalTokens` / `ProviderStats.totalTokens` / `ModelStats.totalTokens`
**都含缓存**，只有 `DailyStats.totalTokens` 不含。前端 `UsageTrendChart.tsx:134-138`
自己把四个分桶相加，所以暂时看不出问题——但这是明确的契约错误，且测试
（`usage_stats.rs:5594`）把它固化了（断言 150 = 100 + 50）。

### P1-4【默认视图就出错】小时分桶越界被 clamp 后 `insert` 覆盖，吞掉最后一小时

**位置**：`usage_stats.rs:1296-1302` + `:1386-1390`

```rust
let bucket_count = (duration + bucket_seconds - 1) / bucket_seconds;
...
if bucket_idx >= bucket_count { bucket_idx = bucket_count - 1; }
map.insert(bucket_idx, stat);          // ← 覆盖，不是累加
```

SQL 分桶是 `CAST((l.created_at - ?1) / ?3 AS INTEGER)`，`WHERE` 是**闭区间** `<= ?2`。
前端 `1d` 预设 = `endDate - 86400` → duration 恰为 86400 → `bucket_count = 24`（合法 0..23），
`created_at == end_ts` 的行得 `idx = 24` → clamp 成 23 → **覆盖掉第 23 个桶整小时的统计**。

**建议**：改 `map.entry().and_modify(合并)`；或 end 改半开区间；或 `bucket_count = duration/bucket + 1`。

### P1-5【不可恢复的数据空洞】"重建"不是原子的

**位置**：`commands/usage.rs:392-408`

```rust
let backup = db.backup_database_file()?;
db.reset_codex_usage()?;                              // 破坏性，独立事务
let result = sync_codex_usage(&db);                   // 独立事务，逐文件提交
finish_codex_rebuild(restore_rebuild_on_error(&db, backup, result))
```

- `reset_*_usage` 只把 `DELETE` 包在自己的 SAVEPOINT 里，重新导入随后用**独立事务**逐文件提交；
- 进程崩溃/断电 → 明细已删、重导只做一半 → **数据永久缺失**（备份文件在磁盘上，
  但**没有任何启动时恢复逻辑**）；
- 只有返回 `Err` 才走 restore，**部分成功不回滚**；
- `restore_database_file`（`backup.rs:68-81`）用 Backup API 写回**活连接**，
  会**抹掉重建期间代理侧新写入的行**（同步锁不保护代理写入路径）。

**建议**：(a) "删除 + 重导"尽量同事务（`sync_single_codex_file` 已在用
`unchecked_transaction`，把 DELETE 放进第一个文件的事务即可）；(b) 启动时检测半完成状态并回退备份。

### P1-6【游标被清空】`update_sync_state` 用 `INSERT OR REPLACE`，抹掉字节游标与尾部指纹

**位置**：`session_usage.rs:1060-1065`

```sql
INSERT OR REPLACE INTO session_log_sync (file_path, last_modified, last_line_offset, last_synced_at)
VALUES (?1, ?2, ?3, ?4)
```

`REPLACE` = DELETE + INSERT，**未列出的列回落 NULL**。这条 SQL 被 5 个解析器与 Codex 的
游标继承路径共用；只要它对一个 Claude 写过的行执行一次，`last_byte_offset` 与
`last_tail_fingerprint` 就没了 → 下一轮 Claude 按"legacy 行号游标"从文件头重算
（与 P0-1 的后果相同）。

已经在踩的地方：`session_usage_codex.rs:858-862`。

**旁证**：同文件 `:989` 有另一个**正确**的写法，显式列出了全部 6 列。
两处并存说明是漏改。

**建议**：统一改为 `INSERT ... ON CONFLICT(file_path) DO UPDATE SET last_modified=..., last_line_offset=...`，
永不触碰别的 importer 的列。

### P1-7【永久毒丸】超长单行会冻结整个文件，且每轮重复报错

**位置**：`session_usage.rs:230-235` + `:760-769` + `:944-951`

```rust
if buffer.len().saturating_add(take) > MAX_CLAUDE_LINE_BYTES {
    return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "…"));
}
```

超限返回的是 **IO 错误**（而非"跳过这一行"），于是游标停在坏行之前 + mtime 回写成旧值
→ **下一轮必然重试、必然再失败**。坏行之后的所有数据永久无法导入，且每轮同步都往
`result.errors` 追加同一条错误（前端长期报错）。

`session_usage_gemini.rs:279-283` 同款，且更糟：`return Err(...)` 让**整个文件**失败，
一个 100 MB 文件每轮都被完整重读重解析。

**建议**：超长行走"计数 + 跳过 + 游标前进"（与 JSON 解析失败同路径），错误只上报一次。

### P1-8【崩溃即损坏】备份直接写最终文件名且无完整性校验

**位置**：`backup.rs:54-59`、`:69-74`

```rust
let backup_file = ... format!("backup_{ts}_{uuid}.db");
let mut target_conn = Connection::open(&backup_file)?;   // ← 原地写最终文件名
...
backup.run_to_completion(100, Duration::from_millis(50), None)?;
```

- 进程中途被杀 → 磁盘上留下"文件名合法、内容是半截"的 db；
- `restore_database_file` 只检查 `is_file()`，会把它当**好备份**灌回去 → **库损坏**；
- 附带：每 100 页 step 一次 + 每次 `sleep(50ms)`（rusqlite 0.31 已核实），
  100 MB 库 ≈ 25,600 页 = 256 次 sleep ≈ **12.8 秒持锁**，期间所有 UI 查询阻塞；
- `prune_old_backups` 按文件名字典序保留 10 份，同秒内 uuid 随机 → **可能误删最新备份**。

**建议**：`VACUUM INTO` 或"临时文件 → `PRAGMA quick_check` → `fs::rename` 原子改名"；
恢复前先对备份跑 `quick_check`；`pages_per_step` 提到 1000+、`pause` 降到 0。

### P1-9【关窗可能卡死】`on_window_event` 在 UI 线程同步抢 10 次 DB 锁

**位置**：`lib.rs:84-95` + `settings.rs:85-118`

`get_settings_from_db` 对每个 key 各调一次 `db.get_setting`，每次都在
`database/dao/settings.rs` 取一次 `db.conn` 锁 → **一次关窗连续抢 9~10 次锁**。
而写库阶段是持锁的（Claude 单文件单事务可含 5 万条插入）。
用户点关闭 → 主线程阻塞 → 窗口"卡死"。

**建议**：关窗路径不查 DB（把设置缓存进 `AppState`），或 `spawn_blocking` + 先返回默认值。

### P1-10【前端回滚全是死代码】`autoSaveSettings` 返回对象而非 boolean

**位置**：`hooks/useSettings.ts:41`；调用方 `App.tsx:183-185, 211-213`；
判断方 `UsageDisplaySection.tsx:199, 218, 229, 237`、`UsageDashboard.tsx:312-315`

```ts
result = { requiresRestart: false };      // ← 对象，永远不等于 false
```

```tsx
onUsageDisplayChange={(usageDisplay) => { void autoSaveSettings({ usageDisplay }); }}  // 返回 undefined
```

```ts
if ((await save({ sidebarTodayItems: next })) === false) { setTodayItems(savedTodayItems); }  // 永不执行
```

**后果**：币种、汇率、侧栏项、日志列、图表类型、刷新间隔这 **6 处乐观更新一旦后端失败，
界面会继续显示"已生效"的假象**，直到下次启动读回旧设置。

**同一仓库里本来有正确写法**：`components/usage/UsagePage.tsx:17-24`
（`return (await autoSaveSettings(updates)) != null;`）——但 `UsagePage` 是**死组件**
（`App.tsx` 直接引 `UsageDashboard`），这个正确包装没人用。

### P1-11【指标污染】`input_token_semantics` 一半显式写、一半靠列默认值

显式写：`codex:2350`(TOTAL)、`grokbuild:647`(TOTAL)、`pi:962`(FRESH)、`mcode:74`(FRESH)
**未写**（依赖 `schema.rs:79` 的 `DEFAULT 0` = LEGACY）：`session_usage.rs:1313`、
`gemini:630`、`opencode:523`、`dsh:455`。

当前**没有实际错账**（已核对：gemini/dsh 存的是 cache-inclusive 输入，LEGACY 只减
`cache_read` 恰好得到 fresh；opencode/claude 是 fresh 输入但 `app_type` 不在
`CACHE_INCLUSIVE_APP_TYPES` 白名单内，走 `ELSE` 不变）。但正确性依赖
"列默认值 + 白名单 + 各家上报口径"三者恰好对齐——**任何一侧改动都会静默错账**。

**建议**：8 个写入点全部显式写死语义，schema 把该列改为 `NOT NULL` 无默认值强制显式。

### P1-12【macOS 平台事故】没有任何窗口控制按钮

**位置**：`App.tsx:177` + `tauri.conf.json`

```tsx
<WindowControlsContext.Provider value={isMac() ? null : <WindowControls />}>
```

```json
"decorations": false,
"trafficLightPosition": { "x": 14, "y": 16 },
```

mac 上 `decorations: false` 隐藏了原生红绿灯，前端又刻意不给 mac 渲染自绘按钮
（`AppPageHeader.tsx:103-107` 因此什么都不画）→ **窗口无法关闭 / 最小化 / 最大化**，
只能 Cmd+Q 或托盘退出。而 `trafficLightPosition` 是 macOS 专用项，通常需配合
`titleBarStyle: "Overlay"` / `hiddenTitle` 才生效，与 `decorations: false` 语义冲突
→ 明显是从完整版抄来、**未在 mac 上验证过**。

**建议**：二选一——(a) mac 用 `titleBarStyle: Overlay` + `hiddenTitle` 保留红绿灯；
(b) mac 也渲染 `<WindowControls />`。无论哪种都需 mac 实机确认。

---

## 3. P2 — 工程质量与性能

### 3.1 数据库层

| 问题 | 位置 | 说明 |
|---|---|---|
| `synchronous=NORMAL` 在 WAL 未生效时**不安全** | `database/mod.rs:39-44` | `PRAGMA journal_mode = WAL` 的**返回值从未校验**。若 DB 落在网络盘 / OneDrive 重定向目录，WAL 静默失败退回 delete 模式，此时 `NORMAL` 表示"提交时不同步 db 文件"→ **断电可损坏数据库**。应 `query_row` 校验返回值必须是 `wal`，否则提到 `FULL` |
| 缺 `cache_size` / `mmap_size` / `temp_store` | 全库仅 4 处 PRAGMA | 默认 `cache_size = -2000`（2 MB），而核心查询是整表聚合。建议 `-65536` / `256MB` / `MEMORY` |
| **全库无 `ANALYZE` / `PRAGMA optimize`**（已 grep 确认 0 处） | — | `sqlite_stat1` 永不存在，规划器只能靠默认选择率猜 |
| `foreign_keys = ON` 是纯装饰 | `database/mod.rs:42`；schema 中 `FOREIGN KEY`/`REFERENCES` **0 处** | 要么真加外键，要么删掉这条 PRAGMA |
| 9 个索引只有 2 个真正可用 | `schema.rs:258-313, 513-517` | `(provider_id,app_type)` 被 `(provider_id,app_type,created_at)` 前缀覆盖；`(session_id)` 被 `(session_id,app_type)` 覆盖；`(model)` 被 `LOWER()`/`COALESCE()` 包裹而失效；`(app_type,created_at)` 被 `CASE WHEN app_type='claude-desktop'...` 折叠而失效；`(data_source,created_at)` 被 `COALESCE` 包裹而失效。**有效者仅 `(created_at)` 与 rollup 主键**。每条 INSERT 却要维护 9 个索引 |
| 45 次 `ensure_column` 代替真正的版本化迁移 | `schema.rs:318-523` | `SCHEMA_VERSION = 1`，`user_version` **只写不读**（注释自己承认）。每次启动 ~46 次元数据查询；无法表达"删列/改类型/一次性回填"；无"库比代码新"的检测 |
| 回滚失败被 `let _ =` 吞掉 | `schema.rs:530-534`、`:2527` | 若 `ROLLBACK TO` 失败但 `RELEASE` 成功，会**提交一个半迁移库** |
| `BEGIN IMMEDIATE` 裸用，不可重入且会拖垮启动 | `schema.rs:2515-2531` | 顶层 `BEGIN`（未来被包进事务即报错，`usage_rollup.rs:83` 的注释说明这类坑已踩过）；`IMMEDIATE` 要拿写锁，共享库场景下阻塞 5 s 后失败，而这是 `init` 必经步骤 → **整个 app 启动失败** |
| 备份/恢复路径每次重新推导 | `backup.rs:44-48` | `Database` 不保存真实 db_path（`mod.rs:24-27` 只有 conn/cache）。若会话期间 `~/.cc-switch/cc-switch.db` 出现或消失，备份会指向另一个目录；`exists()` 为假返回 `Ok(None)` → `restore_rebuild_on_error` **什么都不回滚，安全网静默失效** |
| 恢复后不重跑迁移、不失效缓存 | `backup.rs:75-80` | 恢复整库但**不重跑** `create_tables`/`migrate_schema`/`seed`；若备份来自旧版本 → schema 降级 → "no such column" 且再也走不到迁移。`log_count_cache` 的失效条件是 `total_changes()`，而备份 API 直接写页、不经过 INSERT/UPDATE → 恢复后最多 60 s 返回旧总数 |
| `decimal_sum` 的 finalize 又转回 `f64` | `sql_fns.rs:28-30` | 文件头声称"只转一次"，但这次转换出现在**每个**聚合结果上；`get_usage_summary` 再拿两个 f64 相加。**引入 `rust_decimal` 的精度努力被抵消** |
| 成本字符串解析失败静默归 0 | `sql_fns.rs:61-64` | 空串 / `"-"` / `"N/A"` / 千分位都变 0：**合计里凭空少钱，且无任何日志**。`finalize` 的 `to_f64().unwrap_or(0.0)` 更狠——溢出时整列合计归零 |
| `Decimal` 的 `AddAssign` 溢出会 panic 并穿过 SQLite FFI | `sql_fns.rs:24` | 跨 FFI 展开属 UB。应改 `checked_add` 返回 SQL 错误 |
| `avg_latency_ms` 类型不一致 | `schema.rs:216` 声明 `INTEGER`，`usage_rollup.rs:141-145` 写入 REAL 加权均值 | 同列混存两种类型，下游 `row.get::<i64>` 遇到 REAL 会报类型错误 |
| 剪枝聚合里的逐行相关 `EXISTS` | `usage_rollup.rs:161` + `usage_stats.rs:481-522` | 每条待剪枝行探一次 ±10 分钟窗口，`COALESCE`/`LOWER` 让索引失效 → 准二次复杂度，**全程持锁**，且启动 3 秒后就在后台跑（`lib.rs:246-250`） |
| 剪枝前回填 = 全表扫描 + 全量入内存 | `usage_rollup.rs:84-86` → `usage_stats.rs:2391-2412` | `CAST(total_cost_usd AS REAL) <= 0` 无法走索引 → 全表扫；`collect::<Vec<_>>()` 把**所有**零成本行装进内存（每条含多个 String）。百万级历史库 = 数百 MB~GB 峰值，且全程持锁 |
| `rollup_and_prune(retain_days)` 不校验 | `usage_rollup.rs:62` | 负数会让 cutoff 跑到未来 → **一次性删光所有明细**。当前唯一调用点是常量 30，属潜伏风险，接设置项前必须 `clamp(1, 3650)` |
| 时区两套实现可能不一致 | 写入侧 `date(created_at,'unixepoch','localtime')`（SQLite CRT localtime，**不认 `TZ`**）vs 读侧 `chrono::Local`（认 `TZ`） | Windows 上设了 `TZ` 或跨时区移动时，两者"本地日"可能差一天 → 边界日 rollup 行重复计入或整段漏掉 |
| `settings` 每 key 一次加锁查询 | `dao/settings.rs:6-33` | `get_settings_from_db` 读 ~9 个 key = 9 次加锁 + 9 次往返，而它被 **5 秒定时器**（`lib.rs:269`）和每次关窗调用。建议一次 `SELECT key, value FROM settings` 装 HashMap |

### 3.2 会话解析层

| 问题 | 位置 | 说明 |
|---|---|---|
| `metadata_modified_nanos` 失败返回 0 → 文件**永远**不被读 | `session_usage.rs:1025-1032` | 某些 FUSE / SMB / 老 FAT 上 `modified()` 返回 `Err(Unsupported)` → `file_modified = 0` → 配合 P0-2 的 `<=` 判定，`0 <= 0` 恒成立 → 一次都不读。应返回 `Option<i64>`，`None` 时退化为只按 size 判定并 warn |
| JSON 解析失败静默 `continue`，无计数无日志 | `session_usage.rs:782-785` 等 **6 处**：`codex:1281`、`codex:1655`、`gemini:288`、`grokbuild:363-371`（`.ok()`）、`dsh:238`、`opencode:309` | schema 漂移与真实损坏表现完全一样，都是"静默少记"，且 `skipped` 计数不含它们。用户无法区分"今天没用量"和"解析器坏了" |
| DSH zstd 末帧未写完 → **整文件**解码失败，游标不推进 | `session_usage_dsh.rs:214-217` | dsh 每轮追加写独立 frame；若最后一次写入只落了半个 frame（进程被杀），`Decoder` 报错 → 函数整体 `Err` → 游标不推进 → 每轮都失败，**连前面完整 frame 里的用量也读不到** |
| `read_dir` / `DirEntry` 错误被完全吞掉 | `session_usage.rs:520-523` + `entries.flatten()` 10 余处 | 权限不足、OneDrive 占位符未下载、Windows MAX_PATH 都表现为"文件凭空消失"且零日志 |
| Codex fork 重放假定"严格前缀" | `codex:1816-1830` + `:2149-2158` | 若重放序列在第 k 个事件与父序列不一致（父被 `thread/revert` 改写、父有重复 token_count），循环 `break`，**k 之后所有父用量副本都被当成子会话新增导入** → 父线程 + 子线程各记一次 |
| Codex 每次变化**全量重解析** | `codex:1213-1244` | 永远从文件头 `read_until`，无字节级增量；进行中的 rollout 每 60 秒被整文件重解析（几十 MB） |
| `head_str` 每行都 `format!` + 朴素子串搜索 | `codex:226-231`、`:340-416` | `RequestTimer::observe_line` 对每行至少 2 次 `head_str`，每次一个 `String` 分配 + `windows().position()`。10 万行 ≈ 60 万次小分配 |
| Codex 行头扫描依赖"紧凑 JSON" | `codex:253-283` | `"payload": {`（冒号后有空格）或 `"type": "…"` 会让匹配全失 → `classify_timing_line` 返回 `None` → **不报错，只是没有 latency / first_token** |
| 整文件读入内存，DSH 峰值可达 ~600 MB | `grokbuild:257`、`dsh:211-220`（`file_bytes` ≤128 MB + `decompressed` ≤256 MB + `from_utf8_lossy(...).to_string()` **又拷一份** ≤256 MB）、`gemini:427` | Rust 分配失败是 **abort** 不是 panic → 进程直接死 |
| Codex 按行读取**无行长上限** | `codex:1240-1244` | 遇 1 GB 单行直接 OOM abort。Claude 至少有 8 MB 上限（`session_usage.rs:218-243`） |
| `u64 → u32` 裸 `as` 截断 | `codex:914-930` 等 20 余处 | `compute_delta(None, current)` 用于"第一条 token_count 无前序基线"，`total_token_usage.input_tokens` 超 2^32（长期 resume 的 codex 线程可达）会截断成任意小值 → **按错误 token 计费，并写入永不修复的错误成本**（`total_cost` 只在 `<= 0` 时才回填） |
| 模型名缺失时静默套用"看起来合理"的默认值 | `gemini:266`（默认 `gemini-3.8-flash` + 从自由文本正则抓名字）、`dsh:289-293`（默认 `deepseek-chat` / `deepseek-official`） | **成本归属会错**：用 agy 跑 Gemini Pro 的用户被按 Flash 计价。对照 `grokbuild:470` 用 `"unknown"`（查不到定价 → 成本 0）——这个方向才对 |
| `session_log_sync` 与 `session_usage_dedup` **永不清理** | 全仓只有 3 处 `DELETE FROM session_log_sync`（都是 rebuild 路径）；`session_usage_dedup` **一个 DELETE 都没有** | 删掉的会话、改名后的旧路径永久留行；dedup 增长量与历史请求数同阶（10 万请求 = 10 万行） |
| 每轮把**整张**游标表读进内存，一轮最多 6 次 | `session_usage.rs:99-122` 被 6 处调用（`codex:793-806` 还是**两次**全表） | 用一年的用户可能 10 万行游标 → 单次 ≈15 MB，一轮 ≈90 MB 瞬时分配 + 6 次全表扫描，**每 60 秒一次** |
| 每轮重新 walk 全部目录树，8 个入口各走一遍 | `session_usage.rs:455`、`codex:990`、`gemini:48`、`grokbuild:112`、`dsh:124-178`、`opencode:152`、`pi:169` | 无目录 mtime / inode 级缓存，无上一轮文件列表复用 |
| 多列语义混用（**最易踩雷的设计**） | `last_modified`：5 家写纳秒、`opencode:203` 写毫秒、`mcode:88` 写 0；`last_line_offset`：Claude 恒 0 / Codex 行号 / Gemini 消息数 / Grok 事件数 / **DSH 文件字节数** / Pi 行号 / MCode 源库自增 id；`last_synced_at`：Pi 塞了编码后的 revision | 一列多义，任何第三方写一次就会导致全量重读 |
| 逐行 autocommit | `gemini:563`、`grokbuild:543`、`opencode:444` 各自 `lock_conn!` 一行一事务 | 对照 `dsh:366-390`（单文件单事务，做对了）、`codex:2176`（分批 1000）。导入慢 5~20 倍 |
| 未支持 `CLAUDE_CONFIG_DIR` / `CODEX_HOME`，但注释声称兼容 | `config.rs:46-52` 硬编码 `home.join(".claude")` / `(".codex")`；而 `codex:566-572` 注释写"兼容用户改过 `CODEX_HOME` 后遗留的 cursor" | 这类用户会看到"会话用量全 0"且无提示。`pi_config.rs:20-26` 已是正确范例 |
| HOME 取不到时回退 `"."` | `config.rs:18-27` | Tauri 应用在 Windows 上 CWD 可能是 `C:\Windows\System32`（快捷方式/自启）→ 相对路径 + 游标 key 漂移 → 全量重导 → 与 P0-1 同样的双算路径；`get_app_config_dir()` 还会在 CWD 下建 `.cc-switch-lite`（**数据库落到 System32**） |
| DSH 插入恒 `Ok(true)`，`imported` 虚高 | `session_usage_dsh.rs:454-505` | UPSERT 的 `WHERE` 不成立时 `changes() == 0`，却无条件返回 `true`。对照 `gemini:685`、`grokbuild:699` 都用了 `conn.changes() > 0`。后果：每次全量重扫都报 `imported = N`，前端 toast 抖动 |
| DSH 首字时间在时钟倒挂时写 0 而非 NULL | `session_usage_dsh.rs:323-332` | 列语义是"推不出来为 NULL"，公共校验 `estimated_first_token_ms`（`session_usage.rs:405-412`）明确要求 `> 0`。写 0 会污染首字时延的均值/分位 |

### 3.3 命令层

| 问题 | 位置 | 说明 |
|---|---|---|
| async 命令里同步阻塞访问 DB | `commands/settings.rs:31-42`（get）/`:44-56`（set）直接 `state.db.get_setting(...)` | 在 tokio worker 上跑同步 SQLite；慢统计查询持锁时会阻塞 runtime worker。**同文件的 `get_settings`/`save_settings` 都用了 `spawn_blocking`，明显漏改** |
| `open_dir` 是**同步命令**（主线程）却做磁盘 IO + 起进程 | `commands/misc.rs:31-68` | `exists()` + 两次 `canonicalize()` 在慢盘上直接卡 UI；`Command::spawn()` 后不 wait，Unix 留僵尸 |
| `set_auto_launch` 在 async 命令里做注册表 / AppleScript 同步 IO | `commands/settings.rs:62-76` + `auto_launch.rs:58-73` | macOS 走 osascript（秒级）；且 `is_enabled → disable → enable` 先删后写，enable 失败用户已丢自启项 |
| 时间戳入参**完全无校验** → 可 OOM abort | `commands/usage.rs:71-89` 直接透传 `Option<i64>`；`usage_stats.rs:1418-1420` `Vec::with_capacity(bucket_count)` | `end_date = 8e12` → ~9000 万个 `DailyStats` → `with_capacity` 失败即 abort |
| `get_daily_trends` 对 `start >= end` 静默改写为 24h | `usage_stats.rs:1291-1293` | 同一次刷新里 summary 返回 0、trends 返回 24h 数据，**两个卡片对不上**，且前端无从察觉 |
| 错误契约不统一 + `Localized` 一定中英双显 | `error.rs:40-45` | `#[error("{zh} ({en})")]`；`AppError` 序列化成纯字符串（`:92-98`）**丢掉 `key`** → 前端 `t("usage.invalidPrice")`（`PricingEditModal.tsx:60`）永远拿不到后端 key，两套本地化并行且互不生效 |
| `get_model_pricing_sync` 用 `unwrap_or(false)` 吞 `sqlite_master` 错误 | `commands/usage.rs:187-198` | 真错误伪装成"表不存在"→ 返回空列表 + `Ok`，前端只看到"没有定价" |
| `rollup_and_prune` 是 poison pill | 聚合直接插 `d`（`date(...)` 对非数值 `created_at` 返回 NULL）、`l.model` 为 NULL | 违反 `NOT NULL` → 整批回滚 → **每日一次、只打 WARN** → 明细永远不剪，库无限膨胀 |
| 备份时持连接锁做整库拷贝 | `backup.rs:56-62` | 几百 MB 的库会让所有读查询阻塞到备份结束 |

### 3.4 前端性能与体验

| 问题 | 位置 | 说明 |
|---|---|---|
| 全应用 `select-none`，**一个字都复制不了** | `App.tsx:178`（全仓无任何 `select-text` 逃生口） | `RequestDetailPanel.tsx:342` 那段显然是**专门为长错误文本设计**的（`whitespace-pre-wrap break-words`），却选不中；requestId / providerId / 定价数字同理 |
| 请求日志翻页两个问题 | `RequestLogTable.tsx:183-207` | ① 用 `useEffect` 在**渲染后**才 `setPage(0)` → 换筛选时先用旧页码发一次请求，再发 page 0，**每次筛选多一次无用 invoke**；② `page` 缩小后不收敛（`:195` `totalPages` 变小时 `page` 仍是 5 → 空表 + "6 / 3"）。对照 `TablePagination.tsx:113` 的客户端分页**是有 clamp 的** |
| 趋势图 `useMemo` 形同虚设 | `UsageTrendChart.tsx:197, 215-224` | 渲染体里 `resolveUsageRange(range)` 的 `endDate = Date.now()/1000` **每次渲染都变** → memo 依赖恒变 → **每次渲染都重建数组**，每点还要 2~3 次 `toLocaleDateString`。而父级有 30 s ticker + ResizeObserver 频繁触发 |
| 30 秒 ticker 重渲染整个 1005 行看板 | `UsageDashboard.tsx:129-137, 271` | `now` 只被 `syncedLabel` / `drawerSyncedLabel` 使用，却在**顶层**产生 → 每 30 秒整页（Hero + 趋势图 + 热力图/表格 + 筛选行）全部重渲染，子组件无 `memo` |
| 表格热路径：全列预构造 + twMerge + 每调用新建 Intl | `RequestLogTable.tsx:296-497`、`format.ts:22, 357` | 不论隐藏了多少列，每行都构造全部 14 个 `<td>`；20 行 × 14 列 ≈ **280 次 `cn()`（twMerge 是 parser）+ 数十次 `new Intl.NumberFormat`**（无缓存） |
| 热力图 hover 触发全网格重渲染 | `UsageHeatmap.tsx:383-404` | 单个格子 `mouseenter` → `setHover` → 整块 53×7 网格重跑 + 371 次 `cn()` |
| 热力图 `heat` memo 缺 `today` 依赖 | `UsageHeatmap.tsx:266-283` | 应用可长期驻留托盘（`minimizeToTrayOnClose` 默认 true），跨午夜后 `future` 标记、月份标签、阈值分档**停在昨天** |
| `models.dev` 自动同步**静默覆盖手改定价** | `modelsDevAutoSync.ts:57-66` | 凡在选中集合（含 `includeCommonModels` 默认 6 个常见模型）里的 modelId，每 6 小时被远端价格整条覆盖，**包括用户手改过的值，且无任何提示** |
| 设置保存多一次往返 | `lib/query/index.ts:25-32` + `useSettings.ts:39-40` | `onSuccess` 无条件 `invalidateQueries(settingsKeys.all)`，而 `autoSaveSettings` 紧接着 `setQueryData(merged)` → **每次保存 = 1 次 save + 1 次 get**，且 setQueryData 的值又被 refetch 覆盖 |
| `useTauriEvent` 在渲染期改 ref | `hooks/useTauriEvent.ts:12-13` | `handlerRef.current = handler` 在渲染期间赋值，React 18 StrictMode/concurrent 下渲染可能被丢弃。应放进 effect |
| 定价显示四舍五入到 `$0.00` | `PricingConfigPanel.tsx:64-70` | `toFixed(4)` 把 < 0.00005 的价格抹成 `$0.00`，把 0.00005~0.00009 抹成 `$0.0001`（放大约 2 倍）。缓存命中价常见的 `$0.00002/M` 会直接显示为免费 |
| 格式化边界 | `format.ts:126-130, 190-193, 436-458` | `formatMoney(-1.5)` → `$-1.50`（应为 `-$1.50`）；`formatMoneyTick` 直接 `String(num)` → 会出 `0.30000000000000004` / `1e-7`；日文分支用了**简体** `亿/万`（应为 `億/万`）；`formatTokensShort` 的 `toLocaleString()` 不传 locale |
| 零散正确性 | `UsageHero.tsx:271`（`fmtInt(cacheWrite)` 漏传 locale）、`statsColumns.tsx:86`（`successRate.toFixed()` 无 null 兜底）、`RequestLogTable.tsx:218-231`（`headerHelp` 每行重建） | — |

### 3.5 i18n

| 问题 | 位置 | 说明 |
|---|---|---|
| **ja / zh-TW 各缺 2 个 key** | `usage.windowBehavior.trayHint`、`usage.windowBehavior.autoLaunchHint`；使用处 `WindowBehaviorSection.tsx:98, 102` | 实测：en 4222 / zh 4222 / zh-TW 4221（+1 孤儿 key `settings.oneClickInstall`）/ ja 4220。这两句正好在主看板「设置」页签，日文/繁中用户会看到**英文**（`fallbackLng: "en"` 静默降级，构建期不报错） |
| 唯一的高频失败提示**硬编码中文** | `hooks/useSettings.ts:44` | `toast.error("保存设置失败: " + message)` —— 语言、主题、刷新间隔、窗口行为、币种、列配置**全走这里**，且 `useSettings` 连 `useTranslation` 都没引。对照 `App.tsx:81-87` 的正确写法可看出是漏改 |
| 其他硬编码 | `App.tsx:130` `title="Language"`、`:158-163` `"Switch to light"` 等 | — |
| locale 严重冗余 | 4 份 × ~4200 key = 19537 行 JSON | 按 leaf key 统计，**3903 / 4222 个 key 未被代码直接引用**（`settings` 561、`sessionManager` 252、`providerForm` 205、`skills` 182、`mcpPage` 147、`mcp` 124 …），全是 Lite 已删除的完整版模块留下的 |
| `t(key, "中文默认值")` 掩盖缺失 | `PricingEditModal` / `UsageDateRangePicker` / `usageRange.ts` 大量使用 | 用法本身正确，但把中文兜底文案也打进了 bundle，且**掩盖 key 缺失** |
| `index.html` 硬编码 `<html lang="zh-CN">` | `src/index.html:2` | 仅影响首帧（`i18n/index.ts:100-104` 随后会改对） |

### 3.6 工程 / 打包

| 问题 | 位置 | 说明 |
|---|---|---|
| **首屏 2.15 MB JS，其中约 1.1 MB 是四份语言包** | 实测 `dist`：`main.js` **1,540.57 KB** + `recharts` 385.05 KB + `radix` 276.67 KB；已 grep 确认 `main.js` 里**同时包含**四份 locale 的全部文案（`先に設定を保存してください`、`繁體中文` 均可命中） | `i18n/index.ts:4-7` 四条静态 `import ... from "./locales/*.json"` 全部进主 chunk；`vite.config.ts:17-31` 的 `manualChunks` 只分了 recharts / radix。**locale 懒加载一项即可砍掉约 800 KB** |
| 图标全量内联 | `main.js` 含 96 个 SVG；另有 38 张第三方 provider 图标 1.03 MB 落盘（Lite 已无供应商管理页） | `icons/extracted/index.ts` 是 177.6 KB 的 `Record<string,string>` 字面量，Rollup 无法对单 key tree-shake；本应用实际只用 9 个 app 图标 |
| 无 sourcemap | `dist` 实测 **0 个 `.map`** | `frontendLogger` 已做脱敏，可安全开 `sourcemap: "hidden"` |
| 未设 `build.target` | `vite.config.ts` | Vite 7 默认 `baseline-widely-available`，WebView2/WKWebView 可显式对齐 `es2020` |
| **无 ESLint** | `package.json` 无 `eslint` 依赖，仓库无配置 | 但源码里有 3 处 `// eslint-disable-next-line react-hooks/exhaustive-deps`（`App.tsx:66`、`UsageDisplaySection.tsx:186`、`UsageHeatmap.tsx:172`）→ **抑制注释已失效，`exhaustive-deps` 规则早就不跑了** |
| **前端零测试** | `src` 下 0 个 `*.test.* / *.spec.*`，无 vitest 依赖 | `format.ts` / `usageRange.ts` / `buildUsageTrendChartData` 都是纯函数，最值得测 |
| CI 两端 Rust 版本不一致 | `rust-toolchain.toml:2` = **1.95**；`.github/workflows/release.yml` = `dtolnay/rust-toolchain@stable` | checks 用 1.95、release 用 stable → 可能出现"CI 过了但发布构建失败"（或反之） |
| **release.yml 中文全是乱码** | `.github/workflows/release.yml` 的 `releaseBody` | 实测：`CC Switch 鏋佺畝杞婚噺鐗?..`——UTF-8 被按 GBK 解码后回写。每次发版，GitHub Release 说明页都是乱码。**这是发布流程里最显眼的对外事故** |
| `tsconfig.node.json` 实际从未被检查 | `tsconfig.json:23` 是 composite references，但 CI 跑的是 `tsc --noEmit`（不带 `-b`） | → **`vite.config.ts` 从未被类型检查** |
| CSP 偏宽 | `tauri.conf.json:30` `script-src 'self' 'unsafe-inline'` + `connect-src ... https: http:` | 内联脚本可改外部文件/加 hash；`connect-src` 可收敛到 `https://models.dev` |
| 窗口可见性完全依赖前端 JS | `tauri.conf.json` `"visible": false` + `App.tsx:36-44` | 1.5 MB 主 chunk 只要解析/执行/首帧任一步失败，**窗口永远不出现，只剩托盘**——用户会以为"应用打不开"。建议后端在 setup 里延迟 N 秒未见 ready 就强制 `show()` |
| `minWidth: 900` 让响应式分支不可达 | `tauri.conf.json` vs `UsageDashboard.tsx:274-276` | `compact = width < 720` **永远为假**、`showSyncText = width >= 780` **永远为真** → 双列布局与 tooltip 兜底分支都是死代码 |
| `trafficLightPosition` 与 `decorations: false` 自相矛盾 | `tauri.conf.json:18, 17` | 见 P1-12 |
| README 与实现不一致 | `README.md:24, 22` | 宣称支持 **GBP** + "**实时**汇率折算"，实际 `format.ts:27-34` 是 `USD/CNY/EUR/JPY/TWD/TRY`（**无 GBP**），`:42-45` 汇率**写死**；宣称"按状态码筛选"，实际只有 5 个固定值 |

---

## 4. 死代码与冗余（Lite 裁剪未清扫）

### 4.1 前端（已用引用分析核实）

| 文件 / 符号 | 状态 |
|---|---|
| `components/usage/UsagePage.tsx` | **全文件无人 import**（`App.tsx` 直接用 `UsageDashboard`），却包含 P1-10 中**唯一正确**的 save 包装 |
| `lib/unsavedChanges.ts` | `hasUnsavedChanges` / `discardUnsavedChanges` 无消费者；`useUnsavedChangesTracker` 仅被 `sheet.tsx` 的 `SheetPageContent` 用，而后者也无人用 |
| `components/common/FullScreenPanel.tsx:18` | 声明了 `trackUnsavedChanges` 但既不解构也不实现；`PricingEditModal.tsx:91` 传了它 → **"离开前提醒未保存"实际不存在** |
| `utils/deepClone.ts`、`utils/formatters.ts` | 无引用 |
| `utils/errorUtils.ts:40-129` | `translatePiProviderMutationError` / `translateMcpBackendError` 无引用，且匹配的是**已从 Lite 移除的 MCP/Pi 中文错误**（27 处中文字面量） |
| `lib/api/proxy.ts`、`lib/api/index.ts` | 无引用（`proxyApi` 无人用） |
| `hooks/useUsageSummary`（`lib/query/usage.ts:174`） | **已核实仅定义、无调用**（各组件用的是 `useUsageSummaryByApp`） |
| `components/ui/`：`alert` `badge` `card` `ime-safe-input` `scroll-area` `search-field` `table` `tabs` `textarea` `toggle-row` | 均**未被任何文件 import** |
| `components/usage/format.ts:401` 的 `tr` 分支 | 无土耳其语 locale |
| `icons/extracted/metadata.ts`（25.8 KB，1000+ 行中文 displayName/keywords） | 被 `ProviderIcon` → `AppGlyph` 拉进依赖图，而本应用只用 9 个图标 |

> 说明：`UsageHero` / `UsageTrendChart` / `UsageHeatmap` / `RequestLogTable` /
> `ModelStatsTable` / `ProviderStatsTable` / `PricingConfigPanel` / `RequestDetailPanel` /
> `UsageDataSourcesSheet` / `UsageDisplaySection` / `WindowBehaviorSection` /
> `TablePagination` / `statsColumns` / `requestLogColumns` / `usageTable` / `providerLabel` /
> `format` / `rememberedChoice` 等**是活的**——它们由 `UsageDashboard.tsx` 以相对路径
> `./Xxx` 引入，粗粒度的 `@/` 匹配会漏判。

### 4.2 后端

| 符号 | 状态 |
|---|---|
| `check_provider_limits` 整链 | `usage_stats.rs:2265-2351` + 结构体 `:2354-2365` + 命令 `commands/usage.rs:308-318` + 注册 `lib.rs:321` —— **前端零引用**（已核实无 `checkProviderLimits`），`providers.meta` 的 `limitDailyUsd/limitMonthlyUsd` 也无 UI。且实现本身有 5 个问题：未走去重过滤（代理+会话双计）、`CAST AS REAL` 累加、查询失败吞成 0（**fail-open**）、非 sargable 全表扫、`"NaN"` 能 parse 且 `>= NaN` 恒 false |
| 注册但前端从未 invoke 的命令 | `check_provider_limits`、`get_session_usage_summary`、`get_usage_data_sources`、`open_dir`、`rebuild_dsh_usage`、`rebuild_gemini_usage`（**34 个命令中 6 个无前端调用方**） |
| `CostCalculator::try_calculate_for_app` | `calculator.rs:122-129` 无人调用 |
| 三个完全相同的 `finish_*_rebuild` | `commands/usage.rs:385-390 / 410-415 / 442-447` |
| `find_*_pricing` 一行包装 ×3 | `session_usage.rs:1357-1362`、`gemini:690-692`、`codex:2391-2393` 函数体完全相同 |
| `sqlite_table_exists` ×3 | `codex:574-581`、`gemini:694-703`、`opencode:80-87` |
| 空壳 shim 模块 | `proxy/mod.rs`（只剩 `pub mod usage`）、`gemini_config.rs` / `codex_config.rs` / `opencode_config.rs` / `codex_rollout_file.rs` / `mcode_config.rs`（全是 `pub use` 转发）、`claude_desktop_config.rs`（只剩 1 个常量）、`session_manager/providers/mcode.rs` 的 `scan_sessions`/`load_messages` 是 `#[cfg(test)]` 空实现 |
| **未提交的工作区改动** | `git status` 显示 **30 个文件已修改未提交**，且 `Cargo.toml` 里已删除 `reqwest` / `regex`（源码中确认无任何引用）→ 处于"重构已完成但未落库"的中间状态 |

### 4.3 重复代码（7 个 provider 之间的抽象机会）

约 7,500 行逻辑中，至少 1,500 行是同一模板的第 N 次复制：

1. **"取 metadata → mtime 门 → 早退"**：`session_usage.rs:659-671`（`<=`，无 size）、
   `codex:1964-1979`（`==` + size）、`gemini:411-424`、`gemini:236-248`、`grokbuild:232-251`、
   `dsh:193-209`、`pi:245-267`、`opencode:110-132` —— **7 份实现、3 种语义**（P0-2 就是这么来的）。
2. **`INSERT INTO proxy_request_logs (…24 列…)`**：8 份几乎相同的列清单，只有 `ON CONFLICT`
   策略不同。**新增列漏改已经发生**：`input_token_semantics` 后来加入，4 处写了、4 处没写（P1-11）。
3. **"定价 → 5 个成本字符串 → 全 0 兜底"**：7 份。且**只有 Claude 与 Codex 做了定价缓存**，
   Gemini / Grok / DSH / OpenCode / Pi **每行一次 `SELECT`**。
4. **延迟估算各写一套**：Claude 的 `estimated_latency_ms` / `estimated_first_token_ms`
   （有合理区间校验）只被 Codex 与 Pi 复用；`gemini:340-367`、`gemini:487-506`、
   `dsh:312-332`、`opencode:409-422`、`grokbuild:476-484` 各写一套
   → **同一个"首字时延"指标在 6 个 provider 上语义不同，Dashboard 横向对比没有意义**。
5. **`created_at` 解析 + `SystemTime::now()` 兜底**：6 份。且 `unwrap_or_else(now)` 本身有问题
   ——时间戳缺失时用"当前时间"会让这些行**永远落在最近 30 天内、逃过 rollup 边界**。

**建议的抽象边界（两件就够）**：
- `trait SessionSource { fn roots(); fn collect(root); fn parse(path, &SyncCursor) -> ParseOutcome; }`，
  由 `sync_all_unlocked` 统一驱动 scan / cursor / 错误聚合 / 结果合并；
- `struct UsageRow` + `fn insert_usage_row(conn, &UsageRow, ConflictPolicy)`，把 24 列 SQL、
  `input_token_semantics`、`created_at`、5 个成本列全部收口，并共享一份定价缓存。

这两件能把 7,500 行压到约 4,500 行，并**直接消灭 P0-2 / P1-6 / P1-11 这一整类
"某一家写了、某一家没写"的 bug**。

---

## 5. 推荐修复路线图

### 第 1 批（数据正确性，不可逆风险最高）
1. **P0-1** `is_gemini_cursor_path` → 加 `importer` 列，消除误删游标导致的 rollup 双算。
2. **P0-2** Claude 未变化判定加上 size 比较（约 5 行）。
3. **P0-3** Gemini `message.id` 缺失时的稳定回退键。
4. **P1-1** `rebuild_rollup_primary_key` 改用归一化后的 `fresh_input_sql`。
5. **P0-5** `lock_conn!` 改为 `into_inner()` 容错 + 定价数值上界。

### 第 2 批（功能可用性）
6. **P0-4** 补上 `useUsageAutoRefresh` 的定时器与首扫（并统一刷新节拍）。
7. **P1-10** `autoSaveSettings` 返回 `boolean`，让 6 处回滚生效。
8. **P1-12** macOS 窗口按钮（需实机验证）。
9. **P1-4** 小时分桶改累加 + 闭/半开区间统一。
10. **P1-2** 加 `pricing_status` 列，停止读路径写库。

### 第 3 批（健壮性）
11. **P1-5 / P1-8 / P2 备份组**：重建原子化、备份用 `VACUUM INTO` + `quick_check` + rename、
    `Database` 持有真实 db_path、恢复后重跑迁移。
12. **P1-6 / P1-7 / P1-11**：游标 UPSERT 改 `ON CONFLICT DO UPDATE`、超长行改为跳过、
    8 个写入点显式写语义。
13. **P2 输入校验**：时间戳 `clamp`、`retain_days` `clamp`、`u64 → u32` 饱和。
14. **P2 解析健壮性**：JSON 解析失败计数 + warn、`read_dir` 错误上报、Codex 行长上限。

### 第 4 批（性能与工程）
15. 数据库：补 PRAGMA（`cache_size` / `mmap_size` / `temp_store`）+ 引入 `ANALYZE` / `PRAGMA optimize`；
    校验 WAL 是否真的生效；删 4 个冗余/失效索引；回填与剪枝改流式 + 独立连接。
16. 前端：locale 懒加载 + 图标按需（预期 −1 MB）；`Intl.NumberFormat` 按 locale 缓存；
    30 s ticker 下沉；表格只构造启用列 + 行 `memo`。
17. 工程：修 `.github/workflows/release.yml` 乱码 + Rust 版本对齐；加 ESLint + react-hooks；
    加 vitest 覆盖纯函数；`tsc -b` 覆盖 `vite.config.ts`；CI 校验 locale key 一致性；
    修 `src/types/usage.ts` 乱码注释；README 与实现对齐。
18. **代码抽象**：`SessionSource` + `UsageRow`（第 4.3 节），这是上面多类 bug 的**生产机制**，
    越早收口越省事。

---

## 6. 已核实"没问题"的点（避免二次排查）

- 生产路径 **0 处** `unwrap() / expect() / panic!`（脚本全量扫描 10 个大文件非测试区）。
- `session_sync_mutex` 是 `tokio::sync::Mutex`，跨 `spawn_blocking().await` 持锁**合法且必要**；
  未发现"持 tokio 锁再持 std 锁并反向等待"的循环 → **无死锁**。
- `sync_single_file` 的 `unchecked_transaction()` 在早退 / `?` 时靠 `Transaction` 的 Drop 回滚，**正确**。
- `rollup_and_prune` 的**主流程边界对齐写得比一般实现更严谨**：截止点按本地日边界对齐
  （含 DST gap / ambiguous 处理）、聚合与删除用同一 cutoff、同 SAVEPOINT、LEFT JOIN 不 fan-out。
- 剪枝与游标推进共用同一把锁（`lib.rs:47-55`），不会互相撕裂；rollup 只删 detail
  不删 `session_log_sync`，因此不会因剪枝触发重导。
- **SQL 无注入点**：所有用户输入都走参数绑定；`format!` 只插入常量别名/列名与
  `saturating_*` 处理后的 i64 字面量。
- **前后端 DTO 契约正确**：Rust 侧全部 `#[serde(rename_all = "camelCase")]`，与 `src/types/*`
  逐字段一致，无 snake_case / camelCase 错配。
- `tsc --noEmit` 通过（exit 0）、`cargo check` 通过、`cargo clippy` 仅 3 个警告
  （`session_usage_dsh.rs:314` 数字下划线分组、`:413/:427` 多余的 `&`）。

### 需要留意的两处"单位陷阱"（契约正确但易踩反）

- `UsageSummary.successRate` 是 **0–100**（`usage_stats.rs:77-81` 乘了 100），
  而 `cacheHitRate` 是 **0–1** —— 同一个接口两种量纲，**只写在 Rust 注释里**。
  任何人下次把 `successRate` 传进 `formatHitRatePercent`（会再 ×100）就会显示 `5000%`。
- `src/types/usage.ts:1, 17, 31, 91, 116-165` 的中文注释是 **GBK↔UTF-8 双重编码乱码**
  （全仓唯一有此问题的文件，且 163-165 行把换行也吃掉了，JSDoc 排版已坏）。
