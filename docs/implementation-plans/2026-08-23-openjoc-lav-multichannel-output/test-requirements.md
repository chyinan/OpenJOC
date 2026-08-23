# Test Requirements — OpenJOC-LAV Multichannel Output

NO_GAPS

25 条 Acceptance Criteria 均有明确的 phase/task、RED 路径、GREEN 证据和禁止性断言。`NO_GAPS` 表示计划覆盖完整，不表示实现或环境证据已经通过。

## 通用测试规则

- 测试使用 Arrange–Act–Assert；一次测试只验证一个行为。
- RED 必须因缺少目标能力或违反目标约束而失败，不能因 fixture、依赖或构建环境损坏而失败。
- 异步图状态以条件等待为准，不用任意 `sleep` 充当成功证据。
- controlled sink 只证明受控环境行为，不能升级为真实 renderer 或 PotPlayer 支持。
- 每次 native/PotPlayer 证据必须绑定代码 HEAD、运行时二进制绝对路径及 SHA-256。
- 长生命周期资源、注册表覆盖、COM 注册和 PotPlayer 设置必须由 RAII/finally 恢复并验证恢复结果。

## AC1 — Output policy is explicit and stable

### AC1.1 — 默认 Stereo 与 released path 保持一致

- **Requirement:** 新 filter 默认选择 Stereo；配置为 `OPENJOC_RENDER_STEREO`、不带 speaker preset，并产生 float32/48 kHz、两声道、`FL FR` 输出。
- **Test level:** integration、e2e。
- **RED expectation:** 新实例不是 Stereo，使用 speaker render 模式或 preset；或者输出不是两声道 float32/48 kHz、与既有 Stereo contract 不同。
- **GREEN/pass evidence:** decoder/settings smoke 证明首次实例和缺失/非法注册表状态均为 Stereo；真实 `openjoc_capi.dll` 输出两声道及精确 metadata；严格图测试的完整 media type 和 PCM channel fingerprints 符合 Stereo contract。
- **Phase/task:** Phase 2 Task 1–2；Phase 4 Task 1、Task 3；Phase 5 Task 2。
- **关键禁止性断言:** 仅断言枚举值为 `0` 不足以 PASS；不得把普通非 extensible stock Stereo media type 当作 strict OpenJOC Stereo 证据。

### AC1.2 — 每个 manual preset 使用精确公开 ABI preset

- **Requirement:** 5.1、7.1、5.1.2、5.1.4、7.1.2、7.1.4 均使用 `OPENJOC_RENDER_SPEAKER` 和 canonical table 中精确的 built-in preset name。
- **Test level:** unit、integration。
- **RED expectation:** 缺少 policy API；任一 preset 使用 Stereo 模式、近似名称、UI display text 或由 channel count 推断名称。
- **GREEN/pass evidence:** contract unit test 固定六个 preset 的 ABI 名称；decoder smoke 用真实 C API 对七个 policy 验证 count、layout name 和 channel labels。
- **Phase/task:** Phase 1 Task 1–2；Phase 2 Task 1–2。
- **关键禁止性断言:** 不得通过 channel count、FFmpeg display name 或 property-page 文本证明 ABI preset 正确。

### AC1.3 — policy 变更先重建 decoder

- **Requirement:** 实际 policy 变更必须销毁旧 stream decoder、丢弃 pending frames、重置 admission/counters，再用新 contract 创建 decoder；同值设置为 no-op。
- **Test level:** integration、e2e。
- **RED expectation:** 5.1→7.1.4 后仍可读到旧 frame、旧 counter 未清零、首个后续 frame 带旧 contract，或同值设置造成无意义重建。
- **GREEN/pass evidence:** live decoder switch test 在 pending output 存在时切换，证明旧帧不可用、状态重置、所有后续 frame 仅携带新 contract；graph renegotiation 得到新精确 media type。
- **Phase/task:** Phase 2 Task 1；Phase 4 Task 1；Phase 5 Task 3。
- **关键禁止性断言:** 不得仅以 registry DWORD 已改变或 getter 返回新值证明 decoder 已重建。

### AC1.4 — render target 不由非语义输入选择

