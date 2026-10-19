"""Source evidence is read from immutable Git objects, never the checkout."""

import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
import collect_component_source as source


class SourceTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.git('init', '-q')
        self.git('config', 'user.name', 'Source Test')
        self.git('config', 'user.email', 'source-test@example.invalid')
        self.git('config', 'core.autocrlf', 'false')
        for name in ['Dockerfile', 'apps/frontend/Dockerfile', '.dockerignore']:
            self.write(name, (ROOT / name).read_text(encoding='utf-8'))
        self.write('Cargo.toml', '''[workspace]
members = ["services/query-api", "services/auth-service", "libs/shared", "libs/dev"]
''')
        self.write('Cargo.lock', '# lock\n')
        self.write('package.json', '{"workspaces":["apps/frontend"]}')
        self.write('package-lock.json', '{}')
        for member, name in [('services/query-api', 'query-api'),
                             ('services/auth-service', 'auth-service'),
                             ('libs/shared', 'shared'), ('libs/dev', 'dev')]:
            extra = '\n[dependencies]\nshared = { path = "../../libs/shared" }\n' if member.startswith('services/') else ''
            if name == 'query-api':
                extra += '\n[dev-dependencies]\ndev = { path = "../../libs/dev" }\n'
            self.write(member + '/Cargo.toml', f'[package]\nname = "{name}"\nversion = "0.1.0"\n' + extra)
            self.write(member + '/src/lib.rs', '// source\n')
        self.write('apps/frontend/package.json', '{"name":"web"}')
        self.write('apps/frontend/src/main.ts', '// web\n')
        self.write('proto/otlp/test.proto', '// proto\n')
        self.commit()

    def git(self, *args):
        return subprocess.check_output(['git', *args], cwd=self.root, stderr=subprocess.PIPE).decode().strip()

    def write(self, name, text):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding='utf-8', newline='\n')

    def commit(self):
        self.git('add', '.')
        self.git('-c', 'commit.gpgsign=false', 'commit', '-qm', 'fixture')
        return self.git('rev-parse', 'HEAD')

    def collect(self, component='query', ref='HEAD'):
        return source.collect(self.root, component, ref)

    def test_docs_commit_and_dirty_checkout_do_not_change_source_identity(self):
        before = self.collect()
        self.write('README.md', 'documentation')
        self.commit()
        self.write('libs/shared/src/lib.rs', 'dirty content')
        after = self.collect()
        self.assertEqual(before['source_digest'], after['source_digest'])
        self.assertNotEqual(before['source_revision'], after['source_revision'])
        self.assertFalse(after['identity_verified'])

    def test_transitive_source_changes_but_unrelated_and_dev_source_do_not(self):
        previous = self.collect()['source_digest']
        for name in ['services/auth-service/src/lib.rs', 'libs/dev/src/lib.rs']:
            self.write(name, '// changed\n')
            self.commit()
            self.assertEqual(previous, self.collect()['source_digest'])
        self.write('libs/shared/src/lib.rs', '// changed\n')
        self.commit()
        self.assertNotEqual(previous, self.collect()['source_digest'])

    def test_all_workspace_manifests_and_lock_and_proto_are_inputs(self):
        previous = self.collect()['source_digest']
        for name in ['services/auth-service/Cargo.toml', 'Cargo.lock', 'proto/otlp/test.proto']:
            with self.subTest(name=name):
                path = self.root / name
                self.write(name, path.read_text() + '\n# changed\n')
                self.commit()
                current = self.collect()['source_digest']
                self.assertNotEqual(previous, current)
                previous = current

    def test_web_uses_root_context_and_not_rust_sources(self):
        previous = self.collect('web')['source_digest']
        self.write('libs/shared/src/lib.rs', '// unrelated\n')
        self.commit()
        self.assertEqual(previous, self.collect('web')['source_digest'])
        for name in ['.dockerignore', 'package-lock.json', 'apps/frontend/src/main.ts']:
            self.write(name, (self.root / name).read_text() + '\n')
            self.commit()
            current = self.collect('web')['source_digest']
            self.assertNotEqual(previous, current)
            previous = current

    def test_historical_revision_is_independent_of_current_layout(self):
        old = self.git('rev-parse', 'HEAD')
        before = self.collect(ref=old)
        self.write('Cargo.toml', 'invalid TOML')
        self.commit()
        self.assertEqual(before, self.collect(ref=old))
        with self.assertRaises(source.InvalidSource):
            self.collect()

    def test_renames_deletions_and_modes_change_identity(self):
        previous = self.collect()['source_digest']
        self.git('mv', 'libs/shared/src/lib.rs', 'libs/shared/src/renamed.rs')
        self.commit()
        renamed = self.collect()['source_digest']
        self.assertNotEqual(previous, renamed)
        self.git('update-index', '--chmod=+x', 'libs/shared/src/renamed.rs')
        self.git('-c', 'commit.gpgsign=false', 'commit', '-qm', 'mode')
        executable = self.collect()['source_digest']
        self.assertNotEqual(renamed, executable)
        self.git('rm', 'libs/shared/src/renamed.rs')
        self.commit()
        self.assertNotEqual(executable, self.collect()['source_digest'])

    def test_changed_recipe_requires_selection_review(self):
        for component, name in [('query', 'Dockerfile'), ('web', 'apps/frontend/Dockerfile')]:
            self.write(name, (self.root / name).read_text() + '\nCOPY extra /extra\n')
            self.commit()
            with self.assertRaisesRegex(source.InvalidSource, 'recipe'):
                self.collect(component)

    def test_unknown_local_dependency_and_duplicate_package_fail_closed(self):
        path = 'services/query-api/Cargo.toml'
        original = (self.root / path).read_text()
        self.write(path, original.replace('../../libs/shared', '../../outside'))
        self.commit()
        with self.assertRaises(source.InvalidSource):
            self.collect()
        self.write(path, original.replace('name = "query-api"', 'name = "shared"'))
        self.commit()
        with self.assertRaises(source.InvalidSource):
            self.collect()

    def test_new_build_scripts_and_web_workspaces_require_review(self):
        self.write('libs/shared/build.rs', 'fn main() {}')
        self.commit()
        with self.assertRaises(source.InvalidSource):
            self.collect()
        self.write('package.json', '{"workspaces":["apps/frontend","apps/extra"]}')
        self.commit()
        with self.assertRaises(source.InvalidSource):
            self.collect('web')

    def test_target_build_dependency_is_included(self):
        path = 'services/query-api/Cargo.toml'
        original = (self.root / path).read_text()
        self.write(path, original + '\n[target.\'cfg(unix)\'.build-dependencies]\ndev = { path = "../../libs/dev" }\n')
        self.commit()
        before = self.collect()['source_digest']
        self.write('libs/dev/src/lib.rs', '// build dependency changed\n')
        self.commit()
        self.assertNotEqual(before, self.collect()['source_digest'])

    def test_symlinks_and_submodules_are_rejected_without_following_them(self):
        for mode in ['120000', '160000']:
            oid = self.git('rev-parse', 'HEAD')
            if mode == '120000':
                oid = subprocess.check_output(['git', 'hash-object', '-w', '--stdin'],
                                              input=b'../../outside', cwd=self.root).decode().strip()
            self.git('update-index', '--add', '--cacheinfo', mode, oid, 'libs/shared/unsupported')
            self.git('-c', 'commit.gpgsign=false', 'commit', '-qm', 'unsupported entry')
            with self.assertRaises(source.InvalidSource):
                self.collect()

    def test_unknown_layout_rejected(self):
        self.write('libs/unknown/src/lib.rs', '// unowned\n')
        self.commit()
        with self.assertRaises(source.InvalidSource):
            self.collect()

    def test_existing_agent_notes_are_not_unknown_crate_source(self):
        before = self.collect()['source_digest']
        self.write('services/.junie/memory/README.md', 'agent notes')
        self.commit()
        self.assertEqual(before, self.collect()['source_digest'])

    def test_missing_required_input_rejected(self):
        self.git('rm', 'Cargo.lock')
        self.commit()
        with self.assertRaises(source.InvalidSource):
            self.collect()

    def test_invalid_revision_and_component_fail_without_raw_diagnostic(self):
        for component, ref in [('contracts', 'HEAD'), ('query', '--help'), ('query', 'not-a-ref')]:
            with self.assertRaises(source.InvalidSource) as error:
                self.collect(component, ref)
            self.assertNotIn('fatal:', str(error.exception))

    def test_custom_build_script_paths_require_review(self):
        name = 'services/query-api/Cargo.toml'
        self.write(name, (self.root / name).read_text().replace('[package]',
                   '[package]\nbuild = "../../libs/dev/custom-build.rs"'))
        self.write('libs/dev/custom-build.rs', 'fn main() {}')
        self.commit()
        with self.assertRaises(source.InvalidSource):
            self.collect()

    def test_malformed_manifest_types_reject_without_cli_tracebacks(self):
        self.write('Cargo.toml', '[workspace]\nmembers = [123]\n')
        self.write('package.json', '[]')
        self.commit()
        for component in ['query', 'web']:
            with self.subTest(component=component):
                with self.assertRaises(source.InvalidSource):
                    self.collect(component)
                result = subprocess.run([sys.executable, str(ROOT/'scripts/collect_component_source.py'),
                                         component, '--repo', str(self.root)], capture_output=True, text=True)
                self.assertEqual(result.returncode, 1)
                self.assertEqual(result.stdout, '')
                self.assertNotIn('Traceback', result.stderr)

    def test_records_use_content_sha256_and_are_sorted(self):
        result = self.collect()
        self.assertEqual(result['source_algorithm'], 'component-source-v1')
        records = result['files']
        self.assertEqual([r['path'] for r in records], sorted(r['path'] for r in records))
        self.assertTrue(all(len(r['digest']) == 71 for r in records))
        self.assertEqual(result, self.collect())

    def test_cli_is_json_and_does_not_authorize_publication(self):
        result = subprocess.run([sys.executable, str(ROOT/'scripts/collect_component_source.py'),
                                 'query', '--repo', str(self.root)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(json.loads(result.stdout)['identity_verified'])


if __name__ == '__main__':
    unittest.main()
