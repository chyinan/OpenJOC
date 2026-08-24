# OpenJOC-LAV Windows endpoint QA package

This package runs a private, self-contained DirectShow probe. It does not
register filters globally, change the default audio device, change endpoint
formats, infer layouts from product names, or try fallback media types.

First collect the exact renderer moniker:

```powershell
.\Run-OpenJocEndpointQa.cmd -InventoryOnly -OutputDirectory .\inventory
```

Read `inventory\renderer-inventory.tsv`, then supply the exact renderer moniker
and the corresponding Windows endpoint ID. Example syntax (the values are
machine-specific and intentionally are not inferred by this package):

```powershell
.\Run-OpenJocEndpointQa.cmd `
  -RendererFamily DirectSound `
  -EndpointKind PhysicalEndpoint `
  -RendererMoniker '<exact moniker from inventory>' `
  -EndpointId '<exact endpoint ID>' `
  -OutputDirectory .\endpoint-report
```

The output contains `report.json`, the renderer inventory, read-only endpoint
capability observations, and 14 raw TSV attempts (seven layouts, raw JOC and
MP4 JOC). Every attempt uses one exact WAVEFORMATEXTENSIBLE IEEE-float proposal
with no fallback.

Interpretation boundaries:

- A third-party virtual driver with delivered samples may support
  `VIRTUAL_WINDOWS_ENDPOINT_VERIFIED`.
- A physical endpoint with delivered samples may support
  `REAL_ENDPOINT_VERIFIED` for Windows/DirectShow transport into that endpoint.
- Neither result is `PHYSICAL_MULTICHANNEL_HARDWARE_VERIFIED` unless a human
  separately verifies the real multichannel speakers/AVR and routing.
- A marketing claim such as “16 channel” does not prove acceptance of height
  masks; the seven exact attempts remain separate.
