# OpenJOC 工程规格书（Clean-room Reference Decoder）

**文档状态：** Implementation-ready / Codex Goal Mode 输入规格  
**目标项目名：** OpenJOC  
**建议实现语言：** Rust 2024 Edition  
**首要目标：** 依据公开标准独立实现 E-AC-3 JOC 的对象重建与 OAMD 解码，输出可验证的对象音频（object essences）与时间戳元数据；渲染器与耳机虚拟化不属于首版核心解码器。

---

## 0. 一句话目标

实现一个**不依赖 Dolby 私有实现、不复制私有 SDK 代码、以 ETSI TS 103 420 V1.2.1 为规范基线**的 E-AC-3 Joint Object Coding（JOC）参考解码器：

```text
E-AC-3 bitstream
    │
    ├─ channel-based downmix ──► 64-band complex QMF ──┐
    │                                                   │
    └─ EMDF ──► OAMD payload (11) ──► metadata ────────┤
             └─ JOC payload  (14) ──► side info ───────┤
                                                        ▼
                                     time/frequency reconstruction matrix
                                                        │
                                                        ▼
                                             object QMF essences
                                                        │
                                                        ▼
                                              inverse complex QMF
                                                        │
                                                        ▼
                                      ObjectScene + per-object PCM/WAV
```

首版成功标准不是“能出声”，而是：**规范一致、可测量、可导出、可追踪每一步中间状态，并能用用户自己制作的合法 Atmos/JOC 测试素材做定量验证。**

---

# 1. 权威来源与证据等级

## 1.1 唯一规范基线（Normative baseline）

### ETSI TS 103 420 V1.2.1 (2018-10)
标题：`Backwards-compatible object audio carriage using Enhanced AC-3`

官方 PDF：

`https://www.etsi.org/deliver/etsi_ts/103400_103499/103420/01.02.01_60/ts_103420v010201p.pdf`

本项目实现时必须以以下章节为准：

- Clause 4：OBA 概念与 decoder interface
- Clause 5：Object Audio Metadata (OAMD)
- Clause 6：Joint Object Coding (JOC)
- Clause 7：Complex QMF analysis/synthesis
- Clause 8：E-AC-3 与 EMDF 集成要求
- Annex A：JOC Huffman tables 等
- Annex B：OAMD → ADM 的规范化转换（可作为导出功能）

### ETSI TS 102 366 V1.4.1 (2017-09)
标题：`Digital Audio Compression (AC-3, Enhanced AC-3) Standard`

官方 PDF：

`https://www.etsi.org/deliver/etsi_ts/102300_102399/102366/01.04.01_60/ts_102366v010401p.pdf`

用途：

- E-AC-3 syncframe 解析
- independent/dependent substream
- skip field / EMDF carriage
- E-AC-3 channel-based downmix 解码边界

**Codex 不得凭记忆或网上二手描述猜测 TS 102 366 的 bitstream 字段。没有本地 PDF 时，应先实现可插拔 frontend，并把需要 TS 102 366 的部分明确标记为未完成，而不是编造。**

---

## 1.2 ETSI 官方伴随表文件（Normative companion data）

用户已提供：

`ts_103420v010201p0.zip`

本次已验证其 SHA-256：

```text
a79cf108c4529b7d9ca9525c871183a70b1732ed6df03a3d85b2f31be46eeced
```

ZIP 内只有：

```text
ts_103420_tables.c
```

解压后 SHA-256：

```text
4db8ae83e3c2e9269e88365be92a1a3ed6a9e6ee3851afac8ca03902723b1fcd
```

经实际解析确认包含：

```text
joc_huff_code_coarse_generic           95 nodes
joc_huff_code_fine_generic            191 nodes
joc_huff_code_coarse_coeff_sparse      95 nodes
joc_huff_code_fine_coeff_sparse       191 nodes
joc_huff_code_5ch_pos_index_sparse      4 nodes
joc_huff_code_7ch_pos_index_sparse      6 nodes
prot64                                 640 float coefficients
```

ETSI TS 103 420 Clause 7.4 规定本标准使用 64 个 QMF subbands、640 长度滤波器，并指出 companion ZIP 中包含 QMF coefficient table。附件 C 文件里的 640 项 `prot64[]` 应保留其原始 provenance；实现中可以映射为项目内部的 QMF prototype/window coefficients，但不要伪称原文件里存在未实际存在的 alias。

### 版权/分发策略

ETSI PDF 与 companion archive 有其自身版权声明。项目不得擅自把整份 ETSI 文档复制进仓库。

建议工程方式：

