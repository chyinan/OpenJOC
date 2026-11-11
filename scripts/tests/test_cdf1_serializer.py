"""Independent CDF-1 header checks without optional HDF5/NumPy dependencies."""

import ast
from pathlib import Path
import struct
import unittest


ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "tools" / "generate-builtin-hrtf.py"


def serializer_functions():
    # Execute the production serializer, not a copied implementation. These
    # functions use only struct; full HDF5 conversion is outside this test.
    names = {"pad4", "nc_string", "nc_attr", "nc_dims", "nc_var"}
    tree = ast.parse(SOURCE.read_text(encoding="utf-8"))
    selected = [
        node for node in tree.body
        if isinstance(node, ast.FunctionDef) and node.name in names
    ]
    assert {node.name for node in selected} == names
    namespace = {"struct": struct, "NC_CHAR": 2}
    exec(compile(ast.Module(body=selected, type_ignores=[]), str(SOURCE), "exec"), namespace)
    return namespace


class HeaderReader:
    """Small independent reader for the fixed-width CDF-1 header grammar."""

    def __init__(self, data):
        self.data = data
        self.position = 0

    def integer(self):
        value = struct.unpack_from(">I", self.data, self.position)[0]
        self.position += 4
        return value

    def string(self):
        size = self.integer()
        value = self.data[self.position:self.position + size].decode("utf-8")
        self.position += (size + 3) & ~3
        return value

    def attributes(self):
        tag, count = self.integer(), self.integer()
        if tag == 0:
            if count != 0:
                raise ValueError("ABSENT requires two zero words")
            return {}
        if tag != 12:
            raise ValueError("invalid attribute tag")
        result = {}
        for _ in range(count):
            name, kind, length = self.string(), self.integer(), self.integer()
            if kind != 2:
                raise ValueError("fixture reader expects character attributes")
            result[name] = self.data[self.position:self.position + length].decode("utf-8")
            self.position += (length + 3) & ~3
        return result

    def variables(self):
        if self.data[:4] != b"CDF\x01":
            raise ValueError("not CDF-1")
        self.position = 4
        if self.integer() != 0 or self.integer() != 10:
            raise ValueError("expected fixed dimensions")
        for _ in range(self.integer()):
            self.string()
            self.integer()
        self.attributes()
        if self.integer() != 11:
            raise ValueError("expected variables")
        variables = {}
        for _ in range(self.integer()):
            name = self.string()
            dims = [self.integer() for _ in range(self.integer())]
            attribute_offset = self.position
            attrs = self.attributes()
            kind, size, begin = self.integer(), self.integer(), self.integer()
            if kind not in range(1, 7) or begin + size > len(self.data):
                raise ValueError("invalid variable type or extent")
            variables[name] = {
                "dims": dims, "attrs": attrs, "kind": kind,
                "bytes": self.data[begin:begin + size],
                "attribute_offset": attribute_offset,
            }
        return variables


class Cdf1SerializerTests(unittest.TestCase):
    def test_empty_attributes_use_two_zero_words(self):
        functions = serializer_functions()
        for dims in ([], [0], [0, 1, 2]):
            with self.subTest(dims=dims):
                data = functions["nc_var"]("Data.IR", dims, [], 5, 16, 128)
                reader = HeaderReader(data)
                self.assertEqual(reader.string(), "Data.IR")
                self.assertEqual(reader.integer(), len(dims))
                self.assertEqual([reader.integer() for _ in dims], dims)
                self.assertEqual(reader.attributes(), {})
                self.assertEqual([reader.integer() for _ in range(3)], [5, 16, 128])
                self.assertEqual(reader.position, len(data))

    def test_nonempty_and_unicode_attributes_keep_the_standard_layout(self):
        functions = serializer_functions()
        attrs = [("Units", "samples"), ("Title", "合成 HRIR")]
        data = functions["nc_var"]("Data.Delay", [1], attrs, 6, 16, 256)
        reader = HeaderReader(data)
        self.assertEqual(reader.string(), "Data.Delay")
        self.assertEqual([reader.integer(), reader.integer()], [1, 1])
        self.assertEqual(reader.attributes(), dict(attrs))
        self.assertEqual([reader.integer() for _ in range(3)], [6, 16, 256])
        self.assertEqual(reader.position, len(data))

    def test_complete_generated_container_has_exact_payload_and_offsets(self):
        functions = serializer_functions()
        prefix = b"CDF\x01" + struct.pack(">I", 0)
        prefix += functions["nc_dims"]([("N", 2)])
        prefix += struct.pack(">II", 0, 0) + struct.pack(">II", 11, 2)
        payloads = [struct.pack(">ff", 1.0, -0.0), struct.pack(">dd", 0.5, -2.0)]
        specs = [("first", [], 5), ("second", [("Units", "samples")], 6)]
        provisional = [
            functions["nc_var"](name, [0], attrs, kind, len(payload), 0)
            for (name, attrs, kind), payload in zip(specs, payloads)
        ]
        offset = len(prefix) + sum(map(len, provisional))
        headers = []
        for (name, attrs, kind), payload in zip(specs, payloads):
            headers.append(functions["nc_var"](name, [0], attrs, kind, len(payload), offset))
            offset += len(payload)
        data = prefix + b"".join(headers) + b"".join(payloads)
        variables = HeaderReader(data).variables()
        self.assertEqual(list(variables), ["first", "second"])
        for (name, attrs, kind), payload in zip(specs, payloads):
            self.assertEqual(variables[name]["bytes"], payload)
            self.assertEqual(variables[name]["attrs"], dict(attrs))
            self.assertEqual(variables[name]["kind"], kind)

    def test_independent_sofa_fixture_has_standard_absent_attributes(self):
        path = ROOT / "crates/openjoc-sofa/tests/fixtures/cdf1-standard-absent.sofa"
        data = path.read_bytes()
        variables = HeaderReader(data).variables()
        for name in ["Data.IR", "ListenerUp"]:
            offset = variables[name]["attribute_offset"]
            self.assertEqual(data[offset:offset + 8], bytes(8))
            self.assertEqual(variables[name]["attrs"], {})
        self.assertEqual(variables["Data.IR"]["kind"], 6)
        self.assertEqual(
            struct.unpack(">8Q", variables["Data.IR"]["bytes"]),
            (0x3FF0000000000000, 0x8000000000000000, 1, 0xC000000000000000,
             0x4008000000000000, 0x4010000000000000, 0x8000000000000000, 0x3FE0000000000000),
        )
        self.assertEqual(struct.unpack(">2d", variables["Data.Delay"]["bytes"]), (0, 1))


if __name__ == "__main__":
    unittest.main()