- **Requirement:** carrier channel count、endpoint/product display name、physical subwoofer count 和 filename 均不能选择或改变 policy。
- **Test level:** unit、integration、manual evidence。
- **RED expectation:** 存在接受名称、文件名、carrier count、consumer notation 或 physical-sub count 的 lookup/parser；改变这些值会改变输出 contract。
- **GREEN/pass evidence:** contract lookup 只接受固定 enum；不存在 name/count parsing API；property page 使用 `CB_SETITEMDATA`；raw/MP4、不同 renderer inventory 下固定 policy 始终生成相同 contract。
- **Phase/task:** Phase 1 Task 1；Phase 4 Task 2；Phase 5 Task 2；Phase 6 Task 2–3。
- **关键禁止性断言:** 不得解析 `CABLE In 16 Ch`、文件名、`.2` 文本、`AudSpkIndex_new` 或 endpoint properties 来决定布局。

### AC1.5 — 无证据时不得暴露 Auto

- **Requirement:** 没有跨 Stereo、5.1 和至少一个 height-capable downstream 的标准化语义偏好证据时，Auto 不得出现在 enum、ABI、UI、evidence shipped list 或文档中。
- **Test level:** unit、integration、manual evidence。
- **RED expectation:** 任一 public enum/string、combo row、文档或 evidence 状态把 Auto 声明为可用或可靠。
- **GREEN/pass evidence:** contract/shipped-list tests 证明 Auto 缺席；evidence validator 和 repository hygiene 要求 `AUTO_NOT_RELIABLE`；最终文档不宣称自动布局选择。
- **Phase/task:** Phase 1 Task 1；Phase 4 Task 2；Phase 6 Task 1、Task 3–4。
- **关键禁止性断言:** `EnumMediaTypes`、endpoint channel count 或 renderer 接受多个格式不能证明存在 unambiguous Auto preference。

## AC2 — Canonical logical semantics

### AC2.1 — 七个 canonical contracts 精确匹配

- **Requirement:** 七个候选必须精确匹配下表语义：

| Policy | Count | Order | Mask |
|---|---:|---|---:|
| Stereo | 2 | FL FR | `0x00000003` |
| 5.1 | 6 | FL FR FC LFE Ls Rs | `0x0000060f` |
| 7.1 | 8 | FL FR FC LFE Lb Rb Ls Rs | `0x0000063f` |
| 5.1.2 | 8 | FL FR FC LFE Ls Rs TFL TFR | `0x0000560f` |
| 5.1.4 | 10 | FL FR FC LFE Ls Rs TFL TFR TBL TBR | `0x0002d60f` |
| 7.1.2 | 10 | FL FR FC LFE Lb Rb Ls Rs TFL TFR | `0x0000563f` |
| 7.1.4 | 12 | FL FR FC LFE Lb Rb Ls Rs TFL TFR TBL TBR | `0x0002d63f` |

- **Test level:** unit、integration。
- **RED expectation:** table 缺失、行数不是七、任一 name/count/order/mask 不符，或 5.1 side/back 语义混淆。
- **GREEN/pass evidence:** C++ contract tests 与 Rust canonical layout test 独立通过；真实 decoder frame metadata 和 `AVChannelLayout` 与对应行完全一致。
- **Phase/task:** Phase 1 Task 1–2；Phase 2 Task 1–2。
- **关键禁止性断言:** 不得以相同 channel count 判定两个 contract 相同；7.1 与 5.1.2、5.1.4 与 7.1.2 必须保持不同语义。

### AC2.2 — PCM 顺序等于 mask 升序 set-bit 顺序

- **Requirement:** 每行 ordered channels 必须等于 Windows mask 按位从低到高的 speaker order，无静默 reorder。
- **Test level:** unit、e2e。
- **RED expectation:** mask popcount 虽正确但 order 不同；side/back 发生交换；捕获声道 fingerprint 与 decoder oracle 顺序不一致。
- **GREEN/pass evidence:** unit test 验证 `ordered_channels == ascending_set_bits(mask)`；raw/MP4 strict capture 的每声道时间序列和完整 interleaved bytes 与 direct decoder oracle 相同。
- **Phase/task:** Phase 1 Task 1–2；Phase 2 Task 2；Phase 5 Task 2。
- **关键禁止性断言:** 不得只比较声道数、mask popcount 或无声/相同声道 fixture。

### AC2.3 — 无精确 mask 的布局必须拒绝