1. `references/etsi/` 默认由用户自行放置 PDF/ZIP，并加入 `.gitignore`；
2. `tools/import-etsi-tables` 验证上面的 SHA-256；
   1. 解析 `ts_103420_tables.c` 并生成本地 `target/generated/etsi_tables.rs`；

3. 是否把生成后的常量直接提交到公共仓库，必须在项目发布前单独确认许可/法律状态，不由 Codex 自行判断。

---

## 1.3 公开论文（Informative）

AES 140th Convention (2016)：

`Immersive Audio Delivery Using Joint Object Coding`

作者来自 Dolby Sweden AB。论文公开描述了 JOC 的总体思想：传输 multichannel downmix + parametric side information，由 decoder 重建对象。

用途：理解设计动机与听感/码率背景。**不能替代 ETSI normative algorithm。**

---

## 1.4 软件考古材料（仅交叉验证，不作为实现规范）

以下资料只用于确认架构血统，不得用于逐函数复制私有实现：

- 历史 MediaTek ALPS vendor trees 中的 `DDPDecoder.cpp`、`ddpdec_client.c`、`evo_parser.c`、`ARenderer.cpp`、`Dap2JocProcess.cpp`、OAMDI/DAP2 headers/stubs
- Broadcom WICED 历史 `udc_api.h`
- Google Project Zero 对 Pixel 9/Pixel 10 Dolby UDC 的公开分析

这些材料证明/交叉验证过：

- 64-band complex QMF 是长期存在的实现边界；
- Android 老 OMX 到现代 Codec2 的 UDC 血统连续；
- 现代 Pixel 的 UDC binary 仍出现 `DLB_CLqmf_analysisL` 等 QMF 相关函数；
- `dap_cpdp_init` 在现代实现中仍可见。

**但 OpenJOC 的算法实现必须可以仅靠 ETSI + 合法测试向量解释。**

---

# 2. Clean-room / 法律与品牌边界

## 2.1 允许作为实现依据

- ETSI TS 103 420
- ETSI TS 102 366
- ETSI 官方 companion attachment
- 公开学术论文
- 兼容许可证的开源代码（需记录 license 与具体用途）
- 用户自己制作/合法取得的 E-AC-3 JOC 测试向量
- 黑盒输入/输出互操作测试

## 2.2 禁止

- 从明确标注 confidential / proprietary 的 Dolby 源码逐函数翻写
- 通过改变量名/结构方式复制私有实现
- 将历史 vendor proprietary source 作为代码生成上下文直接喂给实现 Agent
- 将 Dolby/Atmos logo、认证标志或“官方兼容/认证”等措辞用于 OpenJOC 品牌

## 2.3 专利/IPR 风险

“公开标准可下载”不等于“所有专利权自动免费授权”。ETSI 自身明确提示：可能存在已声明或未声明的 essential/potentially essential IPR，ETSI 不保证完整性。

因此：

- 项目 README 必须写明：OpenJOC 是独立研究/互操作实现，不代表任何第三方认证；
- 技术实现成功与“全球无专利风险”是两件不同的事；
- 正式分发前应对 ETSI IPR / 相关 patent families 单独做法律审查。

建议代码许可证：`Apache-2.0`（贡献者专利授权条款更清晰），但这**不解决第三方专利**。

---

# 3. 标准已经确认的核心事实

以下都是实现时不可“自行发挥”的固定事实。

## 3.1 JOC 的角色

JOC 是 E-AC-3 decoder 的 post-processor。TS 103 420 明确规定：JOC tool 可以从 channel-based E-AC-3 bitstream 解出最多 **16 个 OBA essences**。

因此不要把“16”直接等价成某个固定 speaker layout。它是 JOC 对象/essence 重建上限；LFE 在 JOC downmix 配置中是 bypass，不参与 JOC matrix processing。

## 3.2 Downmix 配置

标准定义了若干 JOC downmix configuration，包括：

- 5.X
- 7.X
- 5.X + 2（含 Tfl/Tfr）
- 对应的 90° phase-shift 变体

JOC 实际参与处理的输入 channel count 为 5 或 7；LFE 若存在则 bypass。

## 3.3 JOC parameter bands

允许的 `joc_num_bands`：

```text
1, 3, 5, 7, 9, 12, 15, 23
```

参数按 parameter band 发送，再按 Clause 6.5 / Table 54 映射到 64 个 QMF subbands。

不要用“平均切成 N 段”代替 Table 54。

## 3.4 Quantization

JOC side information 支持两种量化步数：

```text
96
192
```

Clause 6.6.4 的 dequantization 必须原样实现其数学关系；不要做经验缩放、clamp 或“听起来更好”的修正。

## 3.5 Temporal behavior

JOC side information 支持：

- smooth：线性插值
- steep：无插值的跳变

