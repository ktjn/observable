"""Real Bash cleanup lifecycle preserves failures and removes verification fixtures."""
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import tempfile
import unittest

ROOT=Path(__file__).resolve().parents[2]


class LocalCiCleanupTests(unittest.TestCase):
    def cleanup(self,main_status,down_status,started=1):
        bash=shutil.which('bash')
        if os.name=='nt':
            candidate=Path(shutil.which('git')).parent.parent/'bin/bash.exe'
            if candidate.exists():bash=str(candidate)
        self.assertIsNotNone(bash,'Bash is required for shell cleanup regressions')
        prefix,separator,_=(ROOT/'scripts/local-ci.sh').read_text().partition('\ntrap cleanup_smoke_compose EXIT')
        self.assertTrue(separator,'cleanup trap declaration missing')
        with tempfile.TemporaryDirectory(prefix='observable-ci-cleanup-') as directory:
            repo=Path(directory)
            harness=prefix+f"""
docker() {{ printf '%s\n' "$*" > down-args; return {down_status}; }}
SMOKE_COMPOSE_STARTED={started}
trap cleanup_smoke_compose EXIT
exit {main_status}
"""
            (repo/'cleanup.sh').write_text(harness,newline='\n')
            result=subprocess.run([bash,'cleanup.sh'],cwd=repo,capture_output=True,text=True,timeout=30)
            args=shlex.split((repo/'down-args').read_text()) if (repo/'down-args').exists() else None
            return result,args

    def test_cleanup_covers_verification_profile_and_preserves_exit_status(self):
        for main,down,expected in ((0,0,0),(7,0,7),(0,9,9),(7,9,7)):
            with self.subTest(main=main,down=down):
                result,args=self.cleanup(main,down)
                self.assertEqual(result.returncode,expected,result.stdout+result.stderr)
                self.assertEqual(args,['compose','--profile','verification','down','--remove-orphans'])

    def test_no_compose_cleanup_before_smoke_started(self):
        result,args=self.cleanup(7,9,started=0)
        self.assertEqual(result.returncode,7,result.stdout+result.stderr)
        self.assertIsNone(args)


if __name__=='__main__':unittest.main()
