!!! note "翻译说明"
    中文文档是持续维护中的翻译版本，可能会略滞后于英文文档；如有技术差异，以英文版为准。

# 支持的平台

OpenJOC 的核心是一个 Rust 工作区，包含与平台无关的解码、场景、渲染器、ADM 和 WAV/CAF 组件。但实际发行版和各类集成的支持范围，会比核心代码的组件范围更窄。

| 功能 | 当前范围 |
| --- | --- |
| CLI 和 Rust API | 可在当前 Rust 工具链支持的目标平台上，从工作区构建；具体还取决于所选的可选依赖。 |
| 发行版软件包 | 当前提供 macOS arm64、Windows x86_64 和 GNU/Linux x86_64 版本。 |
| 扬声器渲染器 | 预设布局和自定义几何布局可以渲染为 OpenJOC 自有的 WAV/CAF 输出，但受[输出约定](../reference/output-formats.md)限制。 |
| 双耳渲染器 | 使用内置 SADIE II D1 资源或受支持的本地 SOFA 文件，生成双声道虚拟扬声器输出。 |
| FFmpeg | 提供外部桥接和单独的原生 `libopenjoc` 包装器，用于定制 FFmpeg 构建；安装 OpenJOC 不会修改系统自带的 FFmpeg。 |
| GStreamer | 可选的原生插件，需要 OpenJOC 专用的分类 caps 特性和匹配的 GStreamer 运行时。 |
| mpv | 项目提供打过补丁的构建和播放器软件包；不是官方上游的 mpv 或 FFmpeg 发行版。 |
| Windows DirectShow/LAV | 可选的隔离式筛选器，提供七种明确的 48 kHz IEEE-float PCM 输出方案。 |

多声道 PCM 的生成和传输已经在文档规定的测试范围内完成验证。项目不承诺在任意物理硬件上都能播放多声道内容；自动发现音频端点或设备布局也不属于 OpenJOC 的接口约定。

请参阅[能力矩阵](../project/capabilities.md)了解证据边界，参阅[集成概览](../project/integrations.md)查看各适配器对应的仓库文档。
