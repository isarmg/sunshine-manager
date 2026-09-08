import importlib.util
import json
from pathlib import Path
import re
import subprocess
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location('release_manifest', ROOT / 'scripts/write-release-manifest.py')
MANIFEST = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MANIFEST)


class ReleaseIdentityTests(unittest.TestCase):
    def identity(self, revision):
        return {
            'manifest_format': MANIFEST.CONTRACT_FORMAT,
            'application': MANIFEST.APPLICATION,
            'version': MANIFEST.VERSION,
            'api_prefix': '/api/v2',
            'schema_revision': revision,
            'schema_sha256': 'a' * 64,
            'target': MANIFEST.TARGET,
            'source_revision': 'b' * 40,
        }

    def check_identity(self, revision):
        identity = self.identity(revision)
        result = subprocess.CompletedProcess([], 0, (json.dumps(identity) + '\n').encode(), b'')
        with patch.object(MANIFEST.subprocess, 'run', return_value=result):
            return MANIFEST.read_identity(Path('/fixture/sunshine-manager'))[0]

    def test_accepts_current_source_schema(self):
        source = (ROOT / 'src/database_schema.rs').read_text()
        revision = int(re.search(r'pub const SCHEMA_REVISION: i64 = (\d+);', source).group(1))
        self.assertEqual(self.check_identity(revision), self.identity(revision))

    def test_rejects_previous_schema(self):
        with self.assertRaises(SystemExit):
            self.check_identity(5)


if __name__ == '__main__':
    unittest.main()
