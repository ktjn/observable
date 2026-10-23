"""Network-disabled local web OCI builds bind approved bytes to immutable images."""

import io
import hashlib
import shutil
import tempfile
import json
from pathlib import Path
import subprocess
import sys
import tarfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
import build_component_web_image as image
import test_component_web_build as fixtures


class OfflineImageTests(unittest.TestCase):
    git = fixtures.WebBuildTests.git
    write = fixtures.WebBuildTests.write
    commit = fixtures.WebBuildTests.commit
    fetch = fixtures.WebBuildTests.fetch
    create = fixtures.WebBuildTests.create

    def setUp(self):
        fixtures.WebBuildTests.setUp(self)
        self.write('apps/frontend/nginx.conf', 'server { listen 80; }')
        self.commit()
        self.image_output = self.root / 'image'
        self.calls = []
        self.image_digest = 'sha256:' + 'a' * 64
        self.config_digest = 'sha256:' + 'b' * 64
        self.failure = None
        self.context = None

    def execute(self, command, **kwargs):
        self.calls.append(command)
        if 'buildx' in command:
            self.context = kwargs['input']
            labels = dict(command[index + 1].split('=', 1) for index, item in enumerate(command) if item == '--label')
            self.inspected = [{'Id':self.image_digest, 'Descriptor':{'digest':self.image_digest},
                               'Os':'linux', 'Architecture':'amd64', 'Config':{'Labels':labels}}]
            if self.failure == 'platform': self.inspected[0]['Architecture'] = 'arm64'
            if self.failure == 'labels': self.inspected[0]['Config']['Labels'] = {}
            if self.failure == 'digest': self.inspected[0]['Descriptor']['digest'] = self.config_digest
            Path(command[command.index('--metadata-file') + 1]).write_text(json.dumps({
                'containerimage.digest':self.image_digest,'containerimage.config.digest':self.config_digest}))
            Path(command[command.index('--iidfile') + 1]).write_text(self.config_digest if self.failure == 'classic' else self.image_digest)
            if self.failure == 'classic':
                self.inspected[0].pop('Descriptor'); self.inspected[0]['Id'] = self.config_digest
            return subprocess.CompletedProcess(command, int(self.failure == 'build'), b'', b'PRIVATE_DIAGNOSTIC')
        return subprocess.CompletedProcess(command, 0, json.dumps(self.inspected).encode(), b'')

    def build(self, **kwargs):
        return image.build(self.root, 'HEAD', self.output, self.image_output, run=self.execute, **kwargs)

    def test_verified_context_network_and_cache_flags_and_immutable_image_binding(self):
        self.write('apps/frontend/src/main.ts', 'dirty')
        self.write('apps/frontend/ignored.js', 'untracked')
        result = self.build()
        command = self.calls[0]
        self.assertEqual(command[:3], ['docker','buildx','build'])
        self.assertEqual(command[command.index('--network') + 1], 'none')
        self.assertIn('--no-cache', command)
        self.assertIn('--load', command)
        self.assertNotIn('--push', command)
        self.assertNotIn('--cache-from', command)
        self.assertNotIn('--secret', command)
        self.assertEqual(command[-1], '-')
        self.assertEqual(self.calls[1], ['docker','image','inspect',self.image_digest])
        with tarfile.open(fileobj=io.BytesIO(self.context)) as archive:
            contents = {member.name:archive.extractfile(member).read() for member in archive.getmembers()}
            self.assertTrue(all(member.isfile() and member.mtime == 0 for member in archive.getmembers()))
        self.assertEqual(contents['workspace/apps/frontend/src/main.ts'], b'// web\n')
        self.assertNotIn('workspace/apps/frontend/ignored.js', contents)
        self.assertEqual(contents['.dockerignore'], b'')
        recipe = contents['Dockerfile.offline'].decode()
        self.assertIn('# syntax=docker/dockerfile:1.28@sha256:', recipe)
        runs = [line for line in recipe.splitlines() if line.startswith('RUN ')]
        self.assertTrue(runs)
        self.assertTrue(all(line.startswith('RUN --network=none ') for line in runs))
        self.assertNotIn('type=cache', recipe)
        self.assertIn('--include=dev', contents['offline/install.mjs'].decode())
        self.assertIn('--offline', contents['offline/install.mjs'].decode())
        self.assertIn('--ignore-scripts', contents['offline/install.mjs'].decode())
        self.assertIn('production', contents['offline/compile.mjs'].decode())
        self.assertEqual(result['image_id'], self.image_digest)
        self.assertTrue(result['run_network_disabled'])
        self.assertFalse(result['identity_verified'])
        self.assertFalse(result['publication_authorized'])
        self.assertTrue((self.image_output / 'result.json').is_file())

    def test_build_and_image_binding_failures_have_no_success_record(self):
        for failure in ('build', 'platform', 'labels', 'digest'):
            self.failure = failure
            self.image_output = self.root / ('bad-' + failure)
            with self.subTest(failure=failure), self.assertRaises(image.BuildFailed) as caught:
                self.build()
            self.assertNotIn('PRIVATE_DIAGNOSTIC', str(caught.exception))
            self.assertFalse((self.image_output / 'result.json').exists())

    def test_classic_config_digest_binding(self):
        self.failure = 'classic'
        result = self.build()
        self.assertEqual(result['image_id'], self.config_digest)
        self.assertEqual(self.calls[1], ['docker', 'image', 'inspect', self.config_digest])

    def test_corrupt_or_missing_archive_rejected_before_docker(self):
        manifest = json.loads((self.output / 'manifest.json').read_text())
        target = self.output / manifest['archives'][0]['path']
        target.write_bytes(b'corrupt')
        with self.assertRaises(image.BuildFailed): self.build()
        self.assertEqual(self.calls, [])
        target.unlink()
        self.image_output = self.root / 'missing'
        with self.assertRaises(image.BuildFailed): self.build()
        self.assertEqual(self.calls, [])

    def test_staged_archive_mutation_rejected_before_docker(self):
        stage = image.materials.stage_bundle
        def changed(*args, **kwargs):
            result = stage(*args, **kwargs)
            output = Path(args[3])
            target = output / result[2]['archives'][0]['path']
            target.write_bytes(b'changed after staging')
            return result
        with patch.object(image.materials, 'stage_bundle', side_effect=changed):
            with self.assertRaises(image.BuildFailed): self.build()
        self.assertEqual(self.calls, [])
        self.assertFalse((self.image_output / 'result.json').exists())

    def test_forbidden_source_unsupported_platform_and_existing_output_reject(self):
        with self.assertRaises(image.BuildFailed): self.build(platform='windows/amd64')
        self.assertFalse(self.image_output.exists())
        self.write('apps/frontend/.ENV.production', 'fixture'); self.commit()
        with self.assertRaises(image.BuildFailed): self.build()
        self.assertFalse(self.image_output.exists())
        (self.root / 'apps/frontend/.ENV.production').unlink(); self.commit()
        self.image_output.mkdir(); (self.image_output / 'keep').write_text('keep')
        with self.assertRaises(image.BuildFailed): self.build()
        self.assertEqual((self.image_output / 'keep').read_text(), 'keep')
        self.assertEqual(self.calls, [])


