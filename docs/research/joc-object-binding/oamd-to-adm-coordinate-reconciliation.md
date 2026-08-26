# OAMD → ADM Cartesian Coordinate Reconciliation

状态：审计证据、实现闸门与修复后回归记录
日期：2026-08-26
代码基线：`codex/joc-decoded-object-binding` @ `cd1fd44f6bf4e5f7417449723e19981dbf26a21a`

## 范围与清洁来源

本审计只使用公开标准、仓库中的干净生产代码、已脱敏的公开解码场景向量，以及既有 R00 技术验收产物。未读取或使用 DRP、Ghidra、污染目录、私有媒体内容、Raw3/JOC binding/PCM/gain/LAV 等非本任务范围材料。场景向量文件的 SHA-256 为
`5c5763a2587f07304ea762b49f267901b94eace6638cbf905450282cc99a14f3`。

## 标准域

### OAMD / ETSI TS 103 420

[ETSI TS 103 420 V1.2.1](https://www.etsi.org/deliver/etsi_ts/103400_103499/103420/01.02.01_60/ts_103420v010201p.pdf) clause 4.2.1 定义 room-anchored 坐标：左手笛卡尔坐标，归一化到房间 cuboid；X 从左墙 `0` 增至右墙 `1`，Y 从前墙 `0` 增至后墙 `1`，Z 从地面 `-1` 增至天花板 `1`。因此公开解码场景中常见的 OAMD in-room 域是：

```text
OAMD X ∈ [0, 1]
OAMD Y ∈ [0, 1]
OAMD Z ∈ [-1, 1]
```

同一标准的 clause 5.6.1.1.8–5.6.1.1.11 给出绝对位置的主精度/扩展精度解码公式。仓库 `crates/openjoc-oamd/src/position.rs` 已按该域解码；本审计不改变 OAMD 解码器。

标准还允许对象位置因距离投影落在房间外。本次 ADM 导出桥接只承认可无损表达为归一化 ADM Cartesian cube 的 in-room OAMD 子域；对超出该导出 profile 的坐标必须报错，不得静默 clamp。

### ADM / ITU-R BS.2076-3

[ITU-R BS.2076-3](https://www.itu.int/dms_pubrec/itu-r/rec/bs/R-REC-BS.2076-3-202502-I%21%21PDF-E.pdf) clause 8 规定 `audioBlockFormat` 的 Cartesian X/Y/Z：原点在立方体中心，归一化表面为 `±1`；X 正方向向右，Y 正方向向前，Z 正方向向上。

ADM 导出目标域因此为：

```text
ADM X ∈ [-1, 1]
ADM Y ∈ [-1, 1]
ADM Z ∈ [-1, 1]
```

## 唯一候选映射

OAMD 与 ADM 的原点/轴方向不同，唯一符合上述端点语义的逐分量映射是：

```text
ADM X =  2 × OAMD X - 1
ADM Y =  1 - 2 × OAMD Y
ADM Z =      OAMD Z
```

逆映射为：

```text
OAMD X = (ADM X + 1) / 2
OAMD Y = (1 - ADM Y) / 2
OAMD Z = ADM Z
```

这只是 OAMD → ADM 边界转换；不改变 scene model、JOC object ordinal binding、PCM/channel layout 或任何 Raw3/增益/LAV 逻辑。

## 控制向量真值表

“当前导出”栏由 clean production path 的 `convert_coordinate` identity + `position_for_adm` identity 推导，并由现有 R00 的独立 XML 统计交叉验证。控制向量来自脱敏公开解码场景向量；它们不是私有媒体快照。

| 控制向量/行 | OAMD 输入 | 预期 ADM | 当前导出 ADM | 预期 Logic 语义 | 当前偏差 |
|---|---|---|---|---|---|
| C00 row 0 | `(0.5, 0, 0)` | `(0, 1, 0)` | `(0.5, 0, 0)` | 中心/前方 | X 偏右、Y 未转为前方 |
| C06-static-frontleft row 0 | `(0, 0, 0)` | `(-1, 1, 0)` | `(0, 0, 0)` | 左前 | 左/前均丢失 |
| C06-static-center row 3 | `(1, 0, 0)` | `(1, 1, 0)` | `(1, 0, 0)` | 右前 | Y 未转为前方 |
| C04 row 0 | `(0.5, 1, 0)` | `(0, -1, 0)` | `(0.5, 1, 0)` | 中心/后方 | X 偏右；Y 仍被当作前方 |
| C05 row 0 | `(0.5, 0, 0)` → `(0.5, 0, 0.4)` → `(0.5, 0, 1)` | `(0, 1, 0)` → `(0, 1, 0.4)` → `(0, 1, 1)` | `(0.5, 0, 0)` → `(0.5, 0, 0.4)` → `(0.5, 0, 1)` | 中心前方，高度 `0 → .4 → 1` | 水平位置偏右/中深度，高度本身保持 |

### 现有 R00 XML 交叉验证

既有 R00 candidate 的 XML 统计（15 个 object channels，153,735 个 object blocks）为：

```text
x_min=0, x_max=1
y_min=0, y_max=1
z_min=0, z_max=1
x_left=0, x_zero=60,637, x_right=93,098
y_front=106,170, y_zero=47,565, y_back=0
```

这与“X 没有从 `[0,1]` 转为 `[-1,1]`、Y 没有反向并中心化”的候选缺陷一致。该证据支持“坐标域错误是 Logic 空间异常的充分技术原因之一”，但不把主观听感当作数学证明；人耳/Logic 导入复核仍是最终门。

## 条件修复闸门

1. 标准域已由公开 ETSI/ITU 文本固定：PASS。
2. OAMD 解码域与 ADM 写出域已在仓库路径中定位：PASS。
3. 当前代码与 R00 XML 均证明边界为 identity：PASS。
4. 不改变 binding、scene、PCM、Raw3 或增益路径即可在 `openjoc-adm` 边界修复：PASS。
5. 控制向量按唯一映射可同时修复中心、左/右、前/后、Z 高度语义：PASS（实现前数学验算）。

因此允许进入“先红测试、后最小实现”的条件修复阶段。实现必须使用带语义名称的 OAMD/ADM 类型、显式范围验证，并对非有限值或目标 profile 外坐标 fail closed；禁止 silent clamp。

## 实现与回归证据

已在 `crates/openjoc-adm/src/coordinate.rs` 实现单一 typed bridge，并在
`position_for_adm` 处调用一次。`AdmDynamicBlock` 现在保存已验证的
`AdmCartesianPosition`；XML writer 不再包含散落的坐标公式。C00 红测试先在
identity 实现上失败（实际 `(0.5,0,0)`，期望 `(0,1,0)`），随后坐标单元测试、ADM
绑定集成测试均通过。

脱敏公开场景向量桥接/绑定回归和 C06 observed-vendor-compat decoded-object
回归均通过；未读取或使用含 `drp-capture` 的派生分析目录。

R00 使用正常 strict production path 重新导出到新目录，两次结果均通过：

```text
R00_FIXED_FULL_ADM_EXPORT = PASS
R00_FIXED_ADM_VALIDATION = PASS
R00_FIXED_DETERMINISM = PASS
run-a/run-b ADM SHA-256 = 85592FAFF43A2CF00A7DB2539413298744AE26B69CE041A98ABEC8DB9B37D4CF
```

独立 Object XML 统计（153,735 blocks）如下：

| 统计 | 旧 R00 | 修正 R00 |
|---|---:|---:|
| X min / max | `0 / 1` | `-1 / 1` |
| X < 0 | `0` | `71,881` |
| X ≈ 0 | `60,637` | `9,669` |
| X > 0 | `93,098` | `72,185` |
| Y > 0（前方） | `106,170` | `89,638` |
| Y ≈ 0 | `47,565` | `22,804` |
| Y < 0（后方） | `0` | `41,293` |
| Z min / max | `0 / 1` | `0 / 1` |

C00 中心前方、FrontLeft、FrontRight、C04 后方和 C05 高度轨迹均由桥接单元测试/受控 XML 端点覆盖。修正 R00 的 `data` chunk SHA-256 与旧 R00 完全相同：
`C749E57CA0B287C423FE3FC676D84F76EDC0EA337233A5F4E5F5D3EF12313857D`。
独立 signed-24 readback 也保持 `21 channels × 15,742,464 frames`，旧/新最小整数值
`-6,815,384`、最大整数值 `6,121,606` 一致。

这证明了 metadata 坐标域修复没有改动解码 PCM、对象 ordinal binding、scene model
或 Raw3 处理。Logic 的主观空间语义仍须由维护者导入修正版后最终确认。
