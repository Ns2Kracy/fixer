# Fixer 媒体刮削器设计

**日期：** 2026-08-24  
**状态：** 已批准

## 1. 目标

Fixer 是一个专注于元数据刮削、匹配、合并和输出的本地优先工具。它面向个人电脑和 NAS 的单用户、自托管场景，同时提供：

- 运行时无关的 Rust 核心；
- 人体工学的 Rust SDK；
- 基于 Clap 的独立 CLI；
- 基于 Axum 的本地 Web 服务；
- SolidJS 2.0+、Tailwind CSS 和 TanStack Router 构建的 Web 工作台。

首版覆盖电影、电视剧、动漫、音乐和书籍。统一公共流水线，但保留每个领域的类型化语义。真实 Provider 按媒体类型逐步交付，不一次铺开。

Fixer 不承担媒体播放、下载管理或云端账户服务。

## 2. 产品原则

1. **本地优先：** 默认在本机或 NAS 运行，不依赖 Fixer 官方中转服务。
2. **核心纯净：** Core 不包含数据库、Web 框架、CLI 框架、配置加载或任务队列。
3. **默认简单：** SDK 的常见用法不要求用户理解 Transport、缓存或任务系统。
4. **类型安全：** 媒体领域、Provider 和 Writer 使用 Rust 类型表达，不以动态 JSON schema 取代领域模型。
5. **无损合并：** 保留语言版本、字段来源、冲突和置信度，不在早期破坏性覆盖数据。
6. **显式副作用：** 搜索、匹配与合并不写文件；所有写入先生成可审查的 `OutputPlan`。
7. **部分成功：** 单个 Provider 不可达不应拖垮整个任务。
8. **安全默认：** 默认不覆盖现有文件，非回环 Web 监听默认强制认证。
9. **小步交付：** 直接在 `main` 上按独立、可验证的变化提交，不使用 worktree。

## 3. 总体架构

采用分层模块化单体和 Cargo workspace：

```text
fixer/
├── Cargo.toml
├── crates/
│   ├── fixer-core/
│   ├── fixer-sdk/
│   ├── fixer-http/
│   ├── fixer-provider-local/
│   ├── fixer-writer-local/
│   ├── fixer-cli/
│   └── fixer-server/
├── web/
└── docs/
```

网络 Provider 随垂直切片按需新增，例如：

- `fixer-provider-tmdb`
- `fixer-provider-bangumi`
- `fixer-provider-anilist`
- `fixer-provider-musicbrainz`
- `fixer-provider-openlibrary`

不预先创建没有实现的空 crate。

### 3.1 `fixer-core`

Core 是运行时无关的纯领域层，包含：

- `Work / Release / Asset` 三层模型；
- 五类媒体的类型化模型；
- `MediaHint`、候选、置信度和匹配证据；
- BCP 47 本地化值和 Locale 投影策略；
- 字段来源、冲突、完整度和合并策略；
- `Provider`、`HttpClient`、`Writer`、`TemplateRenderer` 等 trait；
- `OutputPlan`、放置方式和写入安全模型；
- 结构化错误及警告。

Core 不依赖 Tokio、Reqwest、Axum、Clap 或 SQLite，不读取应用配置，不初始化日志，不拥有全局状态。异步接口表达标准 Future，不启动运行时或后台任务。

### 3.2 `fixer-sdk`

SDK 使用 Tokio 编排官方高级流程，但底层 Core 保持运行时无关。SDK 提供带合理默认值的门面：

```rust
let fixer = Fixer::builder()
    .provider(Tmdb::from_env()?)
    .provider(LocalMetadata::new())
    .preferred_languages(["zh-CN", "zh-TW", "en"])
    .build()?;

let outcome = fixer
    .movie("花样年华")
    .year(2000)
    .resolve()
    .await?;
```

常见调用不暴露内部上下文对象、数据库或任务队列。高级调用者仍可替换 HTTP Client、评分器、字段合并策略、Locale 策略、Provider 选择策略、Writer、模板渲染器及进度接收器。

### 3.3 编译期 Provider

Provider 是普通 Rust crate，通过公开 trait 和 builder 注册。首版不支持动态库、外部进程、WASM、运行时插件下载或全局注册表。第三方可以发布 Provider crate，用户通过构建自定义发行版使用。

