# Generated coupling regression fixtures

These four one-frame E-AC-3 streams contain generated silence, no third-party
recording or proprietary decoder output. They were generated with the public
FFmpeg 7.1.5 (`7.1.5-0+deb13u1`) encoder:

```sh
ffmpeg -v error -f lavfi -i 'anullsrc=r=48000:cl=4.0' -t 0.032 -c:a eac3 -b:a 384k -y acmod5.eac3
ffmpeg -v error -f lavfi -i 'anullsrc=r=48000:cl=quad' -t 0.032 -c:a eac3 -b:a 384k -y acmod6.eac3
ffmpeg -v error -f lavfi -i 'anullsrc=r=48000:cl=4.0' -t 0.032 -c:a eac3 -b:a 384k -channel_coupling 0 -y acmod5-nocpl.eac3
ffmpeg -v error -f lavfi -i 'anullsrc=r=48000:cl=quad' -t 0.032 -c:a eac3 -b:a 384k -channel_coupling 0 -y acmod6-nocpl.eac3
```

Each stream is 1536 bytes, six blocks, 48 kHz and four full-bandwidth channels.
FFmpeg can decode each with `ffmpeg -v error -i FILE -f null -`. Tests consume
the checked-in bytes and do not require FFmpeg at runtime. The coupled streams
use `cplbegf=11`, `cplendf=12`, and the default coupling structure. The uncoupled
controls isolate channel-inventory label tests from coupling parsing.

SHA-256:

```text
1f6591e387e7f16f9aa24658669df4dad663c6a25259b17be08d526882687126  acmod5.eac3
a090368d1429a429e271bb6ff0df2eea435afce823ab3b8e2fee2a32bb82ce24  acmod6.eac3
4cdce65f0f502957430b683eb12c2b0328fc011f301663031e2e5330cdb7a1d5  acmod5-nocpl.eac3
4403ad561a1d75d6d506659f63df1033c5e487ea7eb4a82e474fc982aa617ef6  acmod6-nocpl.eac3
```