- **Requirement:** zero mask、reserved/unmapped bits、mask/count mismatch、count-only default 和未知 policy 不得生成 contract 或 strict media type。
- **Test level:** unit、integration。
- **RED expectation:** `av_channel_layout_default(count)`、mask `0`、保留位或仅凭 count 仍返回成功。
- **GREEN/pass evidence:** contract 和 strict-output negative tests 对全部非法输入返回失败且不留下 partially initialized layout/media type。
- **Phase/task:** Phase 1 Task 1–2；Phase 2 Task 2；Phase 3 Task 1。
- **关键禁止性断言:** “Windows 能表示该 count”或“mask 合法”不等于该布局已 canonicalized 或受支持。

### AC2.4 — logical LFE 数量精确且不受物理低音炮数量影响

- **Requirement:** Stereo 为 `FL FR`，恰有零个 LFE bit；其余六个 multichannel contract 各恰有一个。5.1.2/7.1.2 的 `.2` 表示 TFL/TFR，不表示第二个 subwoofer。
- **Test level:** unit、integration、manual evidence。
- **RED expectation:** Stereo 出现 LFE、任一 multichannel contract/evidence 行不是恰好一个 LFE、出现 `physical_subwoofer_count` 字段，或 consumer `.2` 被解析为第二 LFE。
- **GREEN/pass evidence:** contract tests 逐行统计 Stereo `LFE == 0`、其余六行 `LFE == 1`；evidence validator 要求对应的 `logical_lfe_channels`；文档 hygiene 验证 logical/physical subwoofer 分离。
- **Phase/task:** Phase 1 Task 1–2；Phase 6 Task 1、Task 3。
- **关键禁止性断言:** 不得从 AVR/endpoint 的物理 subwoofer 数推导 PCM channel 或 mask。

## AC3 — Exact DirectShow negotiation

### AC3.1 — strict media type 字段完全精确

- **Requirement:** 七个 media type 均为 float32/48 kHz `WAVE_FORMAT_EXTENSIBLE`，包含精确 channels、32-bit container/valid bits、IEEE-float subformat、mask、block alignment、average byte rate、sample size 和完整 format bytes。
- **Test level:** unit、integration、e2e。
- **RED expectation:** builder 缺失；Stereo 使用 non-extensible format；任一字段、`cbSize=22`、mask 或 checked arithmetic 不匹配。
- **GREEN/pass evidence:** strict-output tests 按字段和完整 bytes 比较全部七行；e2e requested、两端 `ConnectionMediaType`、sample-attached type 和 post-stream type 完全相等。
- **Phase/task:** Phase 3 Task 1；Phase 5 Task 2。
- **关键禁止性断言:** 不得只比较 major/subtype、channels 或 mask；padding、flags、sample size 和全部 format bytes 都必须纳入相等性。

### AC3.2 — 仅 exact streaming 可声明支持

- **Requirement:** 支持声明必须来自同一 named renderer/endpoint 下的 exact connect、精确 `ConnectionMediaType`、Pause、Run、raw/MP4 sample delivery、EOS 和无 graph error；PotPlayer 支持还要求同 renderer/endpoint 的 Source-as-Output 实际图证据。
- **Test level:** e2e、manual evidence。
- **RED expectation:** 缺少任一连接类型、状态、sample/EOS、same-instance admission、host/renderer identity 或 runtime-module evidence 时 validator 拒绝 `STREAM_PROVEN`。
- **GREEN/pass evidence:** native exact-renderer raw+MP4 行和 PotPlayer raw+MP4 行均完整；实际图 status 显示固定 policy、`OpenJoc` admission 及精确 format/rate/count/mask。
- **Phase/task:** Phase 5 Task 2；Phase 6 Task 1–2、Task 4。
- **关键禁止性断言:** controlled sink PASS 不得升级为 renderer/PotPlayer support；注册 CLSID、helper getter 或截图单独均不足以证明实际图。

### AC3.3 — exact rejection 不得 fallback

- **Requirement:** exact type 被拒绝时返回原始/标准化失败 HRESULT，不尝试 int16、5.1-back、7.1、Stereo、current layout 或其他 mask。
- **Test level:** integration、e2e。
- **RED expectation:** rejection trap 观察到第二个 proposal、sample delivery、connection type mutation 或 stock fallback chain 被调用。
- **GREEN/pass evidence:** post-bootstrap rejection 只出现一次 exact dynamic proposal；无第二 type、无 sample、两端原连接类型不变，并记录精确 failure stage/HRESULT。
- **Phase/task:** Phase 3 Task 3；Phase 5 Task 2；Phase 6 Task 2、Task 4。
- **关键禁止性断言:** 负面测试不得从未连接状态直接 `ConnectDirect(..., exactTarget)`；必须先建立 non-target bootstrap connection 才能观察动态 fallback。

