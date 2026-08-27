!!! note "翻译说明"
    中文文档是持续维护中的翻译版本，可能会略滞后于英文文档；如有技术差异，以英文版为准。

# 故障排除

遇到问题时，先判断是哪一道边界出了问题，再修改配置或归因于渲染器。

## 先做这几项检查

```sh
openjoc --version
openjoc --help
openjoc self-test
```

测试 MP4/M4A 时，请使用本地文件，并确保文件支持随机访问。文档规定的输入包括原始 E-AC-3，以及支持随机访问的普通 MP4/M4A；不支持随机访问的 MP4 和分片 MP4 不属于支持的流式处理路径。对于容器输入，请按照[安装](../getting-started/installation.md)中的说明准备 `ffprobe` 和 `ffmpeg`。

## 渲染前先检查输入

```sh
openjoc inspect input.ec3
openjoc decode input.ec3 --output-dir decode-report
```

`inspect` 会报告输入类型、配置范围、拓扑、时序和拒绝边界，不会让扬声器渲染器自行猜测。`decode` 会生成元数据清单（manifest）和用于诊断的 `ReconstructionBasis` 行 WAV。解码成功，并不代表每一种扬声器布局或容器格式都能正确表达结果。

## 渲染命令失败

先在 [CLI 参考](../reference/cli-reference.md)中确认选项拼写，再运行最基本的测试命令：

```sh
openjoc render-joc input.ec3 --layout 2.0 --output stereo.wav
```

确认这条命令能工作后，再切换到目标布局。只有当声道身份能够由 `WAVEFORMATEXTENSIBLE` 表示时，标准掩码才适用。`7.1.6` 和 `9.1` 系列需要使用 CAF 保存语义声道描述；`22.2` 与自定义几何布局使用显式的无掩码 PCM，这不等于已经验证了任意硬件播放。

如果选择双耳输出，请注意它采用的是虚拟扬声器渲染：

```sh
openjoc render-joc input.ec3 --binaural --output headphones.wav
```

内置 SADIE II D1 HRTF 是通用数据。自定义本地 SOFA 文件必须符合文档规定的严格 `SimpleFreeFieldHRIR` 子集；不支持 HDF5/NetCDF-4 和自动重采样。详见[双耳与 SOFA](binaural-sofa.md)。

## ADM 导出失败，或导出的对象不移动

先独立验证输出文件：

```sh
openjoc export-adm input.ec3 --output reconstructed.wav --adm-policy best-effort
openjoc validate-adm reconstructed.wav
```

除非完整的 decoded-JOC/OAMD 对象绑定配置组合通过支持范围检查，否则严格模式会拒绝导出。尽力模式会保留中性/静态输出；无法证明对应关系时，会记录 `unsupported_binding_reason`。在受支持的配置组合中，移动的重建对象表示从 JOC 数据中解码出的本地运动信息，不是恢复了原始创作 Atmos 母版。请阅读[解码对象与创作对象](../concepts/decoded-vs-authored-objects.md)和[重建 ADM 导出](reconstructed-adm-export.md)。

如果导出报告 PCM24 超范围或非有限值错误，不要期待它自动削波、归一化或使用隐藏的限幅器。写入器会在有符号 24 位量化边界处拒绝写入并明确报错；参阅 [PCM24 余量](../compatibility/pcm24-headroom.md)。

## Windows 播放没有走 OpenJOC

运行软件包中的 `verify.bat`，确认 OpenJOC 筛选器已注册，并在 PotPlayer 中把 **LAV Audio Decoder (OpenJOC)** 设为 **Prefer**。普通 E-AC-3 和压缩直通有意保留在原有路径；只有得到正向确认的 JOC 才会进入 OpenJOC 筛选器。详见 [Windows LAV / PotPlayer](windows-lav-potplayer.md)。

## 收集有用的 issue report

请提供 OpenJOC 版本、运行平台、完整命令、经过清理的 `inspect` 或校验器输出、所选布局/容器格式，以及问题是否可以稳定复现。未经许可，不要附带私人或商业媒体，也不要附带派生 PCM。结构验证通过，不等于原生 JOC 渲染器等价；请把这两类观察分开报告。
