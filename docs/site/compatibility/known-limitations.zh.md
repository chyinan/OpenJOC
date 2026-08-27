!!! note "翻译说明"
    中文文档为维护中的翻译版本，可能略滞后于英文文档。如有技术差异，以英文版本为准。

# 已知限制

这是当前面向用户的限制与不声明事项的 canonical 列表。正向支持状态由[能力矩阵](../project/capabilities.md)维护；历史限制位于 changelog、research record 和 archive 中。

OpenJOC 是独立的实验性互操作项目。不声明 Dolby endorsement、certification、licensed implementation、bit-identical Reference Player output 或 proprietary renderer fidelity。

## 解码器与语义边界

- `ObjectScene` 将 `ReconstructionBasis` rows 与 authored objects 分开。精确准入的 decoded-JOC/OAMD carrier profile 可以报告 `SemanticBindingState::ResolvedWithinCarrier`，但这不是 authored-object identity recovery；其他 profile 均为 `Unresolved`。
- ordinary-domain `JocSpatialBridge` 使用 OAMD-derived control 渲染 decoded Base/RB contributions，但不解决 authored-object identity 或 codec-domain operator `T(t)`。
- non-LFE Base/full-band PCM 已证明是 JOC reconstruction input，但尚未证明是独立的 final-scene contribution。Base C energy 和原始 authored Bed 不能授权额外 ADM export；Base 加 decoded Objects 需要先通过 double-counting proof。
- `ETSI_STRICT` 拒绝 published profile 之外的 syntax，包括观察到的 OAMD warp `raw=3`。`OBSERVED_VENDOR_COMPAT` 是显式的 partial policy，只保留 opaque continuation，不赋予 vendor semantics。一个精确的 15-object raw3 compatibility shape 在 tested scope 内被准入 decoded-object scene path，因为不需要额外 raw3-specific transform。
- public-syntax coding-tool support 具有有界的数值与状态覆盖范围。不声明对所有 producer、carrier、coding-tool combination 及 malformed-input interaction 的完整 fidelity。

## 输入与 streaming

- JOC rendering 使用 48 kHz。原始 E-AC-3 与可 seek 的 ordinary ISO BMFF input 在文档规定的 topology 和 access-unit boundary 内准入；non-seek 或 fragmented MP4 不准入。
- Rust `OpenJocSession` packet API 每次 push 接受一个完整的 E-AC-3 JOC access unit：I0 加可选 D0。demuxing、任意 byte fragmentation 与每次多个 AU 属于 bounded C stream decoder 或 framework adapter，而不是 Rust packet contract。
- 只准入一个 I0 加可选 D0 dependent topology；额外 dependent-substream shape 会被拒绝。
- 某些 seekable container 和 compatible-base workflow 需要 `ffprobe` 或 `ffmpeg`；OpenJOC 不是 zero-dependency distribution。

## 扬声器与双耳渲染

- preset 与 custom layout 共用 generic renderer。custom geometry 限制为最多 64 个有序输出声道，并且至少需要两个可用的 full-range direction。renderer admission 不证明 host、device 或 container 可以传输相同 geometry。
- `7.1.6` 和 `9.1` family 需要 semantic CAF output，因为标准 `WAVEFORMATEXTENSIBLE` mask 无法真实表示其 identity。`22.2` 和 custom WAV 使用 explicit unmasked PCM；需要保留坐标时优先使用 CAF。
- OpenJOC 不执行 crossover、bass management、room correction、speaker calibration、head tracking、distance model、Doppler 或 device discovery。LFE ownership 是显式的，不从 channel count 推断 physical device。
- 双耳输出是 virtual-speaker rendering，不是 proprietary direct-object binaural parity。内置 SADIE II dataset 是 generic 的；listener 可能更适合 custom SOFA。
- custom SOFA 支持严格的本地 `SimpleFreeFieldHRIR` NetCDF classic CDF-1 subset，具有固定 listener pose、两个 receiver、共同 sample rate 以及有界的 exact/interpolated directional coverage。不支持 HDF5/NetCDF-4、resampling、downloads、writing、moving sources 或 universal dataset coverage。

## 输出电平与同步

- DRC 使用编码的 E-AC-3 dynamic-range metadata。Dialnorm 控制 programme calibration；两者彼此分开，也与 file-export normalization 分开。
- `DialnormMode::Default` 是推荐的 calibrated behavior。`Digital` 显式选择 encoded digital calibration；`Analog` 是 advanced unity-gain compatibility/diagnostic policy，不是更高质量或 mastering mode。
- `--normalize-peak` 应用一个静态的 post-render sample-peak scalar。它不是 LUFS 或 true-peak normalization、limiter、compressor 或 DRC；inter-sample peak 可能超过请求值。
- speaker output 报告 609 samples availability delay（577 QMF/Base-RB 加 32 FinalLinkedGain）。binaural 报告 577 samples，不包含有限 FIR tail。逻辑 PTS 不会被移动来隐藏该 delay。

