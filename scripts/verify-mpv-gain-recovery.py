#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 OpenJOC contributors
# SPDX-License-Identifier: Apache-2.0

"""Verify native gain replay with legitimate host format changes and menu intent."""
from __future__ import annotations
import argparse
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import wave

ROOT = Path(__file__).resolve().parents[1]
VOLUME = re.compile(r"volume@openjoc_gain:.*?n:([^ ]+).*?volume_dB:([-+0-9.]+)")


def verify_log(log: str, scenario: str) -> None:
    if 'GAIN_RECOVERY_STAGE DONE' not in log:
        raise ValueError(f'{scenario}: driver did not finish')
    phase = ''
    intent = 0.1
    rebuilt: list[tuple[str, float, list[tuple[str, float]]]] = []
    for line in log.splitlines():
        if 'GAIN_RECOVERY_STAGE ' in line:
            phase = line.split('GAIN_RECOVERY_STAGE ', 1)[1].split(' ', 1)[0]
            if phase == 'PREVIEW_02':
                intent = 0.2
            elif phase == 'PREVIEW_03':
                intent = 0.3
            elif phase == 'CANCEL_01':
                intent = 0.1
        match = VOLUME.search(line)
        if match and phase.startswith(('PRE_FORMAT_', 'PAUSED_PRE_FORMAT', 'RESUME', 'PRE_SEEK')):
            point, gain = match.group(1), float(match.group(2))
            if point == 'nan' and gain == 0.1:
                if rebuilt and rebuilt[-1][0] == phase and len(rebuilt[-1][2]) == 1:
                    rebuilt[-1][2].append((point, gain))
                else:
                    rebuilt.append((phase, intent, [(point, gain)]))
            elif rebuilt:
                rebuilt[-1][2].append((point, gain))
    if not rebuilt:
        raise ValueError(f'{scenario}: no owned graph recreation was observed')
    for phase, expected, values in rebuilt:
        if len(values) < 2 or values[1] != ('nan', expected):
            raise ValueError(f'{scenario}/{phase}: latest target was not replayed before first sample: {values}')
    phases = {phase for phase, _, _ in rebuilt}
    required = {'PRE_FORMAT_44100'}
    if scenario in ('saved', 'preview', 'new-preview', 'cancel', 'paused'):
        required.add('PRE_FORMAT_32000')
    if scenario == 'seek':
        required.add('PRE_SEEK')
    if scenario == 'paused' and not phases.intersection({'RESUME', 'PAUSED_PRE_FORMAT'}):
        raise ValueError('paused: no paused/resume context reconstruction was observed')
    if not required.issubset(phases):
        raise ValueError(f'{scenario}: required context boundaries are missing: {required - phases}')
    for stage in ({'cancel': 'CANCEL_01', 'new-preview': 'PREVIEW_03'}.get(scenario),):
        if stage and 'GAIN_RECOVERY_STAGE ' + stage not in log:
            raise ValueError(f'{scenario}: requested intent transition is missing')
    # No new decoder may be created during format-only changes or gain commands.
    section = log.split('GAIN_RECOVERY_STAGE PREVIEW_02', 1)[1]
    for boundary in ('PRE_SEEK', 'PRE_NEW_FILE', 'PRE_NON_OPENJOC_TRACK'):
        section = section.split('GAIN_RECOVERY_STAGE ' + boundary, 1)[0]
    if 'Selected decoder: libopenjoc ' in section:
        raise ValueError(f'{scenario}: context recovery reopened the decoder')
    if scenario == 'seek' and 'GAIN_RECOVERY_SEEK ' not in log:
        raise ValueError('actual seek evidence is missing')
    if scenario in ('reload', 'track'):
        boundary = 'PRE_NEW_FILE' if scenario == 'reload' else 'PRE_NON_OPENJOC_TRACK'
        section = log.split('GAIN_RECOVERY_STAGE ' + boundary, 1)[1]
        if scenario == 'track':
            if 'NON_OPENJOC_TRACK_NO_GAIN' not in section:
                raise ValueError('ordinary-track gain cleanup was not observed')
            # mpv recreates user filters on decoder handoff before the Lua
            # cleanup observation arrives. This existing constructor window
            # must not carry the old instance's +0.3 command. After cleanup,
            # only the new OpenJOC instance's saved +0.2 may be established.
            if any(float(match.group(2)) == 0.3 for match in VOLUME.finditer(section)):
                raise ValueError('old per-instance preview crossed the track boundary')
            section = section.split('GAIN_RECOVERY_STAGE NON_OPENJOC_TRACK_NO_GAIN', 1)[1]
        values = [float(match.group(2)) for match in VOLUME.finditer(section)]
        if not values or any(value != 0.2 for value in values):
            raise ValueError(f'{scenario}: old per-instance preview crossed a replacement boundary: {values}')