Provider 负责定义请求、解析响应和标准化数据；网络访问由注入的 `HttpClient` 完成，便于测试和替换。

### 3.4 应用层

- `fixer-cli`：Clap、配置加载、批处理、交互确认、终端进度与稳定 JSON 输出；
- `fixer-server`：Axum API、单用户认证、SQLite 任务、后台 worker、SSE 进度；
- `web`：任务工作台，不是媒体播放器；
- SQLite 仅属于 Server 应用层，不进入 Core 或 SDK。

## 4. 领域模型

统一采用三层模型：

1. **Work：** 抽象作品本身；
2. **Release：** 某地区、语言、日期、剪辑或载体的发行版本；
3. **Asset：** 用户实际拥有的视频、音轨、电子书、字幕、图片或目录。

领域实体保留独立语义：

- 电影及其不同剪辑、地区发行；
- `Series → Season → Episode`；
- 动漫季度、OVA、特别篇、绝对集数和播出顺序；
- `Artist → Album/Release → Disc → Track`；
- `BookWork → Edition → File`，版本可关联 ISBN。

公共能力统一，但不创建一张万能媒体表。类型系统应阻止把 ISBN、季集编号等字段用于错误的媒体类型。

## 5. 语言与地域

所有本地化值无损保留，使用 BCP 47 标签。标题、别名、简介等可以同时保存简体中文、繁体中文、日文、英文、原始标题和拉丁转写。搜索、匹配、Web 展示与输出可以使用各自的 `LocalePolicy`。

Unicode 规范化用于搜索与比较，但不改变 Provider 返回的原始展示值。

网络模型保持最小：

- 默认直连且零配置；
- 自动尊重 `HTTP_PROXY`、`HTTPS_PROXY`、`ALL_PROXY` 和 `NO_PROXY`；
- SDK、CLI 和 Web 暴露一个全局代理设置；
- Provider 可覆盖 API endpoint；
- 离线模式只跳过需要网络的 Provider；
- 高级用户可注入自定义 `HttpClient`。

首版不实现地区自动探测、路由图、镜像池、自动多级回退、官方中转服务、离线数据包或复杂缓存状态机。Google Books、AniList 等站点不作为任何地区的必达依赖。

## 6. 核心数据流

```text
Input → Identify → Search → Rank → Fetch → Merge → Plan → Execute
```

### 6.1 Input 与 Identify

显式标题、年份、ISBN、外部 ID，文件或目录扫描，媒体内嵌标签，旁车文件和用户提示统一转换为类型化 `MediaHint`。

识别器只提取证据和提示，不直接决定最终实体。文件名、目录、标签和旁车来源均记录证据，供 CLI 和 Web 解释。

### 6.2 Search 与 Rank

SDK 按媒体类型选择 Provider，并使用 Tokio 并发搜索。离线模式跳过网络 Provider。Provider 失败转化为结构化警告并继续处理其他来源。

候选评分考虑：

- 精确外部 ID；
- 标题、原始标题和别名相似度；
- 年份或发行日期；
- 季、集、Disc、Track 等序号；
- ISBN、条码等领域标识；
- Provider 提供的原始置信度。

评分结果保留可解释的正负证据。阈值由应用层配置，不写死在 Core。

### 6.3 Fetch 与 Merge

只获取选中候选的完整详情。每份部分元数据附带 Provider、外部 ID、语言、抓取时间和置信度。

合并结果包含：

- 类型化最终值；
- 字段级来源图；
- 字段冲突；
- 完整度；
- 警告。

合并规则支持全局、媒体类型和单字段三个层级。标题按语言及类型合并；人员按外部 ID 或规范化姓名去重；评分按评分系统分别保留；图片按类型、语言、尺寸及来源排序；剧集按所选编号体系匹配。

### 6.4 Plan 与 Execute

Writer 接收已解析数据并只生成 `OutputPlan`。计划可预览、比较和确认；只有显式执行才产生副作用。

执行前必须：

- 检查源与目标自计划生成后是否变化；
- 防止目标路径逃逸允许的媒体根目录；
- 应用明确的覆盖策略；
- 使用同目录临时文件和原子替换；
- 返回逐项执行报告。

