!!! note "翻译说明"
    中文文档为维护中的翻译版本，可能略滞后于英文文档。如有技术差异，以英文版本为准。

# Windows LAV / PotPlayer

OpenJOC 提供可选的 Windows DirectShow 下游 LAV Audio Decoder。它使用独立的 filter identity，并与 stock LAV 并存安装。安装 package 不会自动改变 PotPlayer。

这些说明描述 v0.12+ Windows integration 中记录的 package 行为；本站 release baseline 是 v0.13.0。

## 安装与验证

1. 从 [OpenJOC releases 页面](https://github.com/chyinan/OpenJOC/releases)下载 Windows LAV package。
2. 将完整 ZIP 解压到可写目录。
3. 双击 `install.bat`，接受 Windows administrator prompt。
4. 双击 `verify.bat`，要求结果为 **PASS**。
5. 如果安装时 PotPlayer 正在运行，请关闭并重新打开。

package 会安装到隔离的 OpenJOC version 目录，只注册自有 DirectShow filter。它不会替换 stock LAV、修改 `PATH` 或改变 PowerShell execution policy。

## 在 PotPlayer 中选择 filter

1. 按 `F5` 打开 PotPlayer preferences。
2. 选择 **Filter Control** → **Filter Priority (Overall)**。
3. 选择 **Add registered filter**。
4. 添加 **LAV Audio Decoder (OpenJOC)**。
5. 设置为 **Prefer**，然后选择 **Apply** 和 **OK**。

保留 stock LAV decoder。如果看不到 OpenJOC filter，请运行 `verify.bat`；只有在验证报告失败时才重复安装。

## Routing 行为

- 普通 E-AC-3 仍走 stock LAV/FFmpeg path。
- 压缩 E-AC-3 passthrough 仍然拥有权威优先级，并绕过 OpenJOC。
- 只有被正向确认的 JOC 才会准入 OpenJOC filter。
- 已确认的 JOC 可以从 raw E-AC-3 和 MP4 E-AC-3 输入解码。

Windows adapter 只暴露以下七种固定的 48 kHz IEEE-float PCM policy：

| Policy | 声道数 | WAVEFORMATEXTENSIBLE mask |
| --- | ---: | ---: |
| Stereo | 2 | `0x00000003` |
| 5.1 | 6 | `0x0000060f` |
| 7.1 | 8 | `0x0000063f` |
| 5.1.2 | 8 | `0x0000560f` |
| 5.1.4 | 10 | `0x0002d60f` |
| 7.1.2 | 10 | `0x0000563f` |
| 7.1.4 | 12 | `0x0002d63f` |

每个 policy 都只提出一个精确的 semantic proposal，不提供 fallback mask。Stereo 是默认值；其他布局必须显式选择。当前自动下游布局发现状态为 `AUTO_NOT_RELIABLE`。

## Passthrough 与硬件边界

OpenJOC 不从 endpoint name 推断布局，不执行 Bass Management，也不会把多个物理 subwoofer 转成多个逻辑 LFE channel。独立的 7.1.6、9.1.x、22.2、自定义几何和双耳输出都不是 LAV output claim。

该 integration 证明的是通过文档规定的 host 与 endpoint checks 传递 PCM sample；不声明任意设备上的物理多声道硬件播放已经验证。

## 卸载与回滚

双击 `uninstall.bat`。package 只删除 OpenJOC 自有 registration 与文件，并恢复 stock LAV arrangement。已经不存在的 OpenJOC installation 对 package 的 non-interactive uninstall path 是成功 no-op。

更低层的 source、version pins 与 engineering evidence 请查看仓库的 [LAV integration contract](https://github.com/chyinan/OpenJOC/blob/master/docs/integration/LAV_FILTERS_OPENJOC.md)。evidence 文件有意不发布到本站。
