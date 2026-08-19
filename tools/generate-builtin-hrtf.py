#!/usr/bin/env python3
"""Convert an authorized SADIE II HDF5 SOFA source to OpenJOC's CDF-1 subset.

This is an offline packaging tool. It does not run during rendering and does
not fetch network resources. The source file must be the official SADIE II D1
48 kHz / 256-tap KU100 HRIR file recorded in the provenance document.
"""

from __future__ import annotations

import argparse
import struct
from pathlib import Path

import h5py
import numpy as np


NC_CHAR = 2
NC_FLOAT = 5
NC_DOUBLE = 6


def pad4(data: bytes) -> bytes:
    return data + b"\0" * ((-len(data)) % 4)


def nc_string(value: str) -> bytes:
    raw = value.encode("utf-8")
    return struct.pack(">I", len(raw)) + pad4(raw)


def nc_attr(name: str, value: str) -> bytes:
    raw = value.encode("utf-8")
    return nc_string(name) + struct.pack(">II", NC_CHAR, len(raw)) + pad4(raw)


def nc_dims(names: list[tuple[str, int]]) -> bytes:
    result = struct.pack(">II", 10, len(names))
    for name, length in names:
        result += nc_string(name) + struct.pack(">I", length)
    return result


def nc_var(name: str, dims: list[int], attributes: list[tuple[str, str]], ty: int, vsize: int, begin: int) -> bytes:
    result = nc_string(name) + struct.pack(">I", len(dims))
    result += b"".join(struct.pack(">I", dim) for dim in dims)
    if attributes:
        result += struct.pack(">II", 12, len(attributes))
        result += b"".join(nc_attr(attr_name, value) for attr_name, value in attributes)
    else:
        result += struct.pack(">I", 0)
    return result + struct.pack(">III", ty, vsize, begin)


def float32_bytes(values: np.ndarray) -> bytes:
    return np.asarray(values, dtype=">f4", order="C").tobytes(order="C")


def builtin_virtual_directions() -> np.ndarray:
    """Canonical OpenJOC virtual-speaker directions in listener coordinates."""
    directions = [
        (-1.0, 1.0, 0.0),
        (1.0, 1.0, 0.0),
        (0.0, 1.0, 0.0),
        (-1.0, 0.0, 0.0),
        (1.0, 0.0, 0.0),
        (-1.0, -1.0, 0.0),
        (1.0, -1.0, 0.0),
        (-1.0, 1.0, 1.0),
        (1.0, 1.0, 1.0),
        (-1.0, 0.0, 1.0),
        (1.0, 0.0, 1.0),
        (-1.0, -1.0, 1.0),
        (1.0, -1.0, 1.0),
        (-1.0, 0.67767333984375, 0.0),
        (1.0, 0.67767333984375, 0.0),
    ]
    for azimuth_degrees, elevation_degrees in [
        (26.25, 0.0),
        (52.5, 0.0),
        (90.0, 0.0),
        (122.5, 0.0),
        (180.0, 0.0),
        (52.5, -22.5),
        (0.0, -22.5),
        (52.5, 37.5),
        (0.0, 37.5),
        (90.0, 37.5),
        (122.5, 37.5),
        (180.0, 37.5),
        (0.0, 90.0),
    ]:
        azimuth = np.deg2rad(azimuth_degrees)
        elevation = np.deg2rad(elevation_degrees)
        directions.append(
            (
                -np.sin(azimuth) * np.cos(elevation),
                np.cos(azimuth) * np.cos(elevation),
                np.sin(elevation),
            )
        )
    result = np.asarray(directions, dtype=np.float64)
    result /= np.linalg.norm(result, axis=1, keepdims=True)
    return result


def append_virtual_aliases(
    source_position: np.ndarray, ir: np.ndarray, delay: np.ndarray
) -> tuple[np.ndarray, np.ndarray]:
    """Add exact canonical positions using the nearest measured HRIR.

    The aliases are only added for the finite OpenJOC virtual-speaker set. The
    nearest angular error is checked and remains below 3 degrees for the D1
    source grid; no runtime nearest-neighbor fallback is introduced.
    """
    azimuth = np.deg2rad(source_position[:, 0])
    elevation = np.deg2rad(source_position[:, 1])
    world = np.c_[
        np.cos(elevation) * np.cos(azimuth),
        np.cos(elevation) * np.sin(azimuth),
        np.sin(elevation),
    ]
    measured = np.c_[-world[:, 1], world[:, 0], world[:, 2]]
    additions = []
    for target in builtin_virtual_directions():
        dots = measured @ target
        nearest = int(np.argmax(dots))
        angular_error = float(np.rad2deg(np.arccos(np.clip(dots[nearest], -1.0, 1.0))))
        # SourcePosition is stored as float32 in the portable representation;
        # this threshold also avoids creating a decimal alias that rounds back
        # onto an existing measured direction.
        if angular_error <= 0.02:
            continue
        if angular_error > 3.0:
            raise ValueError(f"canonical virtual direction is {angular_error:.3f} degrees from SADIE II grid")
        target_world = np.array([target[1], -target[0], target[2]])
        target_azimuth = np.rad2deg(np.arctan2(target_world[1], target_world[0])) % 360.0
        target_elevation = np.rad2deg(np.arcsin(target_world[2]))
        additions.append((target_azimuth, target_elevation, source_position[nearest, 2], ir[nearest]))
    if not additions:
        return source_position, ir
    added_positions = np.asarray([item[:3] for item in additions], dtype=np.float32)
    added_ir = np.asarray([item[3] for item in additions], dtype=np.float32)
    return np.concatenate((source_position, added_positions)), np.concatenate((ir, added_ir))


