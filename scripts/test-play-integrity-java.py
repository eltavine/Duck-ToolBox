#!/usr/bin/env python3
"""Verify the engine's actual Build value conversion without Android or JNI."""
from pathlib import Path
import subprocess
import tempfile
import unittest

SCRIPT_DIR = Path(__file__).resolve().parent
ENGINE = SCRIPT_DIR.parent / "components/play-integrity"


class BuildValues(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temporary = tempfile.TemporaryDirectory(prefix="pif-java-values-")
        cls.classes = Path(cls.temporary.name)
        source = ENGINE / "android/app/src/main/java/es/chiteroman/playintegrityfix/FieldValue.java"
        subprocess.run(["javac", "--release", "8", "-d", str(cls.classes),
                        str(source), str(ENGINE / "tests/FieldValueTest.java")], check=True)

    @classmethod
    def tearDownClass(cls):
        cls.temporary.cleanup()

    def run_case(self, case):
        subprocess.run(["java", "-cp", str(self.classes),
                        "es.chiteroman.playintegrityfix.FieldValueTest", case],
                       check=True, timeout=10)

    def test_normalized_and_textual_booleans(self):
        self.run_case("booleans")

    def test_integer_and_long_boundaries(self):
        self.run_case("numbers")

    def test_string_fields_keep_literal_values(self):
        self.run_case("strings")

    def test_invalid_values_are_rejected_without_coercion(self):
        self.run_case("invalid")


if __name__ == "__main__":
    unittest.main()
