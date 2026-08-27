!!! note "翻译说明"
    中文文档是持续维护中的翻译版本，可能会略滞后于英文文档；如有技术差异，以英文版为准。

# 集成

适配器负责数据传输、主机生命周期和输出协商；OpenJOC 负责 E-AC-3/JOC 解码、场景构建、空间渲染、输出语义、延迟和排空状态。

仓库把各适配器的约定保留在它们自然所属的位置，而不是复制成另一份需要手动同步的规范。当前实现细节请使用以下链接：

| 适配器 | 仓库中的 canonical 文档 |
| --- | --- |
| FFmpeg 外部桥接 | [FFMPEG.md](https://github.com/chyinan/OpenJOC/blob/master/docs/integration/FFMPEG.md) |
| 原生 FFmpeg `libopenjoc` 包装器 | [FFMPEG_NATIVE.md](https://github.com/chyinan/OpenJOC/blob/master/docs/integration/FFMPEG_NATIVE.md) |
| GStreamer | [GSTREAMER.md](https://github.com/chyinan/OpenJOC/blob/master/docs/integration/GSTREAMER.md) |
| mpv | [MPV.md](https://github.com/chyinan/OpenJOC/blob/master/docs/integration/MPV.md) |
| 播放器软件包 | [PLAYER_PACKAGING.md](https://github.com/chyinan/OpenJOC/blob/master/docs/integration/PLAYER_PACKAGING.md) |
| 生态系统软件包 | [ECOSYSTEM_PACKAGING.md](https://github.com/chyinan/OpenJOC/blob/master/docs/integration/ECOSYSTEM_PACKAGING.md) |
| Windows DirectShow/LAV | [Windows LAV / PotPlayer](../using/windows-lav-potplayer.md) |

安装 OpenJOC 不会修改系统自带的 FFmpeg 或上游 mpv。项目提供的打过补丁的构建是独立产品，各自需要遵守对应源代码和第三方声明义务。
