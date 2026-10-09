!!! note "版本说明"
    本页已与英文版同步更新。当前候选版本的 D1/D2 双耳精确 unity 软件包验证仍是 CI 发布门槛，尚未报告通过；菜单控件存在不代表软件包验证已完成。

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
重新加载或跳转当前文件。**Apply Current** 是另一个显式操作，仅适用于当前已确认由
`libopenjoc` 解码的所选 E-AC-3 音轨。它会写入当前文件专属的 FFmpeg 解码器选项映射，
并让固定版本的 mpv 重建音频链，因此可能出现短暂音频间隙；它不会跳转或重新加载文件。
Apply Current 会立即生效，之后点击 Cancel 不会撤销它；Save 仍单独控制下一个文件的设置。

**Live output gain** 行可在 −20.0 到 +20.0 dB 之间以 0.1 dB 步进调节，并可通过 Reset
重置到 0 dB。设置文件中的键为 `output_gain_tenths_db`；旧设置文件没有此键时仍按 0 dB
处理。0 dB 使用精确 unity。此增益通过渲染后的独立滤波器实现，与 mpv 主音量无关；只有当
所选音轨确认是由 `libopenjoc` 解码的 E-AC-3 时，才会实时预览。FLAC、PCM、普通 E-AC-3
和压缩直通不会添加 OpenJOC 增益滤波器。在这些音轨上，该值仍可保存供之后的 OpenJOC
文件使用，但面板会显示无法实时预览。滤波器或命令失败时会明确报告错误；脚本不会改用
mpv 音量或解码器选项来伪装成功。

点击 **Cancel** 会丢弃草稿，并恢复未保存增益预览前的实时增益，即使预览命令失败也会
尝试恢复。按 Esc、点击面板外或关闭菜单会保留草稿及当前播放中的预览；只有点击 Save
才会将预览值持久保存。若保存时预览仍在等待或已失败，面板会明确说明尚未确认。
解码器创建前，如果文件含有 E-AC-3 音轨，菜单会将解码器选项合并到该文件的选项映射中。
此时尚不能识别 JOC，因此普通或未选中的 E-AC-3 音轨也可能收到这些选项。由于映射按
文件生效，混合音轨文件中的其他音频解码器也可能收到这些选项并记录不支持选项的警告。
菜单不会选择或强制使用 `libopenjoc`，也不会更改 `ad`、`aid` 或 `audio-channels`；
没有 E-AC-3 音轨的文件会跳过此钩子。除非显式选择 Apply Current，菜单不会中断当前播放；
它不会跳转或重新加载文件，这对只能向前播放的原始 JOC 输入尤其重要。

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
属性，但不提供实时 JOC 元数据或诊断。独立的渲染后增益不是 mpv 普通音量控制。当前
mpv 集成也没有提供 LAV 的实时 JOC Stream 页面。

此候选版本的 D1/D2 双耳精确 unity 软件包资格验证仍是 CI 发布门槛，目前尚未报告通过。
不能仅因菜单显示此控件就认为软件包验证已经完成。

[mpv 集成指南](https://github.com/chyinan/OpenJOC/blob/master/docs/integration/MPV.md)
还介绍命令行配置、精确声道映射、解码器选项和验证边界。
