# PURR 0.1.0 验证记录

本项目已采用用户提供的 `purr-slint` 页面和组件，连接 Rust + Slint 原生窗口与真实 Mihomo 后端。原始 `PURR-desktop-v5.html` 是外观和交互参考。没有 WebView。

**内存目标尚未达到。Windows 运行验收尚未完成。** 交叉编译、截图和功能检查不能替代这两项验收，也不能单独证明与 HTML 所有效果完全一致。

## 界面和交互

原生预览覆盖六页、首次运行和空列表、超时、断网、订阅过期、端口冲突、全局、直连、托盘和深色状态。预览脚本通过原生指针事件检查六页导航，通过键盘事件检查 Space 短按取消、约一秒唤醒和约 1.6 秒断开。

预览包含虚构节点、订阅、流量、CPU 和内存数据。只有显式 `--preview` 使用这些数据；生产列表由内核和本地持久化数据提供。截图不能证明实际资源占用、全部系统交互或 Windows 兼容性。

## 真实后端检查

`tools/check_runtime.py` 使用临时目录、随机回环端口、本地订阅服务和真实 Mihomo。禁用系统代理和开机自动连接。

最近完整 release 检查通过 18 组功能检查：

- 加载 2,001 条规则，检查搜索结果为一条、零条及清空恢复；切换三种模式并修改真实内核节点选择。
- 64 次真实代理传输，16 路并发，共 184,549,376 字节。
- 传输期间按 UUID 关闭一条连接，再关闭全部连接。
- 导入两个订阅，读取服务商名称、用量和到期时间，缓存切换并更新备用订阅。
- 编辑主用和备用订阅链接，保留 ID 与条目数；无效编辑保留当前配置和缓存。
- 重复提交无效配置仍显示错误。
- 端口冲突状态及恢复、端口重绑定和恢复原端口。
- 到期订阅提示、备用订阅恢复，并拒绝选择已到期缓存。
- 本地 YAML 导入和超过 2 MB 文件拒绝。
- 设置控件保存 DNS、IPv6 和呼噜声选项。
- 原生托盘弹窗打开、指针切换模式、失焦关闭、Escape 关闭和重复点击关闭。
- 关闭到托盘后重建窗口，恢复完整订阅和规则模型。
- 设置事务、DNS、广告规则及失败修改保留原配置。
- 结束测试内核后由监督进程重新启动。
- 原生指针访问六页，macOS 系统代理保持运行前状态。

`tools/check_probes.py` 用三个本地 HTTP CONNECT 代理验证真实测速：导入后自动选中 Fast，将它置为不可用后，经历三次失败并显示超时，随后自动选中 Slow，合计 68.04 秒。Fast 收到四次请求（导入测速加三次失败）；Slow 和 Dead 各收到两次请求。修复缓冲后的二进制重测通过，记录为 `ui-preview/buffer-probes-recheck.json`，与资源测试使用同一二进制。测试目标 URL 覆盖仅在专用 fixture 标记目录中启用。

该轮较早的一次故障切换复测在 100 秒内未完成切换，失败日志为 `ui-preview/buffer-probes.log`，原因未确定。之后同一二进制重测通过，仍保留这次失败记录，不能由单次通过推断所有情况下均稳定。测试窗口可接受外部操作，重测应用日志也包含脚本以外的页面切换。

尚未完成的端到端项目包括音频听感、TUN 提权、开机启动、实际服务商网络订阅、实际 OS 文件拖放/剪贴板、Windows 安装/卸载及 Windows 系统代理恢复。

`tools/check_macos_tray.py` 另用 Accessibility 读取真实状态栏图标的位置，再通过系统鼠标事件验证左键打开和关闭、长按关闭后不重开、右键打开设计弹层、Escape 关闭，以及主窗口获取焦点后关闭并可再次打开。此前的直接调用检查漏掉了首次打开立即失焦关闭、再次点击后重开的缺陷；已分别修正焦点生命周期和鼠标按下/释放顺序。此检查仅在 macOS 执行。

最终源码的第一次复测中，重复点击关闭失败，同时应用日志出现脚本未请求的模式切换。脚本随后补上请求确认同步，同一二进制连续两次六项检查通过（`nav-delivery-os-tray-recheck.json`、`nav-delivery-os-tray-repeat.json`）。保留首次失败日志 `nav-delivery-os-tray.log`；同步缺失是否完全解释该次失败尚未确定。

动画本轮修正了 CSS 缓动求解、350 ms 页面入场、顶部提示、180 ms 托盘入场、故障卡片入场、三次导入提示抖动、图标启用时重新计时、悬停退出停止，以及流动虚线的帧数。路由卡片、节点标签、连接关闭按钮及托盘条目补上颜色过渡。`tools/check_animation.py` 的 51 个真实 Slint 采样与浏览器 CSS 比较，最大误差 0.000000476。该数值只证明采样曲线，不能证明所有动画逐帧相同。界面字体、部分表格列宽和长文本布局仍有与 HTML 不完全一致的已知差异。