高置信度任务可以配置自动执行；中低置信度或冲突项进入确认队列。批处理默认只自动处理高置信度结果。默认不覆盖现有文件。

## 7. 输出与媒体放置

元数据输出与媒体放置是两个独立维度。

### 7.1 元数据输出

内置输出逐步提供：

- JSON、YAML、通用 XML；
- 标准化结果及来源追踪 manifest；
- Kodi/Jellyfin/Emby 兼容 NFO；
- Plex 可识别的目录与资源命名；
- 图片、封面、背景、Logo、歌词等旁车资源；
- 音乐标签写入计划；
- 图书 OPF、旁车文件及 EPUB 元数据更新计划。

路径模板与内容模板支持用户生成任意文本格式；Rust `Writer` trait 用于复杂、多文件和二进制输出。音乐标签和 EPUB 内部修改默认只生成计划，确认后执行。

### 7.2 媒体放置

首版支持：

- `in-place`：默认，不移动媒体，只在原目录生成元数据；
- `symlink`：支持相对或绝对软链接；
- `hardlink`：仅限同一文件系统；
- `copy`：完整复制；
- `reflink`：支持 required 或显式 fallback-to-copy 策略。

首版不支持破坏性的 `move`。任何能力降级都必须显式配置，不静默从硬链接或 Reflink 退化成复制。

所有放置操作均进入 `OutputPlan`，与元数据文件一起预览和执行。计划检查平台能力、目标冲突、硬链接文件系统边界和可用空间。

## 8. CLI

CLI 默认直接执行，不要求 Server：

```text
fixer search
fixer resolve
fixer scan
fixer plan
fixer scrape
fixer providers
fixer config
```

支持：

- 单条交互确认和批量置信度策略；
- `--dry-run`、`--apply`；
- `--offline`、`--proxy`；
- `--json`、`--quiet`、`--no-color`；
- 稳定机器输出与分层退出码；
- 参数覆盖配置但不隐式修改配置文件。

后续可添加 Server 客户端模式，但本地直接执行始终是一等能力。

## 9. Server 与 Web

### 9.1 Server

Axum API 立即以 `202 Accepted` 返回长任务 ID。固定数量 worker 在后台执行，SSE 提供单向进度事件。

任务状态：

```text
queued → scanning → searching → resolving → awaiting_confirmation
       → planning → writing → completed
```

运行状态可以转为 `failed`、`cancelled` 或 `interrupted`。服务重启后 queued 任务可重新调度，运行中任务标记为 interrupted 并允许重试；首版不实现中间步骤断点恢复。

SQLite 仅保存任务状态、输入、进度摘要、候选选择、确认信息、计划摘要、执行报告、API Token 摘要和配置版本。媒体、图片、大型原始响应、明文密钥及 Core 内部 Rust 快照不写入数据库。持久化 DTO 与 Core 类型布局解耦。

API 从 `/api/v1` 开始版本化，覆盖健康检查、Provider、搜索、任务、事件、候选确认、计划、执行、取消和设置。执行接口支持幂等键，错误使用稳定 DTO。

### 9.2 认证

- 默认监听 `127.0.0.1`；
- 回环地址可显式关闭认证；
- 非回环监听必须配置认证；
- 首版仅支持单用户密码和长期 API Token；
- Token 仅保存摘要，密码使用现代密码哈希；
- Cookie 使用 `HttpOnly`、`SameSite`，HTTPS 下使用 `Secure`；
- 状态修改接口具备 CSRF 防护；
- 反向代理身份头仅对显式可信代理生效；
- 媒体根目录使用 allowlist。

不实现注册、多用户或 RBAC。

### 9.3 Web

Web 使用 SolidJS 2.0+、TypeScript、Tailwind CSS、TanStack Router、TanStack Query 和 Vite。主要页面包括：

- 概览和最近任务；
- 目录选择与扫描；
- 手动搜索；
- 任务列表和实时进度；
- 候选、冲突及字段来源审查；
- 输出差异与写入确认；
- Provider、语言、代理和合并设置；
- 路径与内容模板管理。