### AC3.4 — probe/representability 不能形成 PASS

- **Requirement:** `QueryAccept`、`EnumMediaTypes`、legal mask、endpoint properties 或 channel count 单独均不能形成支持结论。
- **Test level:** unit、e2e。
- **RED expectation:** evidence state machine 接受 mask-only、QueryAccept-only、Pause/Run 零 samples 或 endpoint inventory 行。
- **GREEN/pass evidence:** synthetic validator tests 逐一拒绝上述假阳性；controlled sink test 要求完整 connect/type/state/samples/EOS。
- **Phase/task:** Phase 3 Task 1、Task 3；Phase 5 Task 2；Phase 6 Task 1。
- **关键禁止性断言:** “representable”“proposed”“accepted by QueryAccept”和“STREAM_PROVEN”必须是不同状态。

## AC4 — Stock LAV isolation

### AC4.1 — ordinary E-AC-3 与 pristine stock 相同

- **Requirement:** 所有 policy 下，ordinary non-JOC E-AC-3 必须走 stock admission、decoder、postprocessor 和 delivery，并与独立 pristine start-HEAD control 的类型、PCM、samples 和 EOS 一致。
- **Test level:** e2e。
- **RED expectation:** 任一 policy 进入 OpenJOC、输出与 pristine 不同、使用 strict no-fallback lane，或所谓 control 实际来自修改后 build。
- **GREEN/pass evidence:** 七个 policy 的 target-vs-pristine matrix 精确匹配；证据分别记录 target/pristine filter 和依赖模块路径/hash。
- **Phase/task:** Phase 5 Task 1、Task 3；Phase 6 Task 4。
- **关键禁止性断言:** 不得用修改分支加 `EnableOpenJOC=false` 代替 pristine control。

### AC4.2 — passthrough 优先于 OpenJOC admission

- **Requirement:** `Bitstream_EAC3` 启用时，七个 policy 均不得向 OpenJOC stream decoder 输入字节，bitstream media type/bytes 必须匹配 pristine。
- **Test level:** e2e。
- **RED expectation:** 任一 policy 的 OpenJOC stream-input byte count 非零，或 bitstream 输出与 pristine 不同。
- **GREEN/pass evidence:** 七行均记录 OpenJOC input bytes `0`，且 bitstream media type/bytes 与独立 pristine control 一致。
- **Phase/task:** Phase 5 Task 3；Phase 6 Task 4。
- **关键禁止性断言:** 不得仅检查 UI passthrough checkbox 或最终有声音；必须证明 OpenJOC decoder 未进入。

### AC4.3 — policy 不影响 stock input/fallback

- **Requirement:** OpenJOC policy 不能改变 stock input media-type selection 或 generic fallback 顺序/行为。
- **Test level:** integration、e2e。
- **RED expectation:** 设置任一 policy 后 ordinary stock buffer 进入 strict branch、fallback 被抑制或 media-type selection 改变。
- **GREEN/pass evidence:** unit/integration stock buffer control 仍可走既有 generic fallback；ordinary E-AC-3 在七个 policy 下均匹配 pristine 路径和输出。
- **Phase/task:** Phase 3 Task 3；Phase 5 Task 3。
- **关键禁止性断言:** 不能用“最终输出看起来相同”替代 admission、media type、fallback 和 module identity 证据。

### AC4.4 — stock mixing 不替代或复制 speaker rendering

- **Requirement:** strict OpenJOC buffer 必须绕过所有会 remix、conform、replace、expand 或 substitute 布局的 stock 处理；不得重复生成声道。
- **Test level:** integration、e2e。
- **RED expectation:** mixer/layout 设置改变 strict bytes/mask；7.1 与 5.1.2 因同为八声道而 coalesce；strict/stock bytes 被合并。
- **GREEN/pass evidence:** all-settings postprocessor test 保持 exact contract；不同 contract/strict-stock transition 只 flush 一次；capture 每声道 fingerprint 和 interleaved bytes 与 direct decoder oracle 相同。
- **Phase/task:** Phase 3 Task 2；Phase 5 Task 2。
- **关键禁止性断言:** 不得仅以最终 channel count 不变证明未 remix；必须验证 mask、声道 fingerprint 和 byte order。