## 本次开关和导航补充验证

macOS release SHA-256 为 `51d8d4397d38502935805e876fb9c6c58211172c57ff7d80e18ce9f110ba14e1`。`ui-preview/switch-final-check.json` 记录：

- 170 个真实组件采样，覆盖普通切换和两次中途反向。位置与 Chromium 最大差约 0.000052 px；轨道透明度与 CSS 方程最大差约 0.000000056，浏览器颜色序列化后的最大差约 0.0024。
- 三个实际软件渲染帧覆盖静止、约 -8° 和约 +5°，与原 SVG 的墨线最大距离均为一个物理像素。墨线距离允许两个渲染器的边缘抗锯齿差异，不表示逐像素相等。
- 截图对应的订阅异常波形实际渲染对照，墨线最大距离为一个物理像素。
- 九种状态、三种宽度的 1,450 个原始画布坐标通过，最大纵向差约 0.000819 px。还检查 5/8 虚线长度和长按分段接缝。
- 旧状态刷新不再覆盖待保存的开关值；连续点击、过期完成通知和保存失败回退的状态检查通过。

此前采样没有验证实际旋转渲染。Slint 1.18.1 的软件渲染器 `rotate`、`scale` 是空实现，上游 master 仍如此。本次让开关、图标和表情直接变换路径坐标，并直接调整光标高度。另补偿 `Path` 的描边内缩，避免 SVG 被额外缩小；导航波形使用原始画布坐标，避免 TUN 内外描边各自缩放而错位。未修正描边时的静止帧检查曾失败，最大墨线距离约 2.24 像素，见 `switch-rendered-comparison.log`。

这不能证明整个应用所有帧与 HTML 完全相同。字体、表格列宽和长文本布局等既有差异仍需单独验收。后续内存方案及取舍见 `memory-options.md`。

同一二进制的 `switch-final-runtime.json` 通过 20 项真实后端检查，新增连续七次快切后界面、内核与持久化值一致，以及故意制造偏好保存失败后四处状态回退的检查。传输量仍为 184,549,376 字节，系统代理保持测试前状态。

本次合计 RSS 峰值 **219.824128 MB**，主程序独立峰值 175.734784 MB、内核独立峰值 44.220416 MB；合计 physical footprint 峰值 115.607280 MB。RSS 比上一轮 217.120768 MB 高 2.703360 MB，未达到 150 MB。流程新增了快速切换和失败保存，不能把差额全部归因于某个代码改动。资源脚本以退出码 2 标记预算失败。

本轮全目标严格 Clippy 被并行加入的 `src/ffi.rs:37` 阻塞：公开的安全函数释放裸指针，触发 `clippy::not_unsafe_ptr_arg_deref`。仅选择 `--bin Purr` 仍会检查该库依赖，也因此失败。未修改该独立 Swift/FFI 工作；不能把全仓库 Clippy 报为通过。

`cargo test --release --bin Purr` 的 23 项测试通过。20 个原生预览状态截图和原生交互检查通过，见 `switch-final-pages.log`；`switch-final-animation.log` 的 51 个 Slint 曲线值通过，最大 CSS 参考差约 0.000000476。`tools/gen_icons.py` 与 `cargo fmt --check` 通过。完整预览图保存在 `ui-preview/switch-final-pages/`，其中 PNG 是原始 PPM 的无损转换。

Windows x64 静态 CRT release 交叉编译成功，程序 SHA-256 为 `7e3e98d76748e663d2c700a2a57d38f25159ae4a810a49dfc925f9b3cadce7dc`，大小 11,567,616 字节。链接器报告静态 CRT 缺少 PDB 的 LNK4099 调试信息警告，仍成功生成 PE x64 程序，见 `switch-final-windows-build.log`。最新包位于 `dist/native-v5-switch-candidate/`，包含本轮开关、导航和状态同步修复；旧的 `native-v5-nav-candidate` 不含这些改动。未进行 Windows 实机运行或内存测试。

## 资源测量

默认构建仅包含 software 渲染。可通过 `cargo build --release --features gpu-renderer` 加入 GPU 渲染支持，再设置 `PURR_RENDERER=femtovg`。测量来自 macOS ARM64 上新启动的 release 主程序和 Mihomo，包含启动、并发流量、订阅事务、托盘及六页访问。

取每个采样时刻主程序 RSS 与内核 RSS 的合计峰值，单位为十进制 MB。临时配置校验进程和单独运行的后端验证器不在这项 GUI + 常驻内核统计中。