def build(source: Path, output: Path) -> None:
    with h5py.File(source, "r") as sofa:
        source_position = np.asarray(sofa["SourcePosition"][...], dtype=np.float32)
        ir = np.asarray(sofa["Data.IR"][...], dtype=np.float32)
        sampling_rate = np.asarray(sofa["Data.SamplingRate"][...], dtype=np.float32)
        delay = np.asarray(sofa["Data.Delay"][...], dtype=np.float32)
        listener_position = np.asarray(sofa["ListenerPosition"][...], dtype=np.float32)
        listener_view = np.asarray(sofa["ListenerView"][...], dtype=np.float32)
        listener_up = np.asarray(sofa["ListenerUp"][...], dtype=np.float32)
        receiver_position = np.asarray(sofa["ReceiverPosition"][...], dtype=np.float32)
        emitter_position = np.asarray(sofa["EmitterPosition"][...], dtype=np.float32)
        attrs = {
            key: value.decode("utf-8") if isinstance(value, bytes) else str(value)
            for key, value in sofa.attrs.items()
        }

    source_position, ir = append_virtual_aliases(source_position, ir, delay)

    if ir.ndim != 3 or ir.shape[1:] != (2, 256):
        raise ValueError(f"expected Data.IR [M,2,256], got {ir.shape}")
    if source_position.shape != (ir.shape[0], 3):
        raise ValueError("SourcePosition does not match Data.IR")
    if delay.shape == (1, 2):
        delay = delay.reshape(2)
    if delay.shape not in {(2,), (ir.shape[0], 2)}:
        raise ValueError(f"expected Data.Delay [2] or [M,2], got {delay.shape}")
    if float(sampling_rate[0]) != 48000.0:
        raise ValueError("the built-in resource must be native 48 kHz")

    dimensions = [
        ("M", ir.shape[0]),
        ("R", 2),
        ("N", 256),
        ("I", 1),
        ("C", 3),
        ("E", 1),
    ]
    variables: list[tuple[str, list[int], list[tuple[str, str]], int, bytes]] = [
        ("ListenerPosition", [3, 4], [("Units", "metre")], NC_FLOAT, float32_bytes(listener_position.reshape(1, 3))),
        ("ReceiverPosition", [1, 4], [("Units", "metre")], NC_FLOAT, float32_bytes(receiver_position.reshape(2, 3))),
        ("SourcePosition", [0, 4], [("Type", "spherical"), ("Units", "degree, degree, metre")], NC_FLOAT, float32_bytes(source_position)),
        ("EmitterPosition", [5, 4], [("Units", "metre")], NC_FLOAT, float32_bytes(emitter_position.reshape(1, 3))),
        ("ListenerUp", [3, 4], [("Units", "metre")], NC_FLOAT, float32_bytes(listener_up.reshape(1, 3))),
        ("ListenerView", [3, 4], [("Units", "metre")], NC_FLOAT, float32_bytes(listener_view.reshape(1, 3))),
        ("Data.IR", [0, 1, 2], [], NC_FLOAT, float32_bytes(ir)),
        ("Data.SamplingRate", [3], [("Units", "hertz")], NC_FLOAT, float32_bytes(sampling_rate.reshape(1))),
        ("Data.Delay", [1] if delay.shape == (2,) else [0, 1], [("Units", "samples")], NC_FLOAT, float32_bytes(delay)),
    ]
    global_attributes = [
        ("Conventions", "SOFA"),
        ("SOFAConventions", "SimpleFreeFieldHRIR"),
        ("SOFAConventionsVersion", "1.0"),
        ("DataType", "FIR"),
        ("RoomType", "free field"),
        ("Title", attrs.get("Title", "D1 HRIRs")),
        ("DatabaseName", attrs.get("DatabaseName", "SADIE II")),
        ("ListenerShortName", attrs.get("ListenerShortName", "D1")),
        ("License", attrs.get("License", "Apache-2.0; University of York SADIE II attribution required")),
        ("References", attrs.get("References", "https://doi.org/10.3390/app8112029")),
    ]

    header = b"CDF\1" + struct.pack(">I", 0)
    header += nc_dims(dimensions)
    header += struct.pack(">II", 12, len(global_attributes))
    header += b"".join(nc_attr(name, value) for name, value in global_attributes)
    header += struct.pack(">II", 11, len(variables))
    # Variable header sizes do not depend on their begin offsets. Build a
    # zero-offset pass to determine the final header length, then write the
    # authoritative offsets in a second pass.
    provisional = [nc_var(name, dims, attributes, ty, len(data), 0) for name, dims, attributes, ty, data in variables]
    header += b"".join(provisional)
    header_size = len(header)
    variable_headers = []
    offset = header_size
    for name, dims, attributes, ty, data in variables:
        offset = (offset + 3) & ~3
        variable_headers.append(nc_var(name, dims, attributes, ty, len(data), offset))
        offset += len(data)
    header = header[: header_size - sum(len(item) for item in provisional)]
    header += b"".join(variable_headers)
    if len(header) != header_size:
        raise AssertionError("CDF-1 header size accounting failed")
    blob = bytearray(header)
    for (_, _, _, _, data), var_header in zip(variables, variable_headers):
        while len(blob) % 4:
            blob.append(0)
        blob.extend(data)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_bytes(blob)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("source", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    build(args.source, args.output)


if __name__ == "__main__":
    main()
