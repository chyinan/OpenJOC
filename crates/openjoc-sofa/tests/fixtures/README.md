# CDF-1 interoperability fixture

`cdf1-standard-absent.sofa` is project-owned synthetic data, written with
SciPy 1.17.0's `scipy.io.netcdf_file(..., "w", version=1)`, independently of the
OpenJOC serializer. It contains no measured, private, or vendor HRIR data.

- Length: 1,436 bytes.
- SHA-256: `a63576093a75606d491ac9f109c8e38ab4e0b9837ba07a0d6c923a0c7089ae85`.
- Dimensions: M=1, R=2, N=4, C=3, I=1; all variables are big-endian f64.
- `Data.IR` and `ListenerUp` have no attributes. Each uses the standard
  eight-zero-byte CDF-1 ABSENT header.
- The two receiver rows are right then left, with delays `[0, 1]`.
- IR rows are `[1, -0, smallest positive f64 subnormal, -2]` and
  `[3, 4, -0, 0.5]`. The Rust test checks complete expanded tap bits, including
  signed zero, the subnormal and delay padding.

The existing in-memory Rust fixture builder intentionally retains the old
single-zero-word attribute encoding to cover historical project intermediate
compatibility. This new fixture does not replace any frozen PCM expectation or
built-in HRTF asset. SciPy is not required to run the tests.