## AC5 — Lifecycle and memory safety

### AC5.1 — 生命周期边界保留 policy 且无 stale state

- **Requirement:** initial playback、forward/back seek、flush/new segment、EOS、stop/reopen、graph rebuild 和 media-type renegotiation 后保持所选 contract。
- **Test level:** integration、e2e、manual evidence。
- **RED expectation:** 任一边界后回到 Stereo、出现旧 policy frame、缺少 flush/new-segment/EOS、连接类型残留或 policy change 未重建 decoder。
- **GREEN/pass evidence:** raw/MP4 七 policy lifecycle matrix 在每个边界重新读取完整 `ConnectionMediaType`；观察 BeginFlush/EndFlush/NewSegment、EOS、Stop→seek zero→Run 和 graph rebuild 成功。
- **Phase/task:** Phase 2 Task 1；Phase 5 Task 3；Phase 6 Task 2、Task 4。
- **关键禁止性断言:** 不得只在 initial connect 读取一次 media type 后推定所有生命周期阶段正确。

### AC5.2 — 所有 byte count 使用 checked arithmetic

- **Requirement:** frame、queue、allocator 和 delivery 的 addition/multiplication/growth/narrowing 必须先检查，再分配、设置长度或复制。
- **Test level:** unit、integration、e2e。
- **RED expectation:** overflow wraparound、allocation failure 仍修改旧 buffer、`Append` 无条件返回成功，或 undersized sample 进入 copy。
- **GREEN/pass evidence:** checked-helper boundary tests、GrowableArray failure tests 和 allocator e2e 证明失败时旧内容/count 不变；日志记录 required/actual capacity、checked bytes 和 high-water。
- **Phase/task:** Phase 2 Task 1–2；Phase 3 Task 1–3；Phase 5 Task 2、Task 4。
- **关键禁止性断言:** 不得以当前七个合法 layouts 数值较小为由省略 overflow/narrowing 测试。

### AC5.3 — oversized 输入在任何副作用前失败

- **Requirement:** oversized sample/channel/count、invalid source length、queue overflow、allocator shortfall 必须在 copy、append、growth、`SetActualDataLength` 或 delivery 前失败。
- **Test level:** unit、integration、e2e。
- **RED expectation:** invalid frame 部分复制；queue count 已变；allocator `requiredBytes-1` 仍调用 memcpy/设置长度；失败后有 sample delivery。
- **GREEN/pass evidence:** decoder、strict queue 和 allocator tests 逐层证明 fail-before-side-effect；exact capacity 对照成功；错误 HRESULT 稳定可记录。
- **Phase/task:** Phase 2 Task 1–2；Phase 3 Task 1–3；Phase 5 Task 2、Task 4。
- **关键禁止性断言:** “发生异常但进程未崩溃”不是 PASS；必须证明失败点早于每个受保护副作用。

### AC5.4 — 性能运行无 underrun 或无界增长

- **Requirement:** warm-up 后，Stereo、5.1 和最大 validation candidate 7.1.4 至少运行 128 个 graph cycles；sample/byte counts 可重复，timestamps 连续，EOS 正常，allocator high-water 稳定，working set 不呈随 cycle 数线性增长。
- **Test level:** e2e、manual evidence。
- **RED expectation:** sample/byte 数变化、timestamp discontinuity、graph error、EOS 缺失、allocator high-water 持续抬升或 post-warm-up working set 呈持续线性增长。
- **GREEN/pass evidence:** 三行性能日志包含 elapsed time、cycles、samples、bytes、timestamps、EOS、allocator high-water 和 working-set 序列；自动判定所有功能计数稳定且无持续增长趋势。
- **Phase/task:** Phase 5 Task 4；Phase 6 Task 4。
- **关键禁止性断言:** 性能 PASS 不能推导 renderer/endpoint support；单次短跑或只记录最终 working set 不足以通过。

## AC6 — Settings and evidence honesty

### AC6.1 — UI 只暴露 shipped-evidenced presets