并可包含一个或多个 data points / timeslot offsets。`joc_mix_mtx_prev` 是跨帧状态；首次 E-AC-3 frame 前标准要求其元素初始化为 0。

实现必须把 matrix state 与 frame state 明确建模，不允许写成无状态的单帧函数后再补 hack。

## 3.6 核心重建

最终对象重建本质为 complex-QMF domain 的 time/frequency-varying matrix multiply：

```text
object[obj, ts, sb] +=
    input[ch, ts, sb] * M[obj, ch, ts, sb]
```

其中 `M` 来自：

```text
JOC bitstream
 → Huffman decode
 → differential decode
 → dequantization
 → parameter-band → QMF-subband mapping
 → temporal interpolation
```

这是 OpenJOC 最核心的正确性路径。

## 3.7 Complex QMF

TS 103 420 Clause 7：

```text
subbands      = 64
filter length = 640
```

analysis：每 64 个 PCM samples 生成 64 个 complex subband samples。

synthesis：每 64 个 complex subband samples 生成 64 个 PCM samples。

首版必须有**直接对应规范的 reference implementation**，即便 O(N²)；FFT/SIMD 优化不得先于 reference path。

## 3.8 EMDF payload IDs

TS 103 420 Clause 8 / Table 55：

```text
OAMD payload_id = 11
JOC  payload_id = 14
```

如存在 dependent substreams，承载 OAMD/JOC 的 EMDF container 位于最后一个 dependent substream（按标准要求处理）。

---

# 4. 项目架构

推荐 Rust workspace：

```text
OpenJOC/
├── Cargo.toml
├── LICENSE
├── README.md
├── SECURITY.md
├── CONTRIBUTING.md
├── docs/
│   ├── architecture.md
│   ├── clean-room.md
│   ├── conformance.md
│   ├── test-vectors.md
│   └── research-provenance.md
├── references/
│   └── etsi/
│       ├── README.md
│       └── .gitignore
├── tools/
│   └── import-etsi-tables/
├── crates/
│   ├── openjoc-bitio/
│   ├── openjoc-eac3/
│   ├── openjoc-emdf/
│   ├── openjoc-oamd/
│   ├── openjoc-qmf/
│   ├── openjoc-joc/
│   ├── openjoc-scene/
│   ├── openjoc-wave/
│   ├── openjoc-cli/
│   └── openjoc-testkit/
├── tests/
│   ├── synthetic/
│   ├── vectors/
│   ├── golden/
│   └── fuzz/
└── benches/
```

原则：

**format decode、object reconstruction、scene metadata、renderer 必须分层。**

不要写成一个 5,000 行 `decoder.rs`。

---

# 5. 模块详细规格

## 5.1 `openjoc-bitio`

实现安全、边界检查严格的 MSB-first bit reader。

接口至少包含：

```rust
pub trait BitRead {
    fn read_bit(&mut self) -> Result<bool, BitError>;
    fn read_bits(&mut self, n: u8) -> Result<u64, BitError>;
    fn bits_remaining(&self) -> usize;
    fn byte_align(&mut self) -> Result<(), BitError>;
}
```

要求：

- 所有 length arithmetic 使用 checked arithmetic；
- 禁止 silent truncation；
- malformed payload 必须返回结构化错误；
- 为 fuzzing 设计，无 panic-on-input。

---

## 5.2 `openjoc-eac3`

职责：只处理 OpenJOC 需要的 E-AC-3 frontend 与 frame timing，不重复造完整高质量 E-AC-3 audio decoder。

### 推荐策略

首选把“base E-AC-3 audio decode”交给成熟开源 decoder（例如 FFmpeg/libavcodec），OpenJOC 自己负责：

- syncframe indexing
- substream relation
- frame/sample timing
- EMDF extraction
- 将 decoded channel-based downmix 与对应 EMDF frame 对齐

若 libavcodec 不暴露所需 EMDF 数据，则实现最小 E-AC-3 bitstream frontend，**但字段必须严格依照 TS 102 366 V1.4.1**。

必须提供一个绕过 E-AC-3 frontend 的低层入口，方便单测：

```rust
pub struct JocFrameInput<'a> {
    pub sample_rate: u32,
    pub downmix_pcm: &'a [Vec<f64>],
    pub joc_payload: &'a [u8],
    pub oamd_payload: &'a [u8],
    pub frame_index: u64,
}
```

这样即使 E-AC-3 frontend 尚未完成，也能独立证明 JOC/OAMD/QMF 核心正确。

---

## 5.3 `openjoc-emdf`

职责：

- 解析 EMDF container
- 枚举 payload
- 按 payload_id 分发
- 至少识别 OAMD=11、JOC=14
- 保留未知 payload，但默认不解释