def run(mpv: Path, fixture: Path, output: Path) -> None:
    output.mkdir(parents=True, exist_ok=True)
    cases = ['saved', 'preview', 'cancel', 'new-preview', 'paused', 'seek', 'reload', 'track']
    ordinary_track = output / 'ordinary-control.wav'
    with wave.open(str(ordinary_track), 'wb') as wav:
        wav.setnchannels(2)
        wav.setsampwidth(2)
        wav.setframerate(48000)
        wav.writeframes(bytes(48000 * 2 * 2 * 20))
    for scenario in cases:
        config = output / scenario / 'config'
        (config / 'scripts').mkdir(parents=True, exist_ok=True)
        shutil.copyfile(ROOT / 'integrations/mpv/openjoc-settings.lua', config / 'scripts/openjoc-settings.lua')
        (config / 'openjoc-settings.json').write_text(json.dumps({'schema': 1, 'options': {
            'render_mode': 'speaker', 'speaker_layout': '5.1', 'output_gain_tenths_db': 1,
        }}))
        logfile = output / f'{scenario}.log'
        command = [
            str(mpv), str(fixture), '--no-video', '--ao=null', '--ao-null-untimed=no',
            '--volume=100', '--aid=1', '--gapless-audio=yes', f'--config-dir={config}',
            '--ad=libopenjoc', '--ad-lavc-o=render_mode=speaker,speaker_layout=5.1',
            f'--script={ROOT / "integrations/mpv/test-openjoc-gain-recovery-driver.lua"}',
            f'--script-opts=openjoc_gain_recovery-recovery_case={scenario}',
            '--msg-level=all=debug', f'--log-file={logfile}',
        ]
        if scenario == 'track':
            command.append(f'--audio-file={ordinary_track}')
        try:
            result = subprocess.run(command, capture_output=True, text=True, timeout=18)
        except subprocess.TimeoutExpired:
            if logfile.is_file():
                print(logfile.read_text(errors='replace'), file=sys.stderr)
            raise
        (output / f'{scenario}-console.log').write_text(result.stdout + result.stderr)
        if result.returncode:
            print(logfile.read_text(errors='replace'), file=sys.stderr)
            raise RuntimeError(f'{scenario}: mpv exited {result.returncode}; see {logfile}')
        try:
            verify_log(logfile.read_text(errors='replace'), scenario)
        except ValueError:
            print(logfile.read_text(errors='replace'), file=sys.stderr)
            raise
        saved = json.loads((config / 'openjoc-settings.json').read_text())['options']['output_gain_tenths_db']
        if saved != (2 if scenario in ('saved', 'reload', 'track') else 1):
            raise ValueError(f'{scenario}: Save/preview persistence changed unexpectedly')
        print(f'GAIN_CONTEXT_RECOVERY_CASE:PASS {scenario}', flush=True)
    print('GAIN_CONTEXT_RECOVERY:PASS before_first_sample latest_intent fixed_ao paused repeated actual_seek', flush=True)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mpv', type=Path)
    parser.add_argument('fixture', type=Path, help='seekable long synthetic JOC MP4 fixture')
    parser.add_argument('--output-dir', type=Path)
    args = parser.parse_args()
    if args.output_dir:
        run(args.mpv.resolve(), args.fixture.resolve(), args.output_dir.resolve())
    else:
        with tempfile.TemporaryDirectory(prefix='openjoc-mpv-gain-recovery-') as directory:
            run(args.mpv.resolve(), args.fixture.resolve(), Path(directory))


if __name__ == '__main__':
    main()