- **Requirement:** property page 只列 Stereo 和最终 evidence 中 `STREAM_PROVEN` 的 presets；persistence 仅位于 `Software\LAV\Audio\OpenJOC`。
- **Test level:** unit、integration、manual evidence。
- **RED expectation:** 初始 Phase 4 UI 包含未验证 layout/Auto；最终 shipped list 与 proven set 不相等；unsupported/unverified row 出现在 combo；registry 写入 stock namespace。
- **GREEN/pass evidence:** Phase 4 shipped list 初始仅 Stereo；Phase 6 `--list-shipped` 与 evidence `STREAM_PROVEN` set 完全相等；registry override tests 证明 version/policy DWORD 仅存在于 isolated namespace。
- **Phase/task:** Phase 4 Task 1–3；Phase 6 Task 1、Task 3–4。
- **关键禁止性断言:** representable contract、controlled sink PASS 或 legal mask 均不得自动加入 shipped UI。

### AC6.2 — 使用独立 OpenJOC interface，stock ABI 不变

- **Requirement:** programmatic policy 通过 `ILAVOpenJocSettings` 固定 IID 和固定 `uint32_t` wire values；不得修改 `ILAVAudioSettings` IID、成员、顺序或 vtable。
- **Test level:** unit、integration。
- **RED expectation:** target 返回 `E_NOINTERFACE`；stock 暴露新 interface；旧 IID 不可调用；`ILAVAudioSettings` declaration/vtable 发生变化。
- **GREEN/pass evidence:** target set/get/default/invalid/round-trip smoke 通过；stock 对新 IID 仍返回 `E_NOINTERFACE`；旧 IID 两个 build 均可用；声明 diff byte-for-byte 不变。
- **Phase/task:** Phase 1 Task 1；Phase 4 Task 1、Task 3。
- **关键禁止性断言:** 不得通过扩展旧 interface、复用 stock IID 或只检查编译成功来宣称 ABI 兼容。

### AC6.3 — 三种 evidence 状态语义严格区分

- **Requirement:** 最终每行必须为 `STREAM_PROVEN`、`UNSUPPORTED` 或 `UNVERIFIED`；失败行记录精确 stage/HRESULT，未充分执行行记录 reason。
- **Test level:** unit、e2e、manual evidence。
- **RED expectation:** unexecuted row 被标为 unsupported；changed type/missing sample 被标为 proven；failure 缺 stage/HRESULT；mandatory row 非 proven 仍允许完成。
- **GREEN/pass evidence:** synthetic validator fixtures 覆盖三状态和所有 false-positive；七个真实 candidate 均有完整状态；shipped list 仅等于 proven rows。
- **Phase/task:** Phase 6 Task 1–4。
- **关键禁止性断言:** `UNSUPPORTED` 只表示已测量 exact rejection/mutation；`UNVERIFIED` 不能被文案弱化成“可能支持”或升级为 shipped support。

### AC6.4 — 文档不得夸大物理设备能力

- **Requirement:** 文档只声明有对应 exact evidence 的环境能力，不声明 automatic physical-device adaptation、物理 speaker playback 或 OpenJOC 负责 bass management。
- **Test level:** unit、manual evidence。
- **RED expectation:** 文档出现无证据的 automatic adaptation、physical-sub routing、room correction 或把 controlled sink 当 renderer support 的表述。
- **GREEN/pass evidence:** repository hygiene 验证 evidence links、三状态、no-Auto、logical/physical-sub separation 和 shipped-equals-proven；文档列出 exact masks、formats、environment 和 failure HRESULT。
- **Phase/task:** Phase 6 Task 1、Task 3–4。
- **关键禁止性断言:** 不得把 endpoint 名称、physical speaker 配置、测试 fixture 或 renderer 接收格式转换成真实扬声器播放声明。

## Cross-cutting gates

### GATE-01 — No name parsing

- Contract lookup 和 persistence 只接受固定 enum。
- UI 只能读取 `CB_GETITEMDATA`，不得解析 display text。
- filename、container、carrier count、renderer friendly name、endpoint/product name、consumer notation 均不得进入 policy selection。
- PotPlayer “Source (Input) as Output”必须由可见 UI 确认，不能由 registry index 推断。
- **Fail if:** 出现名称解析入口，或同一 policy 因名称/filename/count 改变 contract。
- **Pass evidence:** Phase 1 contract tests、Phase 4 property-page tests、Phase 5 raw/MP4 matrix、Phase 6 validator/hygiene。

### GATE-02 — Exact logical LFE count