所有 size / variable-length coding 必须有上限检查，避免复现历史 UDC 在 EMDF 长度处理上的内存安全问题。

Rust 实现必须做到：恶意输入只能返回 error，不得产生 OOB read/write、panic、极端内存分配。

---

## 5.4 `openjoc-oamd`

这是**元数据 parser**，不是 renderer。

必须按 TS 103 420 Clause 5 解析并维护：

- content description / program assignment
- object count / class
- bed / ISF / dynamic objects
- position
- size (width/depth/height)
- priority
- gain
- channel lock
- zone constraints
- divergence
- trim
- property update timing
- update/reuse semantics
- high-precision position extension

标准允许每个 object 每个 codec frame 多次 metadata updates，因此 API 不可只返回“每帧一个坐标”。

Bitstream parsing remains object-major as specified by clause 5.5.8, while the
renderer-independent scene timeline is emitted in shared-timing block-major
order. Since clause 5.3.2 defines one timing sequence for all objects, a
two-object frame with two updates is materialized at `t0,t0,t1,t1`; this keeps
temporal consumers ordered without introducing object/audio identity.

建议内部模型：

```rust
pub struct MetadataUpdate {
    pub object_id: u32,
    pub timing: Timing,
    pub position: Option<Position>,
    pub size: Option<Extent3>,
    pub priority: Option<f64>,
    pub gain_db: Option<f64>,
    pub channel_lock: Option<bool>,
    pub zone_constraints: Option<ZoneConstraints>,
    pub divergence: Option<f64>,
    pub trim: Option<ObjectTrim>,
}
```

### 必做导出

1. JSON scene timeline
2. Annex B 风格 ADM export（可以作为第二优先级，但架构必须预留）

---

## 5.5 `openjoc-qmf`

这是整个项目的 DSP 基础层。

必须同时实现：

```text
ReferenceQmf64F64
FastQmf64F32     (后续优化)
```

### Reference path 要求

严格按 Clause 7：

- 64 subbands
- 640-sample prototype/window
- analysis state length 640
- synthesis state length 1280
- complex modulation
- analysis/synthesis state 可 reset

禁止在 reference path：

- 替换 window
- 使用近似 coefficient
- 改 phase convention
- 自作主张 normalization
- 为“听感”修改结果

### Table loader

`tools/import-etsi-tables`：

1. 检查 official C file SHA-256；
2. 解析 `prot64[640]`；
3. 验证数量恰好 640；
4. 生成 Rust 常量；
5. 输出 provenance comment（source filename + hash）。

### QMF 测试

必须包括：

- impulse
- DC
- 1 kHz sine
- 多个接近 band boundary 的正弦
- white noise
- deterministic random signal

analysis→synthesis 需要自动估计固定 delay 后比较 reconstruction error；文档记录 delay、gain、最大误差、RMS error。不要硬编码来自 vendor 实现的非规范 latency 值。

---

## 5.6 `openjoc-joc`

核心 crate。

### 5.6.1 Parser

按 Clause 6.2 / 6.3 实现完整 JOC payload parser。

必须保留原始语义字段，不要一边读 bitstream 一边把所有东西压进临时浮点数组导致无法 debug。

建议模型：

```rust
pub struct JocFrame {
    pub header: JocHeader,
    pub info: JocInfo,
    pub objects: Vec<JocObjectFrame>,
}
```

`JocObjectFrame` 至少明确表示：

- object present
- band resolution
- sparse/full mode
- quantization mode
- data-point timing
- Huffman-coded values / decoded symbols

### 5.6.2 Huffman decoder

从 ETSI attachment 导入六棵 tree。

实现规则对应 Clause 6.6.3：

- node 从 0 开始；
- MSB-first 读取 codeword；
- 正 node → 继续走；
- 非正 node → leaf；
- leaf symbol 由规范定义的映射得到。

### 强制单测

不要只用手写几个 codeword。

对每棵 tree：

1. DFS 枚举所有 leaf 的 bit path；
2. 把 path 输入 decoder；
3. 每个 leaf 必须恰好 decode 回预期 symbol；
4. 验证无重复 leaf path；
5. 验证 prefix-free；
6. truncated path 返回 error。

这能把 Huffman 实现一次性锁死。

### 5.6.3 Differential reconstruction

严格按 Clause 6.6.2 分别实现：

- sparse path
- full-matrix path

禁止把二者“统一简化”为一个未经证明的通用公式。

关键状态：

```text
joc_mix_mtx_q[obj][dp][ch][pb]
```

`nquant` 只能为 96 或 192。

### 5.6.4 Dequantization

按 Clause 6.6.4。

必须提供单独的纯函数，便于 exhaustive test：

