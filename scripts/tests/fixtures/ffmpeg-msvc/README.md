# FFmpeg MSVC dependency excerpts

These are source excerpts for packaging regression tests, not executable or
complete copies of `configure`.

- `legacy.configure` contains the unmodified MSVC detection branch from
  [FFmpeg n8.0 configure](https://github.com/FFmpeg/FFmpeg/blob/n8.0/configure).
  It exercises the legacy awk backslash expression used by the release
  workaround. It is not a complete snapshot of LAV's older FFmpeg pin.
- `native.configure` contains verbatim lines captured from LAV FFmpeg
  `b033272d7ef2070db2dc8ef228e67b20213e90c8`, with line-number comments marking
  omitted source. The capture came from the native Windows preparation
  [diagnostic run](https://github.com/chyinan/OpenJOC/actions/runs/38059089003).
  The subsequent [successful candidate run](https://github.com/chyinan/OpenJOC/actions/runs/38059345505)
  retained an empty FFmpeg patch and generated `CC_DEPFLAGS=-showIncludes`.

The original configure script credits Fabrice Bellard (2000–2002), Diego
Biurrun (2005–2008), and Mans Rullgard (2005–2008). See the upstream
[FFmpeg license](https://github.com/FFmpeg/FFmpeg/blob/n8.0/LICENSE.md).

Tests derive the already-patched legacy form by changing only the backslash
expression. They also exercise LF/CRLF, repeat invocation, altered or missing
dependency markers, and unrelated/ambiguous text. Passing these tests does
not establish that the formal release workflow has run on Windows.