- Stereo canonical mask 恰有零个 LFE bit；其余六行各恰有一个。
- evidence 对 Stereo 固定 `logical_lfe_channels: 0`、对六个 multichannel rows 固定为 `1`，不得含 `physical_subwoofer_count`。
- `.2` height suffix 必须解析为 TFL/TFR。
- **Fail if:** Stereo 有 LFE、任一 multichannel row 不是恰好一个 logical LFE，或物理 subwoofer 数影响 PCM contract。
- **Pass evidence:** Phase 1 unit tests和Phase 6 schema/documentation tests。

### GATE-03 — No fallback

- strict type 只能提出一次。
- rejection 后无第二 proposal、无 sample、无 connection mutation。
- 必须记录真实 failure stage/HRESULT。
- stock fallback 仅允许 stock buffers 使用。
- **Fail if:** retry int16、side/back variant、7.1、Stereo、current type 或其他布局。
- **Pass evidence:** Phase 3 fake-pin trap 和 Phase 5 post-bootstrap dynamic rejection e2e。

### GATE-04 — Pristine stock control

- pristine 必须来自冻结的 start-HEAD `b06ba2cbbd5c8806ca4423a8ff1527e4e2bd6a27` 或已验证等价 tree。
- target/pristine 使用分离 build/runtime 目录和 private activation。
- ordinary E-AC-3 与 passthrough 均比较完整 media type、bytes、sample/EOS 行为。
- **Fail if:** 使用修改后 build 加 feature flag 作为 control，或 target/pristine artifacts 相互覆盖。
- **Pass evidence:** Phase 5 Task 1、Task 3 的独立 module path/hash 和行为对照。

### GATE-05 — Runtime module path/hash

Native harness 必须验证恰好一个运行时实例及 manifest-matching path/hash：

- LAV Audio
- LAV Splitter
- `openjoc_capi.dll`
- 每个实际加载的 `*-lav-*.dll`
- `libbluray.dll`

PotPlayer 必须在 graph 创建后记录相同组件的 in-process path/hash。

- **Fail if:** basename 重复、路径错误、hash 错误、组件缺失，或只有 staged manifest/registered CLSID 而无运行时枚举。
- **Pass evidence:** native `PrivateComModule` 加 process enumeration，以及 PotPlayer in-process module inventory。

### GATE-06 — Mandatory STREAM_PROVEN set

Stereo、5.1、7.1 必须全部满足：

- 同一 named real renderer/endpoint；
- native raw+MP4 exact connect；
- requested/pre/post `ConnectionMediaType` 完全相同；
- Pause/Run、samples、bytes、EOS、无 fallback/error；
- PotPlayer Source-as-Output raw+MP4；
- same-instance policy=`requested`、admission=`OpenJoc`；
- actual float32/48 kHz/count/mask 精确；
- runtime module path/hash 完整。

任一 mandatory layout 非 `STREAM_PROVEN` 时，整体结果为 material blocker，不得声明完成。

### GATE-07 — Evidence-state semantics

| State | Required meaning | Required evidence | Forbidden use |
|---|---|---|---|
| `STREAM_PROVEN` | 指定 host/renderer/endpoint 下 exact streaming 已证明 | 完整 native + PotPlayer 成功链及模块身份 | 由 mask、probe 或 controlled sink 单独产生 |
| `UNSUPPORTED` | exact 测试发生测量到的拒绝或格式 mutation | failure stage、HRESULT、requested/actual types | 用于未运行或证据不完整的 row |
| `UNVERIFIED` | 未充分运行，无法判断支持或拒绝 | 明确 reason 和缺失阶段 | 加入 shipped list 或描述为支持 |

## Completion gate

只有在以下条件全部满足时，测试工作才可声明完成：

- 25 条 AC 全部具有可追溯 RED 和 GREEN 记录；
- 所有 unit/integration/e2e suites 通过；
- Stereo、5.1、7.1 均为真实 `STREAM_PROVEN`；
- shipped list 精确等于 `STREAM_PROVEN` set；
- stock target-vs-pristine controls 通过；
- 无 fallback、名称推断或 logical-LFE 违规；
- 所有 runtime module path/hash 验证通过；
- registry、COM registration、PotPlayer 设置全部恢复；
- controlled sink 与真实 renderer/PotPlayer 结论保持明确隔离；
- 独立 code review 和 completion verification 已完成。
