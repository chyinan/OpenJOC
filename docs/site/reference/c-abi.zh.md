!!! note "翻译说明"
    中文文档是持续维护中的翻译版本，可能会略滞后于英文文档；如有技术差异，以英文版为准。

# 带版本的 C ABI

可分发的头文件是[canonical `openjoc.h` 头文件](https://github.com/chyinan/OpenJOC/blob/master/crates/openjoc-capi/include/openjoc.h)。它由项目手动维护，内容确定，并由仓库冒烟脚本分别用 C 和 C++ 编译。该 crate 通过 Cargo 构建 `rlib`、静态库和动态库目标。平台发行包面向调用方提供的内容包括：macOS 上的 `include/openjoc.h` 加 `libopenjoc_capi.a`/`libopenjoc_capi.dylib`，Windows 上的 `openjoc_capi.lib`/`openjoc_capi.dll.lib`/`openjoc_capi.dll`，以及 Linux 上对应的 `.a`/`.so` 文件。`.rlib` 是 Rust 内部构建产物，不是主要的 C 调用库。

## ABI 策略

ABI 版本为 `1.4-experimental`，与 OpenJOC 软件包版本彼此独立。重大改动可能破坏结构布局或所有权规则，需要增加 ABI 主版本号。小版本新增内容必须追加字段或函数，并保持已有字段的含义不变。配置、PCM 帧和输出信息结构体都包含 `struct_size`；调用方必须初始化它们，生产者必须拒绝尺寸更小的结构体。ABI 次版本 1 追加了 `dialnorm_mode` 字段。使用 ABI 1.0 配置结构大小的调用方仍会被接受，并收到 `OPENJOC_DIALNORM_DEFAULT`。ABI 1.2 追加了函数和状态码，但没有改变已有结构体布局。`openjoc_get_abi_version()` 返回 `(major << 16) | minor`。

ABI 1.4 在 `openjoc_decoder_config` 中追加了 `custom_speaker_layout`。需要使用自定义几何时，把它设为内存中的 `openjoc_custom_speaker_layout`；其中有序的 `openjoc_custom_speaker` 数组包含有限的方位角/仰角（单位为度），以及 `OPENJOC_SPEAKER_FULL_RANGE` 或 `OPENJOC_SPEAKER_LFE` 角色。描述结构和其中的所有字符串只在 `openjoc_decoder_create` 调用期间借用；解码器会复制经过验证的布局，并通过输出标签报告相同的顺序。原有调用方将此字段留空即可继续使用预设行为。自定义布局的约定、坐标规则、校验限制以及 WAV/CAF 元数据边界，记录在[自定义扬声器布局](../using/custom-speaker-layouts.md)中。

`openjoc_decoder_config_init()` 仍是对旧版本安全的 ABI 1.3 前缀初始化函数：它永远不会写入 ABI 1.4 新增的字段，因此真正的 ABI 1.3 调用方可以把它链接到 ABI 1.4 库，而不会发生结构体越界写入。需要完整当前结构或自定义几何的 ABI 1.4 调用方，应使用 `openjoc_decoder_config_init_v1_4()`。

“实验性”表示 C 接口可能会在 OpenJOC 0.x 集成过程中继续演进，并不表示现有的解码器正确性声明被撤回。

## 所有权与调用方式

```c
openjoc_decoder_config config;
openjoc_decoder_config_init_v1_4(&config);

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

ABI 1.2 还提供 `openjoc_stream_decoder`，供数据包边界不是完整访问单元边界的适配器使用。它的 `openjoc_stream_decoder_send_chunk()` 接受任意压缩字节、可选的 1/48000 采样域 PTS，以及已有的不连续/预滚标志。这个句柄复用 FFmpeg 外部桥接的单个、上限为 131,072 字节的组装器、JOC 正向识别、时间戳模型、输出队列、语义声道置换和延迟创建的 `OpenJocSession`。它支持一个数据块包含拆分的访问单元和多个访问单元，但不会暴露任何框架专用类型。

`openjoc_stream_decoder_receive_frame()` 按语义声道标签报告的顺序返回打包浮点 PCM。输出语义、精确的共享配置描述/指纹和当前受限的暂存大小，都可以在解码前或解码过程中获取。`OPENJOC_STATUS_NOT_JOC` 用来区分“已确认是普通 E-AC-3，因此拒绝交给 JOC”的情况；内存不足和外部库错误类别也各有专用数值状态码，方便主机映射错误。

ABI 1.3 增加了 `openjoc_classifier`，这是一个不解码、与框架无关的压缩流探测器。`openjoc_classifier_send_chunk()` 共享受限的访问单元解析器和 JOC 正向识别规则，但永远不会创建 OpenJOC 渲染会话，也不会输出 PCM。`openjoc_classifier_finish()` 会关闭探测，使最后一个完整的单访问单元流无需等待后续同步帧也能完成分类。输出是 `UNKNOWN`、`CONFIRMED_JOC`、`CONFIRMED_NON_JOC` 或 `INVALID_OR_UNSUPPORTED` 之一；暂存和已检查字节访问器会提供受限的探测统计。这适用于必须在把第一个数据包交给渲染器前先选择解码器的播放器。

语义标签可以通过 `openjoc_decoder_get_channel_label` 以及输出/帧描述结构获取。canonical PCM 采样格式值为 `1`（交错的 float32）。

把 `render_mode` 设为 `OPENJOC_RENDER_BINAURAL`，并把 `sofa_data` / `sofa_size` 设置为空/零，即可使用内置的离线 SADIE II 通用 HRTF。提供非空 SOFA 缓冲区时，会选择现有的严格用户数据集路径。如果 `virtual_layout` 为空，虚拟布局默认使用已配置的扬声器布局。设置 `speaker_layout = "22.2"` 可以选择原生 22.2 扬声器会话；其输出提供 24 个有序语义标签，包括 `LFE1` 和 `LFE2`。

C 适配器继承共享会话经过校准的默认 E-AC-3 Dialnorm 节目校准，除非显式把 `dialnorm_mode` 设为 `OPENJOC_DIALNORM_DIGITAL` 或 `OPENJOC_DIALNORM_ANALOG`。Default 推荐用于普通播放/解码。Digital 明确选择编码后的数字节目级校准。Analog 使用单位 Dialnorm 增益，是高级兼容/诊断策略，不是推荐的增大音量方式，也不是母带制作模式。Dialnorm 来自元数据，与现有 DRC 字段彼此独立；DRC 改变的是编码后的动态范围行为。FinalLinkedGain 是内部渲染器余量处理，不是用户的母带制作控制。

C ABI 是流式 PCM 接口，不执行文件导出峰值归一化，也不会为了文件级变换暂存完整节目。应用可以在收到 PCM 后自行应用最终静态增益策略。CLI 的 `--normalize-peak` 是文件输出的离线便利选项：它会在解码和渲染完成后，把最终文件归一化到请求的采样峰值；它不是 Dialnorm、DRC、限幅器、压缩器、LUFS 或真峰值归一化。

## 失败隔离

每个导出操作都会在返回前拦截 Rust panic。Rust panic、Rust 错误对象和 Rust 结构体布局都不会穿过 ABI。`last_error` 由解码器实例拥有，不是进程全局状态。空指针参数、无效结构大小、格式错误的数据包、不支持的配置、格式变化和渲染失败，都会返回数值状态码。

公共 C 头文件不包含第三方生成内容，并以仓库 Apache-2.0 许可证发布。