```rust
fn dequantize(q: u16, quant_mode: QuantMode) -> f64;
```

对所有 96/192 个合法输入枚举测试：

- finite
- monotonic
- 零点/中心位置正确
- 与直接按规范计算结果相同

### 5.6.5 Parameter-band mapping

Table 54 作为**精确映射表**实现。

支持：

```text
1, 3, 5, 7, 9, 12, 15, 23 bands
```

建立：

```rust
fn qmf_subband_to_parameter_band(num_bands: JocBandCount, sb: u8) -> u8;
```

对所有组合 `8 * 64 = 512` 个输入做 exhaustive test。

### 5.6.6 Temporal interpolation

实现 Clause 6.6.5：

- smooth / linear
- steep / transition
- previous-frame matrix state
- data point offsets

状态对象：

```rust
pub struct JocDecoderState {
    prev_matrix: ...,
    sequence_state: ...,
}
```

首次 frame 前 `prev_matrix` 为 0。

必须测试：

- 1 data point smooth
- 2 data points smooth
- 1 data point steep
- 2 data points steep
- frame boundary continuity
- splice/discontinuity behavior（严格依据规范）

### 5.6.7 Object reconstruction

输入：

```text
complex QMF input channels x[ch][ts][sb]
interpolated matrix M[obj][ch][ts][sb]
```

输出：

```text
complex QMF object essences z[obj][ts][sb]
```

reference implementation 使用 f64 complex。

必须每帧将输出 buffer 显式清零后再累加，避免 state contamination。

### Debug dump

CLI 必须可导出：

```text
raw JOC fields
Huffman decoded symbols
quantized matrix
 dequantized matrix
interpolated matrix
QMF input
QMF object output
```

推荐 `.json`（metadata）+ `.npy`/自定义 binary（大矩阵），并支持指定 frame/object 范围，避免文件爆炸。

---

## 5.7 `openjoc-scene`

OpenJOC 的核心产品不是“一个已经被 renderer 染色的 7.1.4 WAV”，而是**可检查的 object scene**。

当前证据边界要求将元数据、重建基与语义绑定分开：

```rust
pub struct ObjectScene {
    pub sample_rate: u32,
    pub duration_samples: u64,
    pub objects: Vec<MetadataObject>,
    pub metadata_timeline: Vec<MetadataUpdate>,
    pub trim_timeline: Vec<TrimUpdate>,
    pub reconstruction_basis: Option<ReconstructionBasis>,
    pub semantic_binding: SemanticBindingState,
}
```

`MetadataObject` 只保留 OAMD 对象身份与类别；它不携带 PCM。JOC 输出使用
独立的 `ReconstructionBasis`，其中的行只有结构索引，不携带 authored-object
identity。默认 `SemanticBindingState::Unresolved`，不得使用隐式
`object[i] = joc_row[i]`、dominant-row 或空间观察 fallback。

```rust
pub struct MetadataObject {
    pub object_id: u32,
    pub class: ObjectClass,
}

pub struct ReconstructionBasis {
    pub rows: Vec<AudioBuffer>,
}
```

输出目录：

```text
output/
├── scene.json
├── metadata/
│   ├── timeline.json
│   └── trim_timeline.json
├── diagnostics/
│   ├── reconstruction_basis.json
│   └── reconstruction_rows/row_000.wav
└── debug/                # 可选
```

这层保证 metadata fidelity 与 reconstruction-row diagnostics 可以独立评价。
Metadata-only `ObjectScene` 可通过；audio-bound `ObjectScene` 与 verified
authored-object PCM 在独立证据建立前不可通过。

---

## 5.8 Renderer：明确不进入 OpenJOC Core

不要为了“马上听起来像 Apple Atmos”而在 decoder 里塞 HRTF、speaker panner、room model、head tracking。

首版边界：

```text
OpenJOC = decode/reconstruct/scene export
OpenJOC Renderer = 独立项目/独立 crate（以后）
```

可以提供一个**极简 debug speaker renderer**用于试听，但不得把它作为 codec correctness 的判断依据。

---

# 6. CLI 规格

必须提供：

```bash
openjoc inspect input.ec3
```

输出：

- E-AC-3 frame/substream summary
- 是否检测到 extension type
- EMDF payload IDs
- OAMD/JOC payload presence
- JOC object count
- JOC downmix config
- parameter band / quantization / sparse/full 统计

```bash
openjoc decode input.ec3 -o output/
```

产出：

```text
scene.json
objects/*.wav
```

```bash
openjoc decode-payload \
  --downmix downmix.wav \
  --joc joc.bin \
  --oamd oamd.bin \
  -o output/
```

