"""Committed build contexts and local-only Docker execution boundaries."""

import json
from pathlib import Path
import subprocess
import sys
import tarfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
import build_component_snapshot as build
import test_component_source as fixtures


class SnapshotBuildTests(unittest.TestCase):
    git = fixtures.SourceTests.git
    write = fixtures.SourceTests.write
    commit = fixtures.SourceTests.commit

    def setUp(self):
        fixtures.SourceTests.setUp(self)
        self.write('distribution/manifest.yaml', 'productVersion: 0.1.0\ncomponents:\n  query: 0.1.0\n  web: 0.1.0\n')
        self.write('apps/frontend/package.json', '{"name":"web","version":"0.1.0"}')
        self.commit()
        self.output = self.root / 'build-output'

    def prepare(self, component='query'):
        return build.prepare(self.root, component, 'HEAD', self.output)

    def tar_contents(self):
        with tarfile.open(self.output/'context.tar') as archive:
            return {member.name: archive.extractfile(member).read() for member in archive.getmembers()}

    def test_context_includes_all_workspace_sources_but_no_dirty_or_untracked_bytes(self):
        self.write('libs/shared/src/lib.rs', 'DIRTY')
        self.write('services/query-api/untracked-secret', 'UNTRACKED')
        self.prepare()
        files = self.tar_contents()
        self.assertEqual(files['libs/shared/src/lib.rs'], b'// source\n')
        self.assertIn('services/auth-service/src/lib.rs', files)
        self.assertNotIn('services/query-api/untracked-secret', files)
        self.assertNotIn('.git/config', files)

    def test_web_context_only_contains_recipe_inputs(self):
        self.prepare('web')
        files = self.tar_contents()
        self.assertIn('apps/frontend/Dockerfile', files)
        self.assertIn('.dockerignore', files)
        self.assertNotIn('Cargo.toml', files)
        self.assertFalse(any(path.startswith('services/') for path in files))

    def test_deterministic_tar_and_raw_git_attributes(self):
        self.write('.gitattributes', 'libs/shared/src/lib.rs export-ignore\n')
        self.commit()
        first = self.prepare()
        tar_bytes = (self.output/'context.tar').read_bytes()
        self.assertIn('libs/shared/src/lib.rs', self.tar_contents())
        second = build.prepare(self.root, 'query', 'HEAD', self.root/'other-output')
        self.assertEqual(tar_bytes, (self.root/'other-output/context.tar').read_bytes())
        self.assertEqual(first['context_digest'], second['context_digest'])
        with tarfile.open(self.output/'context.tar') as archive:
            self.assertTrue(all(member.mtime==0 and member.uid==0 and member.gid==0
                                and member.isfile() for member in archive.getmembers()))

    def test_executable_mode_is_preserved_in_context(self):
        self.git('update-index','--chmod=+x','libs/shared/src/lib.rs')
        self.git('-c','commit.gpgsign=false','commit','-qm','executable fixture')
        self.prepare()
        with tarfile.open(self.output/'context.tar') as archive:
            self.assertEqual(archive.getmember('libs/shared/src/lib.rs').mode,0o755)

    def test_unsafe_entry_in_unrelated_copied_crate_rejected(self):
        oid=subprocess.check_output(['git','hash-object','-w','--stdin'],input=b'../../outside',cwd=self.root).decode().strip()
        self.git('update-index','--add','--cacheinfo','120000',oid,'libs/dev/unsafe-link')
        self.git('-c','commit.gpgsign=false','commit','-qm','unsafe fixture')
        with self.assertRaises(build.BuildFailed):
            self.prepare()
        self.assertFalse(self.output.exists())

    def test_existing_output_is_preserved(self):
        self.output.mkdir()
        sentinel = self.output/'keep'
        sentinel.write_text('keep')
        with self.assertRaises(build.BuildFailed):
            self.prepare()
        self.assertEqual(sentinel.read_text(), 'keep')

    def test_version_disagreement_rejected(self):
        self.write('distribution/manifest.yaml', 'components:\n  query: 9.0.0\n')
        self.commit()
        with self.assertRaises(build.BuildFailed):
            self.prepare()
        self.assertFalse(self.output.exists())

    def fake_runner(self, malformed=None):
        def run(command, **kwargs):
            if command[:3] == ['docker','buildx','build']:
                self.assertIn('--load', command)
                self.assertNotIn('--push', command)
                self.assertEqual(command[-1], '-')
                self.assertEqual(kwargs['input'][:1], b'.')
                labels = dict(command[i+1].split('=',1) for i,arg in enumerate(command) if arg=='--label')
                self.labels = labels
                metadata = dict(zip(['containerimage.config.digest','containerimage.digest'],
                                    ['sha256:'+'a'*64, 'sha256:'+'b'*64]))
                if malformed == 'missing-metadata':
                    metadata = {}
                (self.output/'metadata.json').write_text(json.dumps(metadata))
                return subprocess.CompletedProcess(command, 7 if malformed=='exit' else 0, b'', b'private diagnostic')
            self.assertEqual(command[:3], ['docker','image','inspect'])
            image = {'Id':'sha256:'+'a'*64, 'Os':'linux', 'Architecture':'amd64',
                     'Config':{'Labels':self.labels}}
            if malformed == 'wrong-label':
                image['Config']['Labels'] = {}
            if malformed == 'wrong-id':
                image['Id'] = 'sha256:'+'c'*64
            return subprocess.CompletedProcess(command, 0, json.dumps([image]).encode(), b'')
        return run

    def test_success_binds_source_metadata_and_local_image(self):
        prepared = self.prepare()
        result = build.execute(prepared, self.output, run=self.fake_runner())
        self.assertEqual(result['source_revision'], self.git('rev-parse','HEAD'))
        self.assertEqual(result['image_id'], 'sha256:'+'a'*64)
        self.assertFalse(result['identity_verified'])
        self.assertTrue((self.output/'result.json').exists())

    def test_containerd_store_binds_root_descriptor_when_config_digest_is_absent(self):
        prepared = self.prepare('web')
        original = self.fake_runner()
        def run(command, **kwargs):
            result = original(command, **kwargs)
            if command[:3] == ['docker','buildx','build']:
                (self.output/'metadata.json').write_text(json.dumps({'containerimage.digest':'sha256:'+'b'*64}))
            else:
                images = json.loads(result.stdout)
                images[0]['Id'] = 'sha256:'+'b'*64
                images[0]['Descriptor'] = {'digest':'sha256:'+'b'*64}
                result.stdout = json.dumps(images).encode()
            return result
        result = build.execute(prepared,self.output,run=run)
        self.assertEqual(result['image_id'],'sha256:'+'b'*64)

    def test_mismatched_containerd_root_descriptor_rejected(self):
        prepared = self.prepare('web')
        original = self.fake_runner()
        def run(command,**kwargs):
            result = original(command,**kwargs)
            if command[:3] == ['docker','image','inspect']:
                images = json.loads(result.stdout)
                images[0]['Descriptor'] = {'digest':'sha256:'+'c'*64}
                result.stdout=json.dumps(images).encode()
            return result
        with self.assertRaises(build.BuildFailed):
            build.execute(prepared,self.output,run=run)

    def test_failures_leave_no_success_record_or_raw_diagnostic(self):
        for index, error in enumerate(['exit','missing-metadata','wrong-label','wrong-id']):
            self.output = self.root/f'failed-{index}'
            prepared = self.prepare()
            with self.subTest(error=error), self.assertRaises(build.BuildFailed) as failure:
                build.execute(prepared, self.output, run=self.fake_runner(error))
            self.assertNotIn('private',str(failure.exception))
            self.assertFalse((self.output/'result.json').exists())

    def test_tar_corruption_rejected_before_docker_runs(self):
        prepared = self.prepare()
        with (self.output/'context.tar').open('ab') as stream:
            stream.write(b'changed')
        calls = []
        def forbidden(*args,**kwargs):
            calls.append(args)
            raise RuntimeError('must not execute')
        with self.assertRaises(build.BuildFailed):
            build.execute(prepared, self.output, run=forbidden)
        self.assertEqual(calls, [])

    def test_changed_requests_cannot_escape_fixed_routes_or_claim_verified_identity(self):
        for index, (key,value) in enumerate([('recipe','https://other.invalid/Dockerfile'),
                                           ('image','ghcr.io/other/image:release'),
                                           ('platform','windows/amd64'), ('identity_verified',True)]):
            self.output = self.root/f'changed-{index}'
            prepared = self.prepare()
            prepared[key] = value
            calls = []
            def forbidden(*args,**kwargs):
                calls.append(args)
                raise RuntimeError('must not execute')
            with self.subTest(key=key), self.assertRaises(build.BuildFailed):
                build.execute(prepared,self.output,run=forbidden)
            self.assertEqual(calls, [])

    def test_stale_metadata_prevents_another_build_in_same_directory(self):
        prepared = self.prepare()
        (self.output/'metadata.json').write_text('{}')
        calls = []
        def forbidden(*args,**kwargs):
            calls.append(args)
            raise RuntimeError('must not execute')
        with self.assertRaises(build.BuildFailed):
            build.execute(prepared,self.output,run=forbidden)
        self.assertEqual(calls, [])


if __name__=='__main__':
    unittest.main()
