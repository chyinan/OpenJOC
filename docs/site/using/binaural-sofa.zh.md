!!! note "翻译说明"
    中文文档是持续维护中的翻译版本，可能会略滞后于英文文档；如有技术差异，以英文版为准。

# 双耳与 SOFA

`--binaural` 会把 OpenJOC 的扬声器场渲染为双声道耳机输出。它采用虚拟扬声器渲染，不代表直达对象渲染或专有渲染器的听感保真度。

OpenJOC 提供两套通用内置配置：`SADIE II — KU100`（默认/参考配置）和
`SADIE II — KEMAR`，同时继续支持 Custom SOFA。不同听音者可能偏好不同的非个体 HRTF，因为 HRTF 感知强烈依赖
个体解剖结构；不应把任何一套宣传为适合所有人的最佳配置。

来源、许可证、哈希和可复现的预处理记录见[内置 HRTF 记录](https://github.com/chyinan/OpenJOC/blob/master/docs/hrtf.md)。

```sh
openjoc render-joc input.m4a \\
  --binaural \\
  --output headphones.wav
```

默认虚拟布局是 7.1.4。未指定 HRTF 时，CLI 使用内置的离线 SADIE II D1 / KU100。要显式选择内置配置，请使用 `--binaural-hrtf`：

## 选择内置 HRTF

`--binaural-hrtf` 后面填写稳定 ID，而不是界面显示名称。省略该选项时使用默认的 SADIE II D1 / KU100：

```sh
openjoc render-joc input.m4a --binaural --binaural-hrtf sadie-ii-d1-ku100 --output headphones-ku100.wav
openjoc render-joc input.m4a --binaural --binaural-hrtf sadie-ii-d2-kemar --output headphones-kemar.wav
```

如果希望使用自己的 SOFA 文件，请改用 `--binaural-sofa listener.sofa`（或 `--sofa listener.sofa`）；提供 SOFA 时使用该文件，而不是内置 preset：

```sh
openjoc render-joc input.m4a \\
  --binaural \\
  --binaural-sofa listener.sofa \\
  --backend direct \\
  --output custom-headphones.wav
```

## SOFA 支持范围 {#sofa-scope}

加载器支持 NetCDF classic CDF-1 和 NetCDF-4/HDF5 容器中的 `SimpleFreeFieldHRIR`。它继续校验受支持的字段、两个接收器、坐标和方向覆盖。渲染前会使用有界的确定性窗函数 sinc 滤波器将 HRIR 转成 48 kHz。输入本身为 48 kHz 时，系数保持逐位不变。仍不接受分数采样延迟；转换后的整数延迟按最近的 48 kHz 采样点取整。

转换按固定采样率比例补偿 FIR 卷积增益，分数时序保留在系数中。
为保留完整的 sinc 滤波前沿，左右耳会增加相同的公共延迟。
延迟为 `ceil(16 * max(输出采样率 / 源采样率, 1)) + 1` 个输出采样点。
例如，44.1→48 kHz 增加 19 个采样点，96→48 kHz 增加 17 个。
这个延迟保留在 HRIR 和完整尾部中，也计入渲染器的延迟报告。
SOFA 原有的实测延迟仍不计入报告。
源、目标采样率相同时不增加延迟。转换比例超过 16:1 时拒绝处理。
使用 `equal-power-dual-mono` 时，LFE 也会延迟相同的时间并完整排空尾部，保持与空间声道的相对时序。

加载器接受标准的固定 `ReceiverPosition [R,C,1]` 和 `Data.Delay [1,R]` 布局。
接收器位置使用听者局部坐标；平移或旋转听者不会改变左右耳的对应关系。
`ListenerUp` 未提供自身的坐标类型或单位时，会继承 `ListenerView` 的相应属性。
`Data.Delay` 即使省略 `Units` 属性，也始终以采样点为单位。
显式指定不兼容的坐标或延迟单位会被拒绝。
听者位置和朝向向量必须使用以米为单位的笛卡尔坐标，目前不支持球面听者元数据。

单个 HDF5 chunk 解压后最多为 16 MiB，还受配置中的文件字节数和系数预算约束。
超限 chunk 会在解压前被拒绝。

使用前先检查文件：

```sh
openjoc sofa inspect listener.sofa --json
```

`sofa inspect` 显示文件的源采样率；双耳渲染前会先把 HRIR 转成 48 kHz，再交给所选后端。`direct` 是数值参考后端。`partitioned` 使用一个固定的二次幂分区大小，并保留完整输入和 FIR 尾部。如果方向覆盖不足，两种后端都会拒绝继续处理。

## LFE 策略

CLI 默认使用 `exclude`。如果明确希望把逻辑 LFE 声部发送到左右耳，可以使用 `equal-power-dual-mono`：

```sh
openjoc render-joc input.m4a \\
  --binaural \\
  --lfe-policy equal-power-dual-mono \\
  --output headphones-with-lfe.wav
```

这是渲染器策略，不会推断物理低音炮，也不会修改源场景。