这个入口必须存在，用于将核心解码与 E-AC-3 frontend 解耦。

```bash
openjoc dump-joc input.ec3 --frame 0..20 --json joc.json
openjoc dump-oamd input.ec3 --json oamd.json
openjoc dump-matrix input.ec3 --frame 123 --object 4
openjoc qmf-check input.wav
```

可选：

```bash
openjoc export-adm input.ec3 -o scene.xml
```

---

# 7. 测试策略：这是项目能否“超过 Cavern”的关键

“吊打 Cavern”不能写成主观口号，必须变成可量化 benchmark。

## 7.1 用户自制 ground-truth 测试集

用户具备 Atmos 混音能力，应制作一套可公开/合法使用的最小测试工程：

### 单对象位置

- centre
- left/right
- front/back
- top/bottom
- known 3D coordinates

### 轨迹

- azimuth sweep
- elevation sweep
- front→rear
- diagonal 3D movement
- fast step
- slow continuous movement

### level / metadata

- gain automation
- size automation
- divergence
- channel lock
- object enable/disable
- multiple metadata updates per codec frame

### DSP probe

- impulse
- white noise
- pink noise
- single sine at multiple frequencies
- narrow-band noise around parameter-band boundaries
- correlated signals
- anti-correlated signals
- multiple crossing objects

由合法 Dolby encoder/toolchain 生成真实 E-AC-3 JOC，并保留原始 ADM/DAMF/authoring metadata 作为“意图 ground truth”。

---

## 7.2 三层正确性指标

### A. Metadata fidelity

比较：

```text
authoring scene ↔ OpenJOC OAMD scene
```

指标：

- object count
- object IDs/class
- coordinate error
- size error
- gain error
- update timestamp error
- divergence/trim/channel-lock correctness

### B. Reconstruction fidelity

针对已知输入：

- object stem waveform correlation
- per-band energy error
- phase error
- impulse timing
- cross-talk / leakage
- frame-boundary discontinuity

### C. Renderer-independent comparison

Cavern 与 OpenJOC 都先导出 object stems / metadata；不要先比较 binaural output。

真正有意义的结论是：

> 在同一 JOC bitstream 上，哪一个更接近原始 authoring object scene / reference object essence。

只有 benchmark 明显优于 Cavern，README 才可以写“更高 reconstruction fidelity”；在那之前不要在README和Cavern做比较。

---

# 8. Reference / Optimized 双实现策略

为了既保证规范正确，又为未来实时解码留空间：

```text
Reference path
  f64
  direct complex matrix modulation
  最少优化
  最大可读性
  conformance oracle

Fast path
  f32
  SIMD
  FFT/fast modulation
  cache-friendly matrix layout
  multithreading（谨慎）
```

规则：

- Fast path 每个 PR 必须跑 reference differential tests；
- 优化前后的 scene metadata 必须完全一致；
- audio 允许浮点容差，但必须设定误差阈值；
- 不允许为了速度改标准行为。

---

# 9. 安全性与鲁棒性

媒体 decoder 是攻击面。

要求：

- 所有 bitstream lengths checked
- allocation 有硬上限
- object count / channel count / bands / dpoints 全部 validate
- multiplication/addition checked
- malformed Huffman code 返回 error
- unknown/reserved fields 按标准处理
- fuzz target：EMDF、OAMD、JOC、E-AC-3 frontend
- `cargo fuzz` 或 libFuzzer integration
- fuzz 目标要求：无 panic、无 OOM、无 hang

特别针对 EMDF variable-length size 做 pathological test。

---

# 10. 代码质量硬约束

Codex 必须遵守：

1. 禁止 `unsafe`，除非有单独 design note + benchmark 证明必要；首版原则上 0 unsafe。
2. 不允许 `unwrap()`/`expect()` 处理外部输入。
3. 每个规范字段/算法函数 doc comment 写对应 ETSI clause。
4. 不复制 ETSI 大段原文；只写实现说明与 clause reference。
5. 所有 magic constants 写来源。
6. 所有表必须验证长度/hash。
7. 公共 API 具有 rustdoc。
8. `cargo fmt --check` / `cargo clippy -- -D warnings` / `cargo test` 全通过。
9. Windows/Linux/macOS CI。
10. release build 不能依赖网络。

---

# 11. Definition of Done（Codex 不得提前宣布成功）

## Mandatory

