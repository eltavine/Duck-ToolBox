#!/usr/bin/env python3
"""Embedded setup and native startup gate contract checks."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "module/device-faker.sh"
class EmbeddedInstall(unittest.TestCase):
    def test_setup_calls_manager_without_creating_a_sibling_module(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            module = root / "modules_update/duck-toolbox"
            binary = module / "device-faker/device-faker-engine"
            binary.parent.mkdir(parents=True)
            binary.write_text('#!/bin/sh\nprintf "%s\\n" "$*" > "$MODPATH/called"\n')
            result = subprocess.run(['sh', '-c', '. "$SCRIPT"; setup_device_faker'],
                env={**os.environ, 'SCRIPT': str(SCRIPT), 'MODPATH': str(module)}, capture_output=True, timeout=10)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual((module / 'called').read_text().strip(), 'stage setup')
            self.assertEqual(list(module.parent.iterdir()), [module])
    def test_setup_failure_is_reported(self):
        with tempfile.TemporaryDirectory() as directory:
            module = Path(directory)
            binary = module / 'device-faker/device-faker-engine'
            binary.parent.mkdir()
            binary.write_text('#!/bin/sh\nexit 1\n')
            result = subprocess.run(['sh', '-c', '. "$SCRIPT"; setup_device_faker'],
                env={**os.environ, 'SCRIPT': str(SCRIPT), 'MODPATH': str(module)}, timeout=10)
            self.assertNotEqual(result.returncode, 0)
class NativeGate(unittest.TestCase):
    @unittest.skipUnless(shutil.which('rustc'), 'Rust compiler unavailable')
    def test_parent_flags_boot_order_and_standalone_conflict(self):
        with tempfile.TemporaryDirectory() as directory:
            binary = Path(directory) / 'gate-test'
            subprocess.run(['rustc','--edition=2024','--test',str(ROOT/'components/device-faker/engine/src/toolbox.rs'),'-o',str(binary)],check=True)
            subprocess.run([str(binary)],check=True,timeout=10)
    @unittest.skipUnless(shutil.which('rustc'), 'Rust compiler unavailable')
    def test_mapping_permissions_and_bounded_context_names(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / 'maps.rs'
            source.write_text('const PROP_PREFIX: &str = "/dev/__properties__/";\n'
                '#[allow(dead_code)]\n#[path = ' + repr(str(ROOT/'components/device-faker/engine/src/prop_backend/maps.rs')).replace("'", '"') + '] mod maps;\n')
            binary = root / 'maps-test'
            subprocess.run(['rustc','--edition=2024','--test',str(source),'-o',str(binary)],check=True)
            subprocess.run([str(binary)],check=True,timeout=10)
if __name__ == '__main__':
    unittest.main()
