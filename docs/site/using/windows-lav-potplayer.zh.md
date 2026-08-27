!!! note "翻译说明"
    中文文档是持续维护中的翻译版本，可能会略滞后于英文文档；如有技术差异，以英文版为准。

# Windows LAV / PotPlayer

OpenJOC 提供一个可选的 Windows DirectShow 下游 LAV Audio Decoder。它有独立的筛选器标识，可以和原有的 LAV 并存安装；安装包不会自动改动 PotPlayer 的设置。

下面记录的是 v0.12 及以后 Windows 集成中验证过的安装包行为；本站的发布基线是 v0.13.0。

## 安装并验证

1. 从 [OpenJOC 发行版页面](https://github.com/chyinan/OpenJOC/releases)下载 Windows LAV 安装包。
2. 将完整 ZIP 解压到自己有写权限的目录。
3. 双击 `install.bat`，接受 Windows 管理员权限提示。
4. 双击 `verify.bat`，确认结果为 **PASS**。
5. 如果安装时 PotPlayer 正在运行，请关闭后重新打开。

安装包会放入隔离的 OpenJOC 版本目录，只注册 OpenJOC 自有的 DirectShow 筛选器。它不会替换原有的 LAV，不会修改 `PATH`，也不会改变 PowerShell 执行策略。

## 在 PotPlayer 中选择筛选器

1. 按 `F5` 打开 PotPlayer 设置。
2. 选择 **Filter Control** → **Filter Priority (Overall)**。
3. 选择 **Add registered filter**。
4. 添加 **LAV Audio Decoder (OpenJOC)**。
5. 将它设为 **Prefer**，然后点击 **Apply** 和 **OK**。

请保留原有的 LAV 解码器。如果列表中没有 OpenJOC 筛选器，先运行 `verify.bat`；只有验证报告失败时才需要重新安装。

## 路由行为

- 普通 E-AC-3 仍走原有的 LAV/FFmpeg 路径。
- 压缩 E-AC-3 直通的优先级更高，会绕过 OpenJOC。
- 只有得到正向确认的 JOC，才会交给 OpenJOC 筛选器。
- 已确认的 JOC 可以从 raw E-AC-3 和 MP4 E-AC-3 输入解码。

Windows 适配器只提供以下七种固定的 48 kHz IEEE-float PCM 输出策略：

| 策略 | 声道数 | WAVEFORMATEXTENSIBLE mask |
| --- | ---: | ---: |
| Stereo | 2 | `0x00000003` |
| 5.1 | 6 | `0x0000060f` |
| 7.1 | 8 | `0x0000063f` |
| 5.1.2 | 8 | `0x0000560f` |
| 5.1.4 | 10 | `0x0002d60f` |
| 7.1.2 | 10 | `0x0000563f` |
| 7.1.4 | 12 | `0x0002d63f` |

每种策略只提出一种明确的语义方案，不提供备用声道掩码。Stereo 是默认值，其他布局必须显式选择。当前自动发现下游布局的状态为 `AUTO_NOT_RELIABLE`。

## 直通与硬件边界

OpenJOC 不会根据音频端点名称猜测布局，不执行低频管理，也不会把多个物理低音炮变成多个逻辑 LFE 声道。单独的 7.1.6、9.1.x、22.2、自定义几何和双耳输出，都不属于 LAV 输出声明。

这个集成验证的只是 PCM 采样数据能够通过文档规定的主机环境和音频端点检查；它不表示任意设备上的物理多声道硬件播放都已经验证。

## 卸载与回滚

双击 `uninstall.bat`。安装包只会删除 OpenJOC 自有的注册项和文件，并恢复原有的 LAV 配置。对于 OpenJOC 已经不存在的安装，非交互式卸载流程会把这种情况视为成功，无需再做任何操作。

如需查看更底层的源代码、版本固定信息和工程验证证据，请参阅仓库中的 [LAV 集成约定](https://github.com/chyinan/OpenJOC/blob/master/docs/integration/LAV_FILTERS_OPENJOC.md)。这些证据文件不会发布在本站。
