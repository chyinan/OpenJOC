# GCC runtime corresponding source

Release: openjoc-0.16.0
Evidence date: 2026-08-22

## Result

The distributed `libgcc_s_seh-1.dll` is byte-for-byte identical to the DLL in the official MSYS2 binary package `mingw-w64-x86_64-gcc-libs` version `16.2.0-3`. The exact official MSYS2 source-only package for that binary package has been retained for the corresponding-source candidate. No newer GCC release was substituted.

## Verified artifacts

| Item | SHA-256 |
|---|---|
| `mingw-w64-gcc-16.2.0-3.src.tar.zst` | `EB3479A8B0B23810FBBBC25EF76879E867E88D09960A40145D73F5505FDA4DA0` |
| `mingw-w64-x86_64-gcc-libs-16.2.0-3-any.pkg.tar.zst` | `F8E25EA67BB796E7F65550F0DCA9FCE4CDDE8AAA3DADAFE4D13C6A8233C8DE26` |
| package `mingw64/bin/libgcc_s_seh-1.dll` | `B37C1770C8CA092700875845B34918803EE6311573EBA1C32FF4B1166E4A0E1E` |
| release `runtime/libgcc_s_seh-1.dll` | `B37C1770C8CA092700875845B34918803EE6311573EBA1C32FF4B1166E4A0E1E` |

Official immutable filenames:

- [MSYS2 source-only package](https://repo.msys2.org/mingw/sources/mingw-w64-gcc-16.2.0-3.src.tar.zst)
- [MSYS2 binary package](https://repo.msys2.org/mingw/mingw64/mingw-w64-x86_64-gcc-libs-16.2.0-3-any.pkg.tar.zst)

## Package provenance

The binary package `.PKGINFO` records:

- `pkgname = mingw-w64-x86_64-gcc-libs`
- `pkgbase = mingw-w64-gcc`
- `pkgver = 16.2.0-3`
- architecture `any`
- build date `2026-08-09T05:39:27Z`
- MSYS2 CI build `31295760804`, job `93200480425`
- license expression `GPL-3.0-or-later WITH GCC-exception-3.1 AND LGPL-2.1-or-later`

The source-only archive `.SRCINFO` records `pkgver = 16.2.0`, `pkgrel = 3`, the upstream GNU source URL `https://ftp.gnu.org/gnu/gcc/gcc-16.2.0/gcc-16.2.0.tar.xz`, and upstream source SHA-256 `E6738E29597F733270731AA90600F37FFDC045079DFC27EC7E8192CC81085C3E`. The archive contains that exact GCC source tarball, its signature, the MSYS2 `PKGBUILD`, `.SRCINFO`, and every MSYS2 patch used for the build.

## License evidence

The binary package includes these exact license files under `mingw64/share/licenses/gcc-libs/`:

- `COPYING3`
- `COPYING.LIB`
- `COPYING.RUNTIME`
- `README`

`COPYING.RUNTIME` contains the GCC Runtime Library Exception 3.1. The package README identifies libgcc, libstdc++, libgomp, and libatomic as GPL-3.0-or-later with that exception; libquadmath is LGPL-2.1-or-later. These texts are retained in both candidate license sets.

The exception is not being treated as a reason to omit source. The entire exact official source-only package is included at `third_party_sources/msys2/` in the corresponding-source archive.

## Verification method

The package filename and SHA-256 were verified before extraction. `.SRCINFO`, `.PKGINFO`, and `.BUILDINFO` were read from the downloaded archives. The package DLL and release DLL were separately SHA-256 hashed and yielded the same digest shown above. Automated assertions are in `scripts/tests/test_gcc_runtime_source.py`.
