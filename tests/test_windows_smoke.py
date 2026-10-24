"""packaging/windows/smoke-test.ps1 against a built package (SPARTAN_WINDOWS_ZIP=path\\to\\spartanport-windows.zip):
it passes on the package as built and fails when a DLL is missing or a Python or player file ships."""
from paths import ROOT
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
import zipfile

SCRIPT = ROOT / 'packaging' / 'windows' / 'smoke-test.ps1'
PACKAGE = os.environ.get('SPARTAN_WINDOWS_ZIP')


@unittest.skipUnless(sys.platform == 'win32' and PACKAGE, 'needs Windows and SPARTAN_WINDOWS_ZIP')
class WindowsSmokeTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory()
        cls.tree = Path(cls.tmp.name) / 'tree'
        with zipfile.ZipFile(PACKAGE) as package:
            package.extractall(cls.tree)

    @classmethod
    def tearDownClass(cls):
        cls.tmp.cleanup()

    def smoke(self, change=None):
        """Zips a copy of the package after `change(folder)` and returns the smoke test's exit code."""
        work = Path(tempfile.mkdtemp(dir=self.tmp.name))
        shutil.copytree(self.tree, work / 'tree')
        if change:
            change(work / 'tree' / 'spartanport-windows')
        archive = shutil.make_archive(str(work / 'spartanport-windows'), 'zip', work / 'tree')
        result = subprocess.run(['powershell', '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File',
                                 str(SCRIPT), archive], capture_output=True, text=True, timeout=300)
        print(result.stdout, result.stderr, file=sys.stderr)
        return result.returncode

    def test_the_built_package_passes(self):
        self.assertEqual(self.smoke(), 0)

    def test_a_missing_dll_fails(self):
        self.assertNotEqual(self.smoke(lambda tree: (tree / 'bin' / 'SDL3.dll').unlink()), 0)

    def test_a_python_file_in_the_package_fails(self):
        self.assertNotEqual(self.smoke(lambda tree: (tree / 'bin' / 'stray.py').write_text('print(1)')), 0)

    def test_a_shipped_player_file_fails(self):
        self.assertNotEqual(self.smoke(lambda tree: (tree / 'spartanport.ini').write_text('[spartanport]\n')), 0)


if __name__ == '__main__':
    unittest.main()