class GeneratedCompileTests(unittest.TestCase):
    """Execute generated JavaScript rejection paths, without Docker or network."""

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.workspace = self.root / 'workspace'
        self.offline = self.root / 'offline'; self.offline.mkdir()
        self.frontend = self.workspace / 'apps/frontend'; self.frontend.mkdir(parents=True)
        self.source = self.frontend / 'main.ts'; self.source.write_bytes(b'approved')
        (self.offline / 'source.json').write_text(json.dumps({'apps/frontend/main.ts':hashlib.sha256(b'approved').hexdigest()}))
        self.tools = []
        for name, binary in (('typescript', 'tsc'), ('vite', 'vite')):
            directory = self.workspace / 'node_modules' / name
            (directory / 'bin').mkdir(parents=True)
            (directory / 'package.json').write_text(json.dumps({'name':name,'bin':{binary:'bin/' + binary + '.js'}}))
            (directory / 'bin' / (binary + '.js')).write_text('process.exit(0);')
            self.tools.append({'name':name,'binary':binary,'directory':'node_modules/' + name,'args':[]})
        self.compiler = self.workspace / 'node_modules/typescript/bin/tsc.js'
        self.bundler = self.workspace / 'node_modules/vite/bin/vite.js'
        self.bundler.write_text("const fs=require('fs');fs.mkdirSync('dist');fs.writeFileSync('dist/index.html','html');fs.writeFileSync('dist/app.js','js');")
        (self.offline / 'tools.json').write_text(json.dumps(self.tools))

    def execute(self, *, code=None):
        runner = self.root / 'compile.mjs'
        # Substitute sandbox roots and Linux path semantics; execute the generated guards and limits.
        runner.write_text((code or image.COMPILE).replace("from 'node:path'", "from 'node:path/posix'").replace('/offline/', self.offline.as_posix() + '/').replace('/app/', self.workspace.as_posix() + '/'))
        node = shutil.which('node')
        self.assertIsNotNone(node, 'Node is required for generated runner regression tests')
        return subprocess.run([node,str(runner)],capture_output=True,timeout=30)

    def test_real_generated_compiler_accepts_regular_static_output(self):
        self.assertEqual(self.execute().returncode, 0)

    def test_real_generated_compiler_rejects_modified_source(self):
        self.compiler.write_text("require('fs').writeFileSync('main.ts','changed')")
        result = self.execute()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(b'committed build input changed', result.stderr)

    def test_real_generated_compiler_rejects_unsafe_entrypoint(self):
        package = self.workspace / 'node_modules/typescript/package.json'
        package.write_text(json.dumps({'name':'typescript','bin':{'tsc':'../../outside.js'}}))
        result = self.execute()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(b'unsafe compiler path', result.stderr)

    def test_real_generated_compiler_rejects_missing_static_output(self):
        self.bundler.write_text('process.exit(0)')
        self.assertNotEqual(self.execute().returncode, 0)

    def test_real_generated_artifact_size_bound(self):
        # Lower only the fixture's resource constant, preserving the actual production guard.
        result = self.execute(code=image.COMPILE.replace('536870912', '1'))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(b'artifact bound exceeded', result.stderr)


if __name__ == '__main__': unittest.main()