| 动画补齐前的历史 release 测量 | 十进制 MB |
| --- | ---: |
| 合计 RSS 峰值 | 210.845696 |
| 主程序独立 RSS 峰值 | 168.591360 |
| 内核独立 RSS 峰值 | 43.991040 |
| 合计 physical footprint 峰值 | 110.593752 |
| 要求的 RSS 上限 | 150 |

上表是本轮动画补齐前的历史记录 `ui-preview/buffer-runtime.json`，对应 macOS 二进制 SHA-256 为 `dbb349c043f866fc98dc3051b8e4f8f3fd4ffdcd5d7828c3e5fcad67f53474dc`。

在当时的界面、字体、分辨率和动画实现下，移植 softbuffer 上游 CoreGraphics 缓冲复用与图层释放修复后，同一压力流程的合计 RSS 峰值由 241.188864 MB 降至 210.845696 MB，减少 30.343168 MB，约 12.6%。仍未达到 150 MB。修复版本的 17 组功能检查、23 项测试和 20 个预览状态检查通过；节点、订阅、连接、规则及习惯五页的正文区域与修复前逐像素比较一致。该历史比较没有证明旋转已正确渲染。移植来源见 `vendor/softbuffer/PATCHES.md`。

本轮最终构建补齐上述动画，并共享规则存储、缓存规则模型、避免模型比较时分配临时向量，且默认不再链接可选的 OpenGL 渲染器。最终实测如下：

| 本轮最终 release 测量 | 十进制 MB |
| --- | ---: |
| 合计 RSS 峰值 | 217.120768 |
| 主程序独立 RSS 峰值 | 173.015040 |
| 内核独立 RSS 峰值 | 44.187648 |
| 合计 physical footprint 峰值 | 113.264392 |
| 要求的 RSS 上限 | 150 |

记录为 `ui-preview/nav-delivery-runtime.json`，二进制 SHA-256 为 `702047205a02ed7457b15b1e644f4f08bebcd1fb6398a15d902f48b921f93da8`，与最终托盘、缓动和预览检查使用同一程序。18 组功能检查通过，传输 184,549,376 字节，系统代理保持测试前状态。RSS 比上一版 210.845696 MB 上升 6.275072 MB，不能报告为进一步降低。

本轮早期补动画的构建为 221.921280 MB，软件专用中间构建为 216.367104 MB；这些构建的动画和悬停细节不同，不能把差额完全归因于某一项优化。另试用 mimalloc 后为 233.226240 MB，因回退已撤销，最终程序没有该依赖。所有测量均使用新进程，完整样本保存在各 JSON 中。

最终预览的节点、订阅、连接、规则、习惯五页正文分别比较 2,592,000 个物理像素，与动画补齐前截图差异均为零（`ui-preview/nav-delivery-pixels.json`）。比较在减少动态效果模式下进行，只验证静态正文保留，不能证明与 HTML 或所有动画一致。

此次优化仅修改 macOS 渲染后端；Windows 渲染后端原已复用缓冲，代码未改，不能把这 30.3 MB 的节省套用到 Windows。Windows 暂无实测内存结果。physical footprint 是不同指标，不能替代 RSS 要求。

功能检查通过但 RSS 超限时脚本退出码为 2，不能报告为全部通过。

## 构建和复现

macOS 的 23 项 Rust 测试及严格 Clippy 已通过，最终原生预览完成 20 个状态截图及交互检查。相同源码的 Windows MSVC x64 release 严格 Clippy 和交叉构建已通过，包含静态 Microsoft C runtime 和通过 SHA-256 校验的 Mihomo v1.19.33 compatible x64。Windows 实机运行仍需验收。

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
target/release/Purr --preview --capture /absolute/path/screenshots
python3 tools/check_runtime.py --binary target/release/Purr --report /absolute/path/runtime.json
python3 tools/check_probes.py --binary target/release/Purr --report /absolute/path/probes.json
python3 tools/check_animation.py --binary target/release/Purr
python3 tools/check_switch.py --binary target/release/Purr
```

Windows 上将 `--binary` 指向解压后的 `Purr.exe`。仓库的 `.github/workflows/windows.yml` 提供 Windows 原生构建、测试、截图、后端和资源验证。该工作流尚未发布或执行。

## 数据和交付边界

支持多个订阅，各自保存缓存；一次使用一个活动订阅。活动订阅和备用订阅都可以编辑。订阅导入上限为 2 MB。

规则和连接列表保留全部匹配行，使用虚拟滚动；日志保留最近 200 条。规则命中数、广告拦截数和路由流量来自连接轮询采样，可能漏掉短连接和很快结束的拒绝；不能当作内核精确计数。探测成功率按最近 24 小时的测速样本计算，不等于连续在线时长。每日统计按 UTC 日期归档。

Windows 安装包和程序未签名。安装器为当前用户安装，卸载保留 `%APPDATA%\Purr` 配置。便携版须完整解压并保留相邻的 `resources` 目录。