- [ ] Rust workspace 建立完成
- [ ] Bit reader 完整 + fuzzable
- [ ] official ETSI companion attachment importer + SHA-256 validation
- [ ] 6 棵 Huffman tree 全部 exhaustive tests 通过
- [ ] 64/640 complex QMF reference implementation 完成
- [ ] QMF analysis/synthesis test suite 完成
- [ ] JOC Clause 6.2/6.3 parser 完成
- [ ] sparse/full differential decoding 完成
- [ ] 96/192 dequantization 完成
- [ ] Table 54 parameter-band mapping 完成且 exhaustive test
- [ ] smooth/steep interpolation 完成
- [ ] object QMF reconstruction 完成
- [ ] inverse QMF object PCM 输出完成
- [ ] OAMD Clause 5 parser 完成
- [ ] ObjectScene JSON 完成
- [ ] per-object WAV export 完成
- [ ] `decode-payload` 端到端通过
- [ ] E-AC-3 + EMDF frontend 可直接读取真实 `.ec3/.eac3` JOC 输入
- [ ] `openjoc inspect` 正确识别 JOC/OAMD
- [ ] real legal JOC vector 至少一条端到端 decode 成功
- [ ] malformed/fuzz tests 不 panic
- [ ] CI 三平台通过
- [ ] clean-room/provenance docs 完成

## Strongly desired

- [ ] Annex B ADM export
- [ ] matrix/QMF debug dump
- [ ] benchmark harness（OpenJOC vs original authoring scene）
- [ ] optional Cavern comparator importer（仅测试，不作为实现依赖）
- [ ] optimized f32 path

---

# 12. Codex 工作方式（Goal Mode）

Codex 的任务不是“一次生成很多代码然后停止”，而是**在一个 Goal 内循环执行：研究 → 实现 → 编译 → 测试 → 修复 → 端到端验证**。

必须遵循：

```text
1. 先读取本规格书
2. 检查 references/etsi 中的官方文件
3. 建立 RESEARCH_NOTES.md
4. 建立 REQUIREMENTS_MATRIX.md：每个 ETSI clause → source module → tests
5. 先写 reference implementation
6. 每实现一层立刻写测试
7. cargo test
8. 发现失败就修复，不允许跳过/disable test
9. 完成 real vector end-to-end 后才进入优化
10. 最终生成 IMPLEMENTATION_REPORT.md
```

`REQUIREMENTS_MATRIX.md` 至少追踪：

```text
ETSI 4.4    decoder interface        -> openjoc-scene
ETSI 5      OAMD                     -> openjoc-oamd
ETSI 6.2/3  JOC syntax/semantics     -> openjoc-joc
ETSI 6.5    band mapping             -> openjoc-joc
ETSI 6.6.2  differential decode      -> openjoc-joc
ETSI 6.6.3  Huffman                  -> openjoc-joc
ETSI 6.6.4  dequantization           -> openjoc-joc
ETSI 6.6.5  interpolation            -> openjoc-joc
ETSI 6.6.6  object reconstruction    -> openjoc-joc
ETSI 7      QMF                      -> openjoc-qmf
ETSI 8      E-AC-3/EMDF integration  -> openjoc-eac3/openjoc-emdf
Annex B     ADM conversion           -> openjoc-scene
```

---

# 13. 禁止 Codex 做的“看似聪明但会毁项目”的事情

- 不要只因为某个 WAV 能播放就宣告 decoder 正确；
- 不要把 5.1 downmix 直接当对象输出；
- 不要假装 FFmpeg 普通 E-AC-3 decode 已经重建 JOC objects；
- 不要用 HRTF/rendered output 掩盖 object reconstruction 错误；
- 不要把 OAMD object coordinates 用“常见 Atmos 坐标”猜出来；
- 不要把 64 QMF bands 换成 STFT；
- 不要用随机/平均 band mapping 替代 Table 54；
- 不要把 smooth interpolation 改成 nearest/linear in wrong domain；
- 不要忽略 cross-frame `prev_matrix`；
- 不要把历史 proprietary Dolby source 复制进 repo；
- 不要因为测试向量少就 hard-code 某个文件的 object count/layout；
- 不要在没有 benchmark 的情况下宣称“bit-perfect Dolby decoder”。

---

# 14. 第一版 README 应该如何描述项目

建议：

> OpenJOC is an independent, clean-room implementation of the object-audio decoding process specified by ETSI TS 103 420 for backwards-compatible object audio carriage using E-AC-3. The project focuses on reconstructing object audio essences and decoding OAMD metadata into an inspectable object scene. It is not affiliated with, endorsed by, or certified by Dolby Laboratories.

随后明确：

- normative source: ETSI TS 103 420 / TS 102 366
- no proprietary Dolby source included
- no renderer certification claims
- patents/IPR may apply; users/distributors are responsible for review

---

# 15. 最终项目验收场景

## Scenario 1：规范级核心

输入：

```text
known downmix PCM
+ known JOC payload
+ known OAMD payload
```

输出：

```text
object PCM stems
+ scene.json
```

