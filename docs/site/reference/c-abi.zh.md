!!! note "翻译说明"
    中文文档是持续维护中的翻译版本，可能会略滞后于英文文档；如有技术差异，以英文版为准。

# 带版本的 C ABI

可分发的头文件是[canonical `openjoc.h` 头文件](https://github.com/chyinan/OpenJOC/blob/master/crates/openjoc-capi/include/openjoc.h)。它由项目手动维护，内容确定，并由仓库冒烟脚本分别用 C 和 C++ 编译。该 crate 通过 Cargo 构建 `rlib`、静态库和动态库目标。平台发行包面向调用方提供的内容包括：macOS 上的 `include/openjoc.h` 加 `libopenjoc_capi.a`/`libopenjoc_capi.dylib`，Windows 上的 `openjoc_capi.lib`/`openjoc_capi.dll.lib`/`openjoc_capi.dll`，以及 Linux 上对应的 `.a`/`.so` 文件。`.rlib` 是 Rust 内部构建产物，不是主要的 C 调用库。

## ABI 策略

ABI 版本为 `1.7-experimental`，与 OpenJOC 软件包版本彼此独立。重大改动可能破坏结构布局或所有权规则，需要增加 ABI 主版本号。小版本新增内容必须追加字段或函数，并保持已有字段的含义不变。配置、PCM 帧和输出信息结构体都包含 `struct_size`；调用方必须初始化它们，生产者必须拒绝尺寸更小的结构体。ABI 次版本 1 追加了 `dialnorm_mode` 字段。使用 ABI 1.0 配置结构大小的调用方仍会被接受，并收到 `OPENJOC_DIALNORM_DEFAULT`。ABI 1.2 追加了函数和状态码，但没有改变已有结构体布局。`openjoc_get_abi_version()` 返回 `(major << 16) | minor`。

ABI 1.4 在 `openjoc_decoder_config` 中追加了 `custom_speaker_layout`。需要使用自定义几何时，把它设为内存中的 `openjoc_custom_speaker_layout`；其中有序的 `openjoc_custom_speaker` 数组包含有限的方位角/仰角（单位为度），以及 `OPENJOC_SPEAKER_FULL_RANGE` 或 `OPENJOC_SPEAKER_LFE` 角色。描述结构和其中的所有字符串只在 `openjoc_decoder_create` 调用期间借用；解码器会复制经过验证的布局，并通过输出标签报告相同的顺序。原有调用方将此字段留空即可继续使用预设行为。自定义布局的约定、坐标规则、校验限制以及 WAV/CAF 元数据边界，记录在[自定义扬声器布局](../using/custom-speaker-layouts.md)中。

`openjoc_decoder_config_init()` 仍是对旧版本安全的 ABI 1.3 前缀初始化函数，不会写入 ABI 1.4 或之后追加的字段。ABI 1.4 调用方使用 `openjoc_decoder_config_init_v1_4()` 初始化自定义布局字段；ABI 1.6 调用方使用 `openjoc_decoder_config_init_v1_6()` 初始化精确的 v1.6 前缀并选择 HRTF；ABI 1.7 调用方使用 `openjoc_decoder_config_init_v1_7()` 初始化完整结构。保留的 v1.6 对齐字段可防止旧结构尾部填充被误读为启用新功能。
ABI 1.5 新增了 `openjoc_stream_decoder` 的只读 `openjoc_live_inspection_snapshot`。它报告同一条带内解码路径观察到的 programme 布局、重建载体、验证状态、对象/复杂度、EMDF、动态场景、AU 和时间戳信息；实时覆盖明确区分 `partial` 与 `complete_continuous`，seek、flush 或 reset 会开始新的观察 epoch。

ABI 1.6 在 `openjoc_decoder_config` 末尾追加 `hrtf_preset`。值 `0` 选择默认 SADIE II D1/KU100，值 `1` 选择 SADIE II D2/KEMAR。旧版 `struct_size` 会继续使用 D1；已退役的 Aachen 值 `2` 也会为旧调用方映射到 D1。

ABI 1.7 追加 `listener_orientation_pull_samples`。值 `0` 保持现有固定听音者姿态的双耳路径；`1..=256` 显式启用实验性的设备无关 3DoF 听音者姿态接口，并限制每次拉取的输出采样点数。CLI、WASM、DirectShow/LAV 和默认路径不变。

“实验性”表示 C 接口可能会在 OpenJOC 0.x 集成过程中继续演进，并不表示现有的解码器正确性声明被撤回。

## 所有权与调用方式

```c
openjoc_decoder_config config;
openjoc_decoder_config_init_v1_7(&config);

openjoc_decoder *decoder = NULL;
openjoc_decoder_create(&config, &decoder);
openjoc_decoder_send_packet(decoder, bytes, byte_count,
                            OPENJOC_NO_PTS, 0);

openjoc_pcm_frame frame;
openjoc_pcm_frame_init(&frame);
while (openjoc_decoder_receive_frame(decoder, &frame) ==
       OPENJOC_STATUS_FRAME_AVAILABLE) {
    /* frame.data is interleaved float32, valid until the next send/receive/reset */
}
openjoc_decoder_drain(decoder);
openjoc_decoder_destroy(decoder);
```

解码器是一个不透明句柄。数据包内存只在 `openjoc_decoder_send_packet` 调用期间借用，不会被保留。PCM 内存由解码器拥有，在该句柄下一次 send、receive、flush、reset 或 destroy 之前保持有效。需要更长生命周期的应用必须复制帧数据。多个句柄彼此独立。

对于 `openjoc_decoder_send_packet`，`pts_samples` 表示数据包第一个采样点的位置；`OPENJOC_NO_PTS` 表示省略该数据包的时间戳。即使前面的数据包没有时间戳，第一个提供的 PTS 仍会通过减去已解码的采样点数量来确定当前片段的时间原点。此前已返回或复制的帧时间戳保持不变；后续输出（包括先前数据包的延迟 PCM）使用该原点。后续提供的 PTS 必须与采样点数量的连续推进一致。原点或预期数据包 PTS 无法表示时，在解码之前拒绝输入；输出帧 PTS 无法表示时，返回渲染错误，不会回绕或钳位。reset、flush 或 discontinuity 清除时间锚点。`INT64_MIN` 专用于 `OPENJOC_NO_PTS`：如果后补的时间锚点使输出帧的实际 PTS 为 `INT64_MIN`，receive 返回 `OPENJOC_STATUS_RENDER_ERROR`，不改写输出帧；句柄必须经过 reset 或 flush 才能继续解码。Rust 的 `Option<i64>` 没有这一哨兵值限制。这是完整 AU 数据包 API 的约定；`openjoc_stream_decoder` 仍采用更严格的[数据包流时间戳约定](../../integration/FFMPEG.md#timestamps)。

ABI 1.2 还提供 `openjoc_stream_decoder`，供数据包边界不是完整访问单元边界的适配器使用。它的 `openjoc_stream_decoder_send_chunk()` 接受任意压缩字节、可选的 1/48000 采样域 PTS，以及已有的不连续/预滚标志。这个句柄复用 FFmpeg 外部桥接的单个、上限为 131,072 字节的组装器、JOC 正向识别、时间戳模型、输出队列、语义声道置换和延迟创建的 `OpenJocSession`。它支持一个数据块包含拆分的访问单元和多个访问单元，但不会暴露任何框架专用类型。

`openjoc_stream_decoder_receive_frame()` 按语义声道标签报告的顺序返回打包浮点 PCM。输出语义、精确的共享配置描述/指纹和当前受限的暂存大小，都可以在解码前或解码过程中获取。`OPENJOC_STATUS_NOT_JOC` 用来区分“已确认是普通 E-AC-3，因此拒绝交给 JOC”的情况；内存不足和外部库错误类别也各有专用数值状态码，方便主机映射错误。

ABI 1.3 增加了 `openjoc_classifier`，这是一个不解码、与框架无关的压缩流探测器。`openjoc_classifier_send_chunk()` 共享受限的访问单元解析器和 JOC 正向识别规则，但永远不会创建 OpenJOC 渲染会话，也不会输出 PCM。`openjoc_classifier_finish()` 会关闭探测，使最后一个完整的单访问单元流无需等待后续同步帧也能完成分类。输出是 `UNKNOWN`、`CONFIRMED_JOC`、`CONFIRMED_NON_JOC` 或 `INVALID_OR_UNSUPPORTED` 之一；暂存和已检查字节访问器会提供受限的探测统计。这适用于必须在把第一个数据包交给渲染器前先选择解码器的播放器。

语义标签可以通过 `openjoc_decoder_get_channel_label` 以及输出/帧描述结构获取。canonical PCM 采样格式值为 `1`（交错的 float32）。

把 `render_mode` 设为 `OPENJOC_RENDER_BINAURAL`，并把 `sofa_data` / `sofa_size` 设置为空/零，即可使用 `hrtf_preset` 选择的内置离线 HRTF（默认 `OPENJOC_HRTF_SADIE_D1_KU100`）。提供非空 SOFA 缓冲区时，会选择现有的严格用户数据集路径。如果 `virtual_layout` 为空，虚拟布局默认使用已配置的扬声器布局。设置 `speaker_layout = "22.2"` 可以选择原生 22.2 扬声器会话；其输出提供 24 个有序语义标签，包括 `LFE1` 和 `LFE2`。

### 实验性听音者姿态

仅对双耳会话设置非零的 ABI 1.7 拉取上限。把解码器交给渲染线程前，先调用 `openjoc_decoder_get_listener_orientation_preparer()` 或对应的 stream 函数取得不可变的准备句柄。该句柄共享已验证的 HRTF/布局资源；姿态更新在工作线程准备，不需要在准备期间访问或锁住解码器。状态快照给出 `stream_epoch`，每个 epoch 内的序号必须递增；reset 会推进 epoch，旧更新会被拒绝且不会被消费。

四元数结构采用带尺寸字段的 `(x, y, z, w)`，并对有限值做尺度稳定归一化。它表示从听音者局部坐标到场景坐标的主动旋转，坐标轴为 `+Y` 前方、`+X` 右方、`+Z` 上方。实现对固定虚拟扬声器方向应用逆旋转，然后为整组声道查询 HRIR。API 不读取传感器，也不接管任何音频设备。

在拉取模式下，每次接收最多返回配置的采样点数。压缩流桥接会在第一次接收前保留渲染尚未开始的状态；`pending_binaural_input_samples` 报告等待双耳化的投影虚拟扬声器 PCM，上限为一个 1,536 采样点访问单元。调用方必须在喂入下一 AU 前取完当前 PCM。在两次接收之间接受的姿态目标会作用于尚未渲染的后续分块；若已有 240 采样点过渡正在进行，新过渡要等其完成，且最多保留一个等待目标。drain 后的重建尾部和 FIR 尾部也会分块返回。这是 PCM/控制边界，不代表传感器时间戳或端到端延迟保证。

应用校验/生命周期错误时，原 update 句柄仍可使用。成功时 update 置空，`retired` 返回同一分配，即使没有旧滤波器也如此；请在音频回调之外销毁 retired。捕获到 panic 后解码器会被隔离，update 可能为空，应销毁且不要重试。同一个 preparer 句柄上的 prepare 与读取 last_error 必须串行；不同句柄可并行准备。准备错误可从 `openjoc_listener_orientation_preparer_last_error()` 读取。无姿态时的静态处理仍是默认行为。


C 适配器继承共享会话经过校准的默认 E-AC-3 Dialnorm 节目校准，除非显式把 `dialnorm_mode` 设为 `OPENJOC_DIALNORM_DIGITAL` 或 `OPENJOC_DIALNORM_ANALOG`。Default 推荐用于普通播放/解码。Digital 明确选择编码后的数字节目级校准。Analog 使用单位 Dialnorm 增益，是高级兼容/诊断策略，不是推荐的增大音量方式，也不是母带制作模式。Dialnorm 来自元数据，与现有 DRC 字段彼此独立；DRC 改变的是编码后的动态范围行为。FinalLinkedGain 是内部渲染器余量处理，不是用户的母带制作控制。

C ABI 是流式 PCM 接口，不执行文件导出峰值归一化，也不会为了文件级变换暂存完整节目。应用可以在收到 PCM 后自行应用最终静态增益策略。CLI 的 `--normalize-peak` 是文件输出的离线便利选项：它会在解码和渲染完成后，把最终文件归一化到请求的采样峰值；它不是 Dialnorm、DRC、限幅器、压缩器、LUFS 或真峰值归一化。

## 失败隔离

每个导出操作都会在返回前拦截 Rust panic。Rust panic、Rust 错误对象和 Rust 结构体布局都不会穿过 ABI。`last_error` 由解码器实例拥有，不是进程全局状态。空指针参数、无效结构大小、格式错误的数据包、不支持的配置、格式变化和渲染失败，都会返回数值状态码。

公共 C 头文件不包含第三方生成内容，并以仓库 Apache-2.0 许可证发布。
