# Research history

This directory is the historical record of OpenJOC's evidence-driven
development. It preserves dated experiments, negative results, implementation
milestones and changes in the admissible evidence boundary.

Historical statements such as “unresolved at J1R10” are not today's capability
status. For current truth, use:

- [Capabilities](../CAPABILITIES.md)
- [Known limitations](../KNOWN_LIMITATIONS.md)
- [Roadmap](../ROADMAP.md)

Files:

- [Research history](RESEARCH_HISTORY.md) — research notes and experiment chronology.
- [Implementation history](IMPLEMENTATION_HISTORY.md) — dated implementation and
  verification record.
- [Ordered adjacent-output AVX FIR](ordered-fir-avx/README.md) — strict-bit-exact
  static-FIR acceleration, paired wall-time evidence and control regressions.
- [Bit-exact performance gate](bitexact-performance/README.md) — paired baseline,
  synthetic long-corpus, timing, allocation, and coverage notes.
- [Performance summary](bitexact-performance/performance-summary.csv) — compact
  first-milestone timing and hotspot observations.
- [Candidate A timing](bitexact-performance/candidate-a-timing60s.csv),
  [allocation](bitexact-performance/candidate-a-allocations.csv),
  [exact-gate](bitexact-performance/candidate-a-exact-gates.csv), and
  [provenance](bitexact-performance/candidate-a-provenance.json) — owned-frame
  experiment; allocation reduction observed, CPU speedup not demonstrated.
- [Candidate D timing](bitexact-performance/candidate-d-timings.csv),
  [allocation](bitexact-performance/candidate-d-allocations.csv),
  [exact-gate](bitexact-performance/candidate-d-exact-gates.csv),
  [unsupported sizes](bitexact-performance/candidate-d-unsupported-partitions.csv),
  and [provenance](bitexact-performance/candidate-d-provenance.json) — optional
  FFT scratch experiment; bit-exact in tested cases, CPU gain not demonstrated.
- [ADM BWF interoperability oracle report](ADM_BWF_INTEROPERABILITY_ORACLE_0.9.2.md)
  — read-only Logic structural comparison, RIFF/RF64 contract, and external
  acceptance gates for the 0.9.2 candidate.
- [Historical requirements matrix](../archive/requirements/REQUIREMENTS_MATRIX.md)
  — archived evidence consolidation, not a current status owner.

## Measured QMF phase-row optimization

[QMF phase-row CPU investigation](qmf-phase-row/README.md) records released-route mapping, baseline noise, function attribution, exactness and paired timing evidence. [Reproduction](qmf-phase-row/REPRODUCE.md) uses the existing strict gate.