Web 是刮削任务工作台，不实现播放、下载、云同步、文件上传、插件市场或原生移动端。

## 10. 错误模型

错误按阶段分类：输入、识别、Provider、匹配、合并、模板、计划和执行。Provider 错误进一步区分网络不可达、超时、代理、认证、限流、响应格式变化、无结果、不支持能力和离线跳过。

任务结果分为：

1. 致命错误；
2. 可继续的结构化警告；
3. 需要用户决策的确认项。

日志不得输出 API Key、密码、Cookie 或完整认证头。

## 11. Provider 交付策略

每个媒体切片分层交付：

1. Fixture 或本地 Provider，验证模型、匹配、语言和合并；
2. 一个真实网络 Provider，打通 HTTP、认证和错误处理；
3. 第二个互补来源，验证字段级多源合并。

建议首批来源：

| 媒体 | 本地/基础来源 | 主网络来源 | 可选互补来源 |
| --- | --- | --- | --- |
| 电影/电视剧 | NFO/JSON | TMDB | 后续区域来源 |
| 动漫 | NFO/JSON | Bangumi | AniList |
| 音乐 | 文件标签/CUE | MusicBrainz | 后续补充 |
| 图书 | EPUB/JSON | Open Library | Google Books |

网络站点不可达时，本地来源及其他 Provider 仍应工作。任何单一海外网站都不是产品启动或基本流程的硬依赖。

## 12. 测试与验证

根据风险选择验证，不在每次文件修改后运行完整套件：

- Core：模型、语言投影、匹配和合并单元测试；
- Provider：固定响应 Fixture，默认不访问真实网络；
- Writer：快照和临时目录行为测试；
- Placement：软链接、硬链接、复制和 Reflink 平台测试；
- SDK：Fixture Provider 到 `OutputPlan` 的端到端测试；
- CLI：参数、JSON 和退出码测试；
- Server：API、认证、状态机和 SQLite 迁移测试；
- Web：关键组件测试与阶段性真实浏览器验证；
- 在线 Provider 测试仅手动或定时运行。

公共接口完成时运行对应快速检查；垂直切片完成时运行相关测试；跨 crate 里程碑及最终交付运行工作区测试。

## 13. 增量交付顺序

1. 初始化 workspace、忽略规则和基础文档；
2. 建立 Core 标识、语言、置信度及来源模型；
3. 加入五类媒体 Work/Release/Asset 类型；
4. 定义 Provider、HttpClient、Writer 和输出计划协议；
5. 实现匹配与字段级合并；
6. 建立人体工学 SDK 和 Fixture Provider；
7. 实现本地 Provider 与文件识别；
8. 实现模板和本地 Writer；
9. 实现 in-place、软链接、硬链接、复制和 Reflink；
10. 交付电影 CLI 垂直切片；
11. 接入 TMDB，完成电影多源流程；
12. 依次交付电视剧、动漫、音乐和书籍；
13. 建立 Axum Server、SQLite 任务和认证；
14. 建立 SolidJS Web 工作台；
15. 完成模板管理、任务确认和写入预览；
16. 完成发布文档和端到端验收。

所有工作直接提交到 `main`，不创建 worktree。每个提交只表达一个独立变化，避免大量修改后一次提交。

## 14. 首版验收标准

- Rust SDK 可独立嵌入且不依赖数据库、Axum 或 Clap；
- 常见 SDK 调用简洁，高级组件可替换；
- 五类媒体均有类型化模型与端到端能力；
- 每类至少有本地来源和一个真实网络 Provider；
- 至少一个媒体类型验证两个来源的字段级合并；
- 简体中文、繁体中文、日文和英文可以无损保存并按策略投影；
- 直连、全局代理、endpoint 覆盖和简单离线模式可用；
- CLI 可以搜索、扫描、解析、预览和执行；
- Web 可以创建任务、查看进度、处理确认并批准写入；
- 输出支持内置格式、路径模板、内容模板和 Rust Writer；
- 媒体放置支持 in-place、symlink、hardlink、copy 和 reflink；
- 默认不覆盖，写入失败不留下半成品；
- 单个 Provider 不可达时可以部分成功；
- Server 重启不会无声丢失任务状态。