必须能逐 frame dump：

```text
parsed JOC
quantized matrix
dequantized matrix
interpolated matrix
object QMF
metadata updates
```

任何差异都能定位到具体阶段。

## Scenario 2：真实 E-AC-3 JOC

输入：用户通过合法官方 encoder 从已知 ADM/DAMF 工程编码的 `.ec3`。

执行：

```bash
openjoc inspect known_scene.ec3
openjoc decode known_scene.ec3 -o result/
```

比较：

```text
original authoring metadata
vs
result/scene.json
```

并比较对象 stem 位置/能量/时序。

## Scenario 3：与 Cavern 进行公平对照

必须在 renderer 前比较：

```text
same JOC file
  ├─ Cavern object reconstruction
  └─ OpenJOC object reconstruction

        ↓
compare against known authoring ground truth
```

最终依据数字决定优劣，而不是“耳朵觉得谁更 Atmos”。

---

# 16. 我们这一晚真正得出的工程结论

最初最大的黑箱是：

```text
E-AC-3 downmix
    ↓
???? Dolby JOC magic ????
    ↓
objects
```

现在已经变成完全工程化的链路：

```text
E-AC-3 downmix
    ↓
64-band complex QMF
    ↓
JOC payload parse
    ↓
Huffman decode
    ↓
differential matrix decode
    ↓
96/192-step dequantization
    ↓
1/3/5/7/9/12/15/23 parameter-band mapping
    ↓
smooth/steep temporal interpolation
    ↓
time × frequency reconstruction matrix
    ↓
complex-QMF object reconstruction
    ↓
inverse QMF
    ↓
object PCM essences

OAMD payload
    ↓
object metadata + timed updates
    ↓
ObjectScene
```

也就是说，**OpenJOC 已经不需要复刻 Dolby UDC、OAMDI 或 DAP2 才能成立。**

- UDC：Dolby 的具体实现，不是规范本身；
- OAMDI：Dolby 的 OAMD data-model implementation，不是唯一实现；
- DAP2：renderer/post-processing，位于 OpenJOC core 的输出边界之后。

OpenJOC 应把“格式解码的正确性”和“最终空间渲染的主观品质”彻底拆开。

---

# 17. 关键官方参考链接

1. ETSI TS 103 420 V1.2.1  
   https://www.etsi.org/deliver/etsi_ts/103400_103499/103420/01.02.01_60/ts_103420v010201p.pdf

2. ETSI TS 103 420 companion archive  
   https://www.etsi.org/deliver/etsi_ts/103400_103499/103420/01.02.01_60/ts_103420v010201p0.zip

3. ETSI TS 102 366 V1.4.1  
   https://www.etsi.org/deliver/etsi_ts/102300_102399/102366/01.04.01_60/ts_102366v010401p.pdf

4. AES 140 / “Immersive Audio Delivery Using Joint Object Coding”  
   https://secure.aes.org/forum/pubs/conventions/?elib=18285

5. Google Project Zero / Pixel 9 Dolby UDC research  
   https://projectzero.google/2026/01/pixel-0-click-part-1.html

6. Google Project Zero / Pixel 10 follow-up  
   https://projectzero.google/2026/05/pixel-10-exploit.html

---

# 18. 给实现 Agent 的最后一句话

**不要模仿 Dolby 的代码；复现 ETSI 所定义的行为。不要以“能播放”为目标；以“每一个 bitstream field、每一张 matrix、每一个 QMF sample 都能解释和验证”为目标。Reference correctness first, optimization second, renderer last.**

### 5.7.1 Semantic binding evidence contract (J1R13)

`SemanticBindingState` is deliberately limited to `Unresolved` in the current
release. Evidence strength is a separate, serializable
`SemanticBindingEvidence` record with an explicit relation, scope, provenance,
supporting observations, contradictions, negative controls, producer/carrier
constraints, evidence dimensions, and falsifier. Its classes are
`structural`, `empirical`, and `verified`; none of the first two is a semantic
identity claim.

The admission validator requires independent WHO/WHERE/SLOT/ROW (or basis),
audio identity, context, timing, repeatability, negative-control, and
cross-state evidence. Equal row counts, equal indices, a dominant row, one
fixture, field-name similarity, or a single tone/carrier can never satisfy the
contract. A private-field capability token is the only result of a successful
synthetic contract check; J1R13 admits no real binding and provides no
conversion from that token to `ObjectScene`.

Metadata-only `ObjectScene` and diagnostic `ReconstructionBasis` row export
remain admissible; audio-bound ObjectScene and verified authored-object PCM
remain blocked. Clean-room provenance excludes proprietary, decompiled,
leaked, and unknown-constant evidence.
