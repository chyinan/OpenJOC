# R00 Logic 人声定位复核包

这是 ADM profile timing 修复后的新 R00 候选。它不是原始 authored ADM
master，也不恢复原始 Object identity。

## Candidate

- Source: `C:\OpenJOC-RealMedia-Acceptance\成都 ~Dolby ATMOS Test2~ .m4a`
- Source SHA-256: `83bbaabd5705bd4a458dc8ebc7b8bef66ebf5d76d2f8d09b1cdaa225105e2156`
- Candidate A: `C:\Users\chyin\AppData\Local\Temp\openjoc-r00-adm-fixed-20260826\R00-fixed-candidate.wav`
- Candidate A SHA-256: `5AC8AA2A75F60D020889732AF800992A1323DE2CC967A9C086BB8AD6BB8DCEBF`
- Report: `C:\Users\chyin\AppData\Local\Temp\openjoc-r00-adm-fixed-20260826\R00-fixed-candidate.adm-report.json`
- Candidate B SHA-256: `5AC8AA2A75F60D020889732AF800992A1323DE2CC967A9C086BB8AD6BB8DCEBF`
- Candidate B `data` chunk SHA-256: `C749E57CA0B287C423FE3FC676D84F76EDC0EA337233A5F4E5F5D3EF12313857`

The candidate was exported with strict policy and passed `validate-adm`:
RIFF/BWF, 21 tracks, 21 CHNA UIDs, 327.968 s, 15 bound dynamic Objects,
24-bit PCM, and zero non-finite or out-of-range decoder-domain samples. The
XML contains 15 first Object blocks with `interpolationLength=0` and 153,720
subsequent Object blocks with `interpolationLength=250`.

## Transfer and Logic checklist

Copy the candidate WAV and its report to the Mac without transcoding, then
import the WAV in Logic. Compare against the previous coordinate-fixed
candidate at matched sample rate, start, level, and monitoring layout.

- [ ] File imports normally as ADM/BWF and retains 21 channels.
- [ ] Global spatial topology remains PASS.
- [ ] Dynamic objects still move through the programme.
- [ ] Lead vocal is no longer materially fixed left; note the exact cue/time.
- [ ] Head-tracking rotation does not reveal a static left anchor.
- [ ] Height and rear/side movement remain present.
- [ ] Base/LFE behavior remains separate and unchanged.
- [ ] No unrelated level, clipping, silence, or channel-order regression.

The code change is limited to Dolby ADM `jumpPosition` metadata. It does not
apply OAMD gain to PCM, alter the accepted coordinate bridge, change row
binding, decode raw3/warp-3 semantics, or recover authored identity. A failed
vocal check therefore leaves the profile-state defect fixed but does not
support a gain/active explanation for the vocal symptom.

## Result fields

- Human Logic global topology: `PENDING`
- Human Logic vocal localization: `PENDING`
- Causal conclusion for the former vocal-left observation: `PENDING`
