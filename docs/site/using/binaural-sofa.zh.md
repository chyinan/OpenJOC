!!! note "翻译说明"
    中文文档是持续维护中的翻译版本，可能会略滞后于英文文档；如有技术差异，以英文版为准。

# 双耳与 SOFA

`--binaural` 会把 OpenJOC 的扬声器场渲染为双声道耳机输出。它采用虚拟扬声器渲染，不代表直达对象渲染或专有渲染器的听感保真度。

```sh
openjoc render-joc input.m4a \\
  --binaural \\
  --output headphones.wav
```

默认虚拟布局是 7.1.4。如果没有提供 SOFA 路径，会使用内置的离线 SADIE II D1 HRTF。指定自定义数据集时，会走受支持的本地 SOFA 流程：

```sh
openjoc render-joc input.m4a \\
  --binaural \\
  --binaural-sofa listener.sofa \\
  --backend direct \\
  --output custom-headphones.wav
```

## SOFA 支持范围

加载器接受文档规定的 `SimpleFreeFieldHRIR` NetCDF classic CDF-1 子集。文件必须提供两个接收器、匹配的采样率，并为每个请求的非 LFE 虚拟方向提供精确覆盖或可安全插值的覆盖。HDF5/NetCDF-4、重采样、下载、写入、移动声源和任意方向覆盖均不受支持。

使用前先检查文件：

```sh
openjoc sofa inspect listener.sofa --json
```

`direct` 是数值参考后端。`partitioned` 使用一个固定的二次幂分区大小，并保留完整输入和 FIR 尾部。如果覆盖范围或采样率不匹配，两种后端都会拒绝继续处理。

## LFE 策略

CLI 默认使用 `exclude`。如果明确希望把逻辑 LFE 声部发送到左右耳，可以使用 `equal-power-dual-mono`：

```sh
openjoc render-joc input.m4a \\
  --binaural \\
  --lfe-policy equal-power-dual-mono \\
  --output headphones-with-lfe.wav
```

这是渲染器策略，不会推断物理低音炮，也不会修改源场景。
