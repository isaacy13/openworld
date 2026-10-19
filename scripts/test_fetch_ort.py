# SPDX-License-Identifier: Apache-2.0
"""The ONNX Runtime pins file parses, and a malformed one refuses. No network."""

import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import fetch_ort  # noqa: E402

SHA = "0" * 64


class PinsFile(unittest.TestCase):
    def test_the_committed_pins_have_one_wheel_and_one_library_per_platform(self):
        pins = fetch_ort.read_pins()
        self.assertIn("linux-x86_64", pins)
        for name, entry in pins.items():
            self.assertIsNotNone(entry["wheel"], name)
            self.assertEqual(sum(1 for f in entry["files"] if f[0] == "library"), 1, name)
            self.assertTrue(entry["wheel"][1].endswith(".whl"), name)

    def test_the_flatpak_takes_the_pinned_wheels(self):
        # flatpak-builder needs the digest in its own manifest. It must be the same pin.
        manifest = fetch_ort.ROOT / "desktop" / "packaging" / "app.openworld.OpenWorld.json"
        modules = json.loads(manifest.read_text())["modules"]
        sources = next(m for m in modules if isinstance(m, dict) and m.get("name") == "onnxruntime")["sources"]
        pins = fetch_ort.read_pins()
        self.assertEqual(len(sources), 2)
        for source in sources:
            sha, wheel = pins[f"linux-{source['only-arches'][0]}"]["wheel"]
            self.assertEqual(source["sha256"], sha)
            self.assertTrue(source["url"].endswith("/" + wheel), source["url"])

    def refuses(self, text):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "onnxruntime.pins"
            path.write_text(text)
            with self.assertRaises(SystemExit):
                fetch_ort.read_pins(path)

    def test_a_malformed_file_refuses(self):
        good = f"p wheel {SHA} x.whl -\np library {SHA} lib.so capi/lib.so\n"
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "onnxruntime.pins"
            path.write_text(good)
            self.assertEqual(fetch_ort.read_pins(path)["p"]["wheel"], (SHA, "x.whl"))
        self.refuses(f"p wheel {SHA} x.whl -\n")  # no library
        self.refuses(f"p library {SHA} lib.so capi/lib.so\n")  # no wheel
        self.refuses(good + f"p library {SHA} lib2.so capi/lib2.so\n")  # two libraries
        self.refuses(good.replace(SHA, "A" * 64, 1))  # not lowercase hex
        self.refuses(good.replace(SHA, "0" * 63, 1))  # too short
        self.refuses(good + "p shared x y z\n")  # unknown kind
        self.refuses(good + "p library\n")  # too few columns


if __name__ == "__main__":
    unittest.main()
