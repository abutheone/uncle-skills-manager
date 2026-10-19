"""Network-free installer lifecycle checks. Run with Python standard library only."""
import hashlib
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / 'install.sh'


@unittest.skipIf(os.name == 'nt', 'Unix installer requires sh')
class ShellInstallerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='uncle-installer-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.mock = self.root / 'mock'
        self.mock.mkdir()
        self.fixture = self.root / 'fixture'
        self.fixture.mkdir()
        self.destination = self.root / 'bin' / 'uncle-skills'
        scripts = {
            'uname': '#!/bin/sh\ncase "$1" in -s) echo Linux;; -m) echo "${MOCK_ARCH:-x86_64}";; esac\n',
            'getconf': '#!/bin/sh\n[ "${MOCK_MUSL:-no}" = no ] || exit 1\necho "glibc 2.39"\n',
            'curl': '''#!/bin/sh
url= out=
while [ "$#" -gt 0 ]; do
case "$1" in https://*) url=$1;; -o) shift; out=$1;; esac
shift
done
cp "$MOCK_FIXTURE/${url##*/}" "$out"
''',
        }
        for name, body in scripts.items():
            path = self.mock / name
            path.write_text(body)
            path.chmod(0o755)
        self.env = dict(os.environ, PATH=str(self.mock) + os.pathsep + os.environ['PATH'],
                        TERMUX_VERSION='', ANDROID_ROOT='',
                        UNCLE_SKILLS_BIN_DIR=str(self.destination.parent),
                        UNCLE_SKILLS_VERSION='latest', MOCK_FIXTURE=str(self.fixture))
        self.asset = self.make_asset('x86_64-unknown-linux-gnu')

    def make_asset(self, target):
        asset = f'uncle-skills-{target}.tar.gz'
        binary = self.fixture / 'uncle-skills'
        binary.write_text('#!/bin/sh\necho fixture\n')
        with tarfile.open(self.fixture / asset, 'w:gz') as archive:
            archive.add(binary, arcname='uncle-skills')
        checksum = hashlib.sha256((self.fixture / asset).read_bytes()).hexdigest()
        (self.fixture / (asset + '.sha256')).write_text(checksum + '  ' + asset + '\n')
        return asset

    def run_installer(self, *args):
        return subprocess.run(['sh', str(SCRIPT), *args], env=self.env,
                              text=True, capture_output=True, check=False)

    def test_fresh_install(self):
        result = self.run_installer()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.destination.read_text(), '#!/bin/sh\necho fixture\n')
        self.assertTrue(os.access(self.destination, os.X_OK))

    def test_existing_command_requires_explicit_replace(self):
        self.destination.parent.mkdir()
        self.destination.write_text('unrelated user command')
        result = self.run_installer()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('--replace', result.stderr)
        self.assertEqual(self.destination.read_text(), 'unrelated user command')
        result = self.run_installer('--replace')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.destination.read_text(), '#!/bin/sh\necho fixture\n')

    def test_checksum_failure_preserves_existing_command_even_with_replace(self):
        self.destination.parent.mkdir()
        self.destination.write_text('keep me')
        (self.fixture / (self.asset + '.sha256')).write_text('0' * 64 + '  ' + self.asset + '\n')
        result = self.run_installer('--replace')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('Checksum mismatch', result.stderr)
        self.assertEqual(self.destination.read_text(), 'keep me')

    def test_directory_never_replaced(self):
        self.destination.mkdir(parents=True)
        result = self.run_installer('--replace')
        self.assertNotEqual(result.returncode, 0)
        self.assertTrue(self.destination.is_dir())

    def test_broken_symlink_is_existing_destination(self):
        self.destination.parent.mkdir()
        self.destination.symlink_to(self.root / 'missing')
        result = self.run_installer()
        self.assertNotEqual(result.returncode, 0)
        self.assertTrue(self.destination.is_symlink())

    def test_termux_selects_android_asset(self):
        self.env.update(TERMUX_VERSION='test', MOCK_ARCH='aarch64')
        self.make_asset('aarch64-linux-android')
        result = self.run_installer()
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_musl_rejected_without_installing_glibc_binary(self):
        self.env['MOCK_MUSL'] = 'yes'
        result = self.run_installer()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('musl', result.stderr)
        self.assertFalse(self.destination.exists())

    def test_unknown_argument_rejected(self):
        self.assertNotEqual(self.run_installer('--force').returncode, 0)


if __name__ == '__main__':
    unittest.main()
