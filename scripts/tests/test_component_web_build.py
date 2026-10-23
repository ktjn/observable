"""Native production compilation consumes committed source and verified npm materials."""

import json
import os
from pathlib import Path
import subprocess
import sys
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
import build_component_web as web
import prepare_component_npm as prepare
import test_component_npm_materials as fixtures


class WebBuildTests(unittest.TestCase):
    git = fixtures.NpmMaterialTests.git
    write = fixtures.NpmMaterialTests.write
    commit = fixtures.NpmMaterialTests.commit
    fetch = fixtures.NpmMaterialTests.fetch
    create = fixtures.NpmMaterialTests.create

    def setUp(self):
        fixtures.NpmMaterialTests.setUp(self)
        self.write('apps/frontend/package.json', json.dumps({'name':'fixture-web','version':'0.1.0','dependencies':{'fixture':'1.0.0'},'scripts':{'build':'tsc && vite build'}}))
        self.write('apps/frontend/tsconfig.json', '{}')
        self.write('apps/frontend/vite.config.ts', '// approved config')
        self.lock['packages']['apps/frontend/node_modules/typescript'] = dict(self.lock['packages']['node_modules/fixture'])
        self.lock['packages']['node_modules/vite'] = dict(self.lock['packages']['node_modules/fixture'])
        self.write('package-lock.json', json.dumps(self.lock))
        self.commit()
        self.create()
        self.build_output = self.root / 'build'
        self.commands = []
        self.failure = None
        self.effect = None

    def execute(self, command, **kwargs):
        self.commands.append((command, kwargs))
        workspace = self.build_output / 'dependencies/workspace'
        if 'ci' in command:
            (workspace / 'node_modules').mkdir()
            for path in ('apps/frontend/node_modules/typescript/bin/tsc', 'node_modules/vite/bin/vite.js'):
                target = workspace / path
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_text('// fixture')
                package = target.parent.parent
                name, binary = ('typescript', 'tsc') if path.endswith('/tsc') else ('vite', 'vite')
                (package / 'package.json').write_text(json.dumps({'name':name,'bin':{binary:'bin/' + target.name}}))
        elif command[1].endswith('vite.js') and self.failure != 'missing':
            output = workspace / 'apps/frontend/dist'
            output.mkdir()
            (output / 'index.html').write_text('<script src="/app.js"></script>')
            (output / 'app.js').write_text('console.log(1)')
            if self.effect:
                self.effect(workspace)
        failed = self.failure and any(self.failure in item for item in command)
        return subprocess.CompletedProcess(command, int(bool(failed)), b'PRIVATE_DIAGNOSTIC', b'PRIVATE_DIAGNOSTIC')

    def build(self, **kwargs):
        with patch.object(prepare, 'npm_command', return_value=['node', 'npm-cli.js']):
            return web.build(self.root, 'HEAD', self.output, self.build_output, run=self.execute, **kwargs)

    def test_committed_source_nearest_entrypoints_explicit_environment_and_output_inventory(self):
        committed = self.git('rev-parse', 'HEAD')
        self.write('apps/frontend/src/main.ts', 'dirty checkout')
        self.write('apps/frontend/public/ignored.js', 'uncommitted')
        with patch.dict(os.environ, {'VITE_ATTACK': 'inherited', 'NODE_OPTIONS': '--bad', 'HTTP_PROXY': 'bad'}):
            result = self.build(build_args={'VITE_OBSERVABLE_SELF_OBSERVABILITY_MODE': 'observer_instance'})
        workspace = self.build_output / 'dependencies/workspace'
        self.assertEqual((workspace / 'apps/frontend/src/main.ts').read_text(), '// web\n')
        self.assertFalse((workspace / 'apps/frontend/public/ignored.js').exists())
        compiler = self.commands[-2:]
        self.assertTrue(Path(compiler[0][0][1]).as_posix().endswith('apps/frontend/node_modules/typescript/bin/tsc'))
        self.assertTrue(Path(compiler[1][0][1]).as_posix().endswith('node_modules/vite/bin/vite.js'))
        self.assertEqual(compiler[1][0][2:], ['build', '--mode', 'production'])
        self.assertEqual(compiler[1][1]['cwd'], workspace / 'apps/frontend')
        for _, options in compiler:
            self.assertNotIn('VITE_ATTACK', options['env'])
            self.assertNotIn('NODE_OPTIONS', options['env'])
            self.assertNotIn('HTTP_PROXY', options['env'])
            self.assertEqual(options['env']['VITE_OBSERVABLE_SELF_OBSERVABILITY_MODE'], 'observer_instance')
            self.assertEqual(options['env']['VITE_OBSERVABLE_SELF_OBSERVABILITY_OTLP_ENDPOINT'], '')
            self.assertEqual(options['env']['NODE_ENV'], 'production')
        self.assertEqual(result['source_revision'], committed)
        self.assertTrue(result['compilation_verified'])
        self.assertFalse(result['network_isolation_verified'])
        self.assertFalse(result['identity_verified'])
        self.assertFalse(result['publication_authorized'])
        self.assertEqual([item['path'] for item in result['artifacts']], ['app.js', 'index.html'])
        self.assertTrue((self.build_output / 'result.json').is_file())

    def test_forbidden_source_is_rejected_before_install(self):
        for index, path in enumerate(('.env', '.env.production', '.ENV.production.local', 'node_modules/x.js', 'Node_Modules/case.js', 'dist/index.html', '.npmrc')):
            self.write('apps/frontend/' + path, 'untrusted')
            self.commit()
            self.build_output = self.root / f'rejected-{index}'
            with self.subTest(path=path), self.assertRaises(web.BuildFailed):
                self.build()
            self.assertEqual(self.commands, [])
            self.assertFalse(self.build_output.exists())
            (self.root / ('apps/frontend/' + path)).unlink()

    def test_compiler_failure_missing_artifact_and_modified_source_have_no_build_success(self):
        for index, failure in enumerate(('tsc', 'vite.js', 'missing', 'modified')):
            self.failure = failure
            self.build_output = self.root / f'failed-{index}'
            self.effect = (lambda workspace: (workspace / 'apps/frontend/src/main.ts').write_text('changed')) if failure == 'modified' else None
            with self.subTest(failure=failure), self.assertRaises(web.BuildFailed) as caught:
                self.build()
            self.assertNotIn('PRIVATE_DIAGNOSTIC', str(caught.exception))
            self.assertFalse((self.build_output / 'result.json').exists())

    def test_changed_build_script_requires_review_before_install(self):
        package = json.loads((self.root / 'apps/frontend/package.json').read_text())
        package['scripts']['build'] = 'custom-command'
        self.write('apps/frontend/package.json', json.dumps(package)); self.commit()
        with self.assertRaises(web.BuildFailed): self.build()
        self.assertEqual(self.commands, [])
        self.assertFalse(self.build_output.exists())

    def test_unsafe_compiler_entrypoint_is_rejected(self):
        directory = self.root / 'installed'
        package = directory / 'node_modules/vite'
        package.mkdir(parents=True)
        (package / 'package.json').write_text(json.dumps({'name':'vite','bin':{'vite':'../../outside.js'}}))
        with self.assertRaises(ValueError):
            web.entrypoint(directory, [{'path':'node_modules/vite'}], 'vite', 'vite')

    def test_artifact_inventory_rejects_missing_js_and_oversize_content(self):
        directory = self.root / 'dist'; directory.mkdir()
        (directory / 'index.html').write_text('<html>test</html>')
        with self.assertRaises(web.BuildFailed): web.artifact_inventory(directory)
        (directory / 'app.js').write_text('code')
        with patch.object(web.materials, 'MAX_EXPANDED', 2), self.assertRaises(ValueError):
            web.artifact_inventory(directory)

    def test_unknown_arguments_and_existing_output_fail_without_commands(self):
        with self.assertRaises(web.BuildFailed): self.build(build_args={'VITE_UNKNOWN': 'bad'})
        self.assertFalse(self.build_output.exists())
        self.build_output.mkdir()
        keep = self.build_output / 'keep'; keep.write_text('keep')
        with self.assertRaises(web.BuildFailed): self.build()
        self.assertEqual(keep.read_text(), 'keep')
        self.assertEqual(self.commands, [])


if __name__ == '__main__': unittest.main()
