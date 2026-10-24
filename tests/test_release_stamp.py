"""Tests for packaging/release_stamp.py, which writes the release.json stamp."""
import json
import subprocess
import sys
import unittest

from paths import ROOT


def stamp(tag, prerelease):
    return subprocess.run(
        [sys.executable, '-I', str(ROOT / 'packaging/release_stamp.py'), tag, '--prerelease', prerelease],
        capture_output=True,
        text=True,
    )


class ReleaseStampTest(unittest.TestCase):
    def test_a_stable_tag(self):
        result = stamp('windows-v2.7.0', 'false')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout), {'channel': 'release', 'version': '2.7.0'})

    def test_a_test_build_tag(self):
        result = stamp('windows-v2.6.0-issue209-test1', 'true')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            json.loads(result.stdout),
            {'channel': 'pre-release', 'version': '2.6.0', 'build': 'issue209-test1'},
        )

    def test_a_linux_tag(self):
        result = stamp('linux-v2.2.1-test1', 'true')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            json.loads(result.stdout),
            {'channel': 'pre-release', 'version': '2.2.1', 'build': 'test1'},
        )

    def test_a_stable_under_a_build_tag(self):
        result = stamp('windows-v2.6.0-hotfix1', 'false')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            json.loads(result.stdout),
            {'channel': 'release', 'version': '2.6.0', 'build': 'hotfix1'},
        )

    def test_an_invalid_tag_fails(self):
        result = stamp('v2.7.0', 'false')
        self.assertEqual(result.returncode, 1)
        self.assertEqual(result.stderr, 'invalid release tag: v2.7.0\n')


if __name__ == '__main__':
    unittest.main()