## ADM 互操作

- `export-adm` 写入重建的 RIFF/RF64 ADM BWF 表示，而不是原始 ADM/BWF master。原始 names、hierarchy、UIDs、authored binding 和 discarded source information 无法恢复。report 明确保留 `original_authored_identity_recovered: false`、`original_adm_master_recovered: false` 与 `lossless_round_trip: false`。
- 对精确 clean-room profiles（15 JOC objects、无 bed、一个前置 Base LFE、无 ISF、15 dynamic OAMD objects、16 total），decoded JOC PCM 通过 typed carrier-local ordinals 与对应的 OAMD dynamic metadata 配对。这包括 ordinary strict profile 和 exact observed raw3-compatible profile。
- 结构和 decoded-scene validation 不保证与 native JOC final renderer 感知上相同的定位。在至少一个真实世界 validation programme 中，技术检查通过后仍观察到 material-specific、non-generalizable 的 residual localization difference。需要 exact native-renderer localization 时，以 native JOC playback 为参考。
- moving reconstructed Object 表示从 JOC programme 中保留并解码的 spatial metadata。其 trajectory 可能与源 DAW automation 不同；meaningful decoded trajectory 不是原始 master recovery。
- OpenJOC 不承诺恢复原始 DAW/Logic track identity、authored Object numbering、Object names、source-stem PCM、unquantized automation、programme/content hierarchy、authoring metadata、Dolby authoring provenance 或 lossless JOC-to-ADM round trip。
- scoped dynamic path 在 decoded OAMD event boundary 导出 position。active/inactive transitions、extent、gain、divergence、channel lock、zones 和其他 properties 不会被用来凭空发明 ADM semantics；不支持的 metadata 在 best-effort 时 neutral output 并给出 reason，strict 时拒绝。
- 存在 Base LFE 时，exporter 创建最小合法的 5.1 transport bed。只有 LFE 携带 recovered Base LFE PCM；L、R、C、Ls、Rs 是确定性的 silence placeholders，并被报告为 generated structure。
- 生成的 `dbmd` 只有 public EBU Supplement 6 envelope。保留的 Atmos-specific segment payload 与 Dolby authoring provenance 不会被复制、猜测或伪造。
- Logic Pro 可以导入重建文件；Logic-authored re-export 被 Dolby Encoding Engine 接受。OpenJOC 自己生成的文件仍不支持且不声明 direct DEE ingest。

## API 与集成

- C ABI 1.4 在 OpenJOC 0.x 期间是实验性的。public header、structure sizes、ownership rules、numeric statuses 与 compatibility initializers 是 contract；ABI evolution 仍可能发生。
- external FFmpeg bridge 是 embedding surface，不是已安装 `ffmpeg` executable 的 out-of-tree plugin。native `libopenjoc` decoder 需要 patched custom FFmpeg build 和显式 positive JOC selection。
- GStreamer 使用 OpenJOC-specific experimental caps feature，需要匹配的 host runtime，不会全局改变已安装的 GStreamer。
- mpv 与 OpenJOC Player Bundles 是项目提供的 custom builds，不是 official upstream mpv 或 FFmpeg releases。物理多声道播放仍需要接受请求 map 的 audio output 和 device。
- Windows DirectShow/LAV integration 正向准入 JOC，将普通 E-AC-3 留在 stock LAV/FFmpeg，并保持 passthrough precedence。其固定的 48 kHz IEEE-float PCM policy 是 Stereo、5.1、7.1、5.1.2、5.1.4、7.1.2 和 7.1.4。每个 policy 只提出一个精确的 `WAVEFORMATEXTENSIBLE` proposal，不提供 fallback。Stereo 是默认值，其他布局需显式选择；physical multichannel hardware 尚未验证。OpenJOC 不从 endpoint name 推断布局，不执行 Bass Management，也不将 physical subwoofer count 转成 logical LFE channel。独立的 7.1.6/9.1.x/22.2 或 custom renderer support 不属于 LAV output claim。

## 平台与 release 范围

- platform package 覆盖当前 release metadata 记录的 target。多声道 PCM generation 与 transport 已 qualified；并非每个 Linux 或 Windows device 的 physical speaker-system playback 都独立验证过。
- macOS artifacts 在需要时使用 ad-hoc signing，不是 Developer-ID signed 或 notarized。Linux compatibility 受记录的 glibc/runtime baseline 约束。Windows bundle 使用其文档规定的 adjacent-DLL 或 isolated LAV installation model。
- private/commercial programme fixtures 与 derived PCM 不分发。因此，一些 real-media acceptance 仍是 maintainer release gate。

相应的正向声明与证据边界请参阅[能力矩阵](../project/capabilities.md)和[扬声器渲染](../using/speaker-rendering.md)。
