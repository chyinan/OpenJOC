!!! note "翻译说明"
    中文文档是持续维护中的翻译版本，可能会略滞后于英文文档；如有技术差异，以英文版为准。

# mpv OpenJOC 播放器软件包

OpenJOC Player Bundle 是项目提供的、打过补丁的 mpv/FFmpeg 构建，不是
mpv 或 FFmpeg 上游官方发行版。解压后，在 macOS/Linux 上使用媒体文件启动
`bin/openjoc-mpv`，Windows 上启动 `bin/openjoc-mpv.cmd`。Windows 也可以直接
运行 GUI 程序 `bin/mpv.exe`；它会自动发现旁边的 `bin/portable_config`，无需
包装脚本，也不会更改用户的全局 mpv 配置。直接发现配置的前提是未设置
`MPV_HOME`；如果已有 `MPV_HOME`，它会优先生效。软件包启动器会明确选择软件包
自己的配置目录，但不会更改该环境变量：

```text
bin/mpv.exe "path/to/media"
```

补丁播放器会自动选择已确认的 JOC；普通 E-AC-3 仍由 mpv 自带的 `eac3`
解码器处理。使用 `--audio-spdif=eac3` 可请求压缩直通，该模式会绕过
OpenJOC 渲染。

## 打开设置菜单

在 Player Bundle 中按 `Ctrl+Alt+J` 打开紧凑设置面板。可以点击方向按钮和
**Set path…**，也可以使用上下方向键（Tab 同样可以移动焦点）、左右方向键
和 Enter。主页面提供：

- 输出策略：Stereo speakers、Binaural、5.1、7.1、5.1.2、5.1.4、7.1.2
  或 7.1.4
- Dialnorm：Calibrated（推荐）或 Unity/Compatibility
- 双耳 HRTF：SADIE II D1/KU100、SADIE II D2/KEMAR 或 Custom SOFA
- 双耳虚拟布局：7.1.4 或实验性的 9.1.6

菜单修改先作为草稿保留。点击 **Save** 后，设置会写入
`bin/portable_config/openjoc-settings.json`，并在下一个文件打开时生效；不会
重新加载或跳转当前文件。点击 **Cancel** 会丢弃草稿。按 Esc 或点击面板外会
关闭面板并保留草稿，重新打开后仍可继续编辑。面板会把待应用的设置与只读的
当前 mpv 解码器/声道属性分开显示。解码器创建前，如果文件含有 E-AC-3 音轨，菜单会
将选项合并到该文件的解码器选项映射中。此时尚不能识别 JOC，因此普通或未选中的
E-AC-3 音轨也可能收到这些选项。由于映射按文件生效，混合音轨文件中的其他音频解码器
也可能收到这些选项并记录不支持选项的警告。菜单不会选择或强制使用 `libopenjoc`，
也不会更改 `ad`、`aid` 或 `audio-channels`；没有 E-AC-3 音轨的文件会跳过此钩子。
菜单不会中断、跳转或重新加载当前播放，特别是因为原始 JOC 输入可能只能向前播放。
如果想对当前文件使用不同策略，请自行停止并重新打开该文件。

点击 **Set path…** 后，可在文本输入框中填写一个已存在的本地 SOFA 文件路径。
解码器打开时仍会验证受支持的 `SimpleFreeFieldHRIR` 子集，详见[双耳与
SOFA](binaural-sofa.zh.md)。

如果旧版软件包曾将设置保存在 `config/openjoc-settings.json`，新配置文件
不存在时，菜单会从旧位置读取并用于迁移。旧文件会保留不动；下次明确点击
Save 时，设置会写入 `bin/portable_config/openjoc-settings.json`。

## 输出与硬件

输出策略选择 OpenJOC 渲染目标。Binaural 是显式双耳渲染，输出双声道耳机
音频；双声道扬声器渲染是另一种选择。mpv 保留原有音频输出映射，并可能根据
当前音频输出设备调整渲染结果。菜单不会修改 `audio-channels`、检测耳机，或
改变普通媒体的路由。若需要精确匹配硬件声道，请单独配置 mpv 的常规声道映射。

设置行显示待应用或已保存的选择；单独的只读行显示 mpv 当前报告的解码器和声道
属性，但不提供实时 JOC 元数据或诊断。LAV 的渲染后
Output gain 滑块尚未接入 mpv 解码器桥接。mpv 普通音量控制与 OpenJOC 输出
增益是不同功能。当前 mpv 集成也没有提供 LAV 的实时 JOC Stream 页面。

[mpv 集成指南](https://github.com/chyinan/OpenJOC/blob/master/docs/integration/MPV.md)
还介绍命令行配置、精确声道映射、解码器选项和验证边界。
