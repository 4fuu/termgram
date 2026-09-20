# 搜索消息

[English](../en/Search.md) · [指南首页](Home.md)

默认在聊天内容区按 `/`，或在聊天列表按 `Ctrl-F`，打开本地搜索；`:search [正则]`
明确选择本地搜索。输入正则表达式后按 Enter。Tab 在当前
聊天、当前文件夹、当前账号三个范围间切换。搜索可以离线使用，不会向 Telegram
请求历史记录。标题显示范围，结果显示缓存消息数与日期范围。这些数字只表示缓存
覆盖情况，不代表已下载完整历史；日期范围内也可能有缺口。

结果从新到旧排列，每页 100 条命中。Up/Down 选择结果，PageUp/PageDown 移动十行，
Enter 打开消息及其缓存前后文。Ctrl-N/Ctrl-P 翻页，Ctrl-F 返回查询输入，Esc 关闭。
在聊天中再次按 `/` 可回到保留的结果。`G` 从历史结果回到最新消息。打开历史搜索
结果本身不会把聊天中尚未阅读的消息全部标为已读。

示例：`(?i)invoice` 忽略大小写，`error.*timeout` 匹配两个词之间的内容，`^TODO`
匹配开头，`东京|上海` 匹配任意一个词。语法采用维护中的 Rust
[regex 库](https://docs.rs/regex/latest/regex/#syntax)，不支持环视和反向引用。错误的
表达式会显示可修正的错误提示；表达式最多 4096 字节，编译内存也有上限。匹配消息
文字和附件说明，不索引附件内部内容。

扫描在独立后台工作线程中进行，同时最多一个扫描和一个可替换的待执行请求。Esc、
修改查询或切换账号会取消扫描。重新搜索会反映已写入缓存的编辑和删除。打开命中项
前会再次检查缓存；消息已删除或被清理时，会提示重新搜索。

在已打开的聊天中使用 `:search --cloud` 进入 Telegram 云端搜索。同一面板会标明
**Telegram search**、捕获的聊天、当前筛选条件及 Telegram 返回的结果数量。文字使用
Telegram 的词语搜索规则，不是 Rust 正则。云端模式需要网络连接，只搜索指定聊天，
Tab 不切换范围。按 Esc，再使用 `:search --cloud ...` 可修改筛选条件或目标聊天。
`/` 返回保留的结果，包括云端结果；`:search` 返回本地正则搜索。

示例：

```text
:search --cloud release notes
:search --cloud --from me --media file
:search --cloud --from @alice --after 2026-09-01 --before 2026-10-01 release
:search --cloud --media photo
:search --cloud -- --literal-leading-dash
```

筛选参数放在查询文字前。`--from` 接受 `me`、`@username` 或已知的 Telegram peer ID；
用户名在一次搜索中只解析一次，后续翻页保持同一发送者身份。`--media` 支持 `all`、
`photo`、`video`、`file`、`music`、`voice`、`round`（圆形视频）、`gif`、`link`、`poll`。
日期为 `YYYY-MM-DD`，按 **UTC 零点**计算：`--after` 严格晚于该时刻，`--before`
严格早于该时刻。日期必须落在 Telegram 的 32 位时间戳范围内；界面接受
1970-01-02 至 2038-01-19。筛选使用
[Telegram messages.search API](https://core.telegram.org/method/messages.search)。
只有云端参数使用现有的 Yazi 平台引号规则（Unix shell 引号或 Windows 命令行引号），
不会执行 shell。本地正则参数仍保留反斜杠和末尾空格。

云端每页最多 100 条，通过有界预取判断下一页。只有筛选条件、没有文字的命令会先打开
输入框；按 Enter 可直接筛选，也可输入文字后再搜。Telegram 拒绝查询时，保留查询并
允许修改。在结果上按 Enter 会加载原消息及其之前最多 79 条消息，然后定位原消息，
不会因此把尚未阅读的历史全部标为已读。`G` 回到最新消息。正文与附件草稿均保留。

Esc 或新搜索会取消旧云端任务，迟到的结果不会替换新查询。显示和持久化之前，云端结果
会与期间收到的编辑、删除合并。结果写入同一个有上限的缓存，不推进同步游标。已有结果
反映实时编辑和删除；需要刷新匹配项和服务端计数时请重新搜索。聊天历史变化时，数量和
分页并非冻结的快照。历史重新同步会使旧结果失效并提示重搜。缓存仍受原有容量限制，
云端搜索不会下载整个聊天。

所有操作都可在 Lua 的 `search` 上下文中配置：`open`、`cancel`、`up`、`down`、
`page_up`、`page_down`、`search_scope`、`search_query`、`search_more`、
`search_previous`。

Ctrl-R（`refresh`）从第一页重新搜索。`g m` / `:mentions` 复用同样的翻页和原消息跳转，
用于[未读提及](Notifications.md)。
