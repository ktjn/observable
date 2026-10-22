"""Declared build-input identity comes from committed, digest-pinned recipes."""

import hashlib
import json
from pathlib import Path
import subprocess
import sys
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
import collect_component_inputs as inputs
import collect_component_source as source
import test_component_source as fixtures


DIGEST = 'sha256:' + 'a' * 64


class InputIdentityTests(unittest.TestCase):
    git = fixtures.SourceTests.git
    write = fixtures.SourceTests.write
    commit = fixtures.SourceTests.commit

    def setUp(self):
        fixtures.SourceTests.setUp(self)
        self.write('distribution/manifest.yaml',
                   'productVersion: 0.1.0\ncomponents:\n  query: 0.1.0\n  web: 0.1.0\n  auth: 0.1.0\n')
        self.write('apps/frontend/package.json', '{"name":"web","version":"0.1.0"}')
        self.commit()

    def resolve(self, component='query', targets=('linux/amd64',), ref='HEAD', **kwargs):
        return inputs.resolve(self.root, component, ref, targets, **kwargs)

    def review_recipe(self, name, text):
        """Simulate a reviewed recipe change so only the pin rules can reject it."""
        self.write(name, text)
        self.commit()
        previous = source.RECIPES[name]
        source.RECIPES[name] = hashlib.sha256(text.encode()).hexdigest()
        self.addCleanup(source.RECIPES.__setitem__, name, previous)

    def references(self, result):
        return {image['reference'].split('@')[0] for image in result['images']}

    def test_identity_is_deterministic_and_never_authorizing(self):
        first, second = self.resolve(), self.resolve()
        self.assertEqual(first, second)
        self.assertEqual(first['input_algorithm'], 'component-input-v2')
        self.assertTrue(first['input_digest'].startswith('sha256:'))
        self.assertEqual(first['version'], '0.1.0')
        self.assertFalse(first['identity_verified'])
        self.assertEqual(first['recipe']['target'], 'component-runtime')

    def test_effective_defaults_and_explicit_defaults_have_identical_identity(self):
        self.assertEqual(self.resolve()['build_args'], {'SERVICE': 'query-api'})
        defaults = {'VITE_OBSERVABLE_SELF_OBSERVABILITY_MODE': 'self',
                    'VITE_OBSERVABLE_SELF_OBSERVABILITY_OTLP_ENDPOINT': ''}
        self.assertEqual(self.resolve('web')['build_args'], defaults)
        self.assertEqual(self.resolve('web'), self.resolve('web', build_args=defaults))

    def test_supported_overrides_change_inputs_but_not_source_and_order_is_irrelevant(self):
        args = {'VITE_OBSERVABLE_SELF_OBSERVABILITY_MODE': 'observer_instance',
                'VITE_OBSERVABLE_SELF_OBSERVABILITY_OTLP_ENDPOINT': 'https://observer.example/otlp?x=a=b'}
        normal=self.resolve('web')
        changed=self.resolve('web', build_args=args)
        self.assertNotEqual(normal['input_digest'], changed['input_digest'])
        self.assertEqual(normal['source_digest'], changed['source_digest'])
        self.assertEqual(changed, self.resolve('web', build_args=dict(reversed(list(args.items())))))
        self.assertEqual(changed['build_args'], args)

    def test_invalid_arguments_are_rejected_without_echoing_values(self):
        for component, args in [('query', {'SERVICE': 'auth-service'}),
                                ('query', {'SERVICE': 'query-api'}),
                                ('web', {'UNKNOWN': 'PRIVATE_SENTINEL'}),
                                ('web', {'VITE_OBSERVABLE_SELF_OBSERVABILITY_MODE': 'PRIVATE_SENTINEL'}),
                                ('web', {'VITE_OBSERVABLE_SELF_OBSERVABILITY_OTLP_ENDPOINT': 'https://user:PRIVATE_SENTINEL@example.com'}),
                                ('web', {'VITE_OBSERVABLE_SELF_OBSERVABILITY_OTLP_ENDPOINT': 'PRIVATE_SENTINEL\n'}),
                                ('web', {'VITE_OBSERVABLE_SELF_OBSERVABILITY_OTLP_ENDPOINT': 7})]:
            with self.subTest(component=component, args=args), self.assertRaises(inputs.InvalidInputs) as caught:
                self.resolve(component, build_args=args)
            self.assertNotIn('PRIVATE_SENTINEL', str(caught.exception))

    def test_unknown_reachable_argument_or_changed_default_fails_closed(self):
        for index, suffix in enumerate(['ARG UNREVIEWED=value\n',
                                        'ARG\tUNREVIEWED=value\n',
                                        'ARG VITE_OBSERVABLE_SELF_OBSERVABILITY_MODE=observer_instance\n']):
            text=(ROOT/'apps/frontend/Dockerfile').read_text()+suffix
            self.review_recipe('apps/frontend/Dockerfile', text)
            with self.subTest(index=index), self.assertRaises(inputs.InvalidInputs):
                self.resolve('web')

    def test_cli_argument_parser_preserves_equals_and_rejects_duplicates(self):
        self.assertEqual(inputs.parse_build_args(['VITE_OBSERVABLE_SELF_OBSERVABILITY_OTLP_ENDPOINT=/otlp?x=a=b']),
                         {'VITE_OBSERVABLE_SELF_OBSERVABILITY_OTLP_ENDPOINT':'/otlp?x=a=b'})
        for values in [['NAME'], ['NAME=a', 'NAME=b'], ['=value']]:
            with self.subTest(values=values), self.assertRaises(inputs.InvalidInputs):
                inputs.parse_build_args(values)

    def test_recipe_evidence_cannot_bypass_reviewed_recipe_guard(self):
        recipe=(self.root/'apps/frontend/Dockerfile').read_bytes()+b'\nRUN echo changed\n'
        with self.assertRaises(inputs.InvalidInputs):
            inputs.recipe_evidence('web',['linux/amd64'],self.resolve('web'),
                                   'apps/frontend/Dockerfile',recipe)

    def test_only_reachable_stage_images_are_inputs(self):
        self.assertEqual(self.references(self.resolve('query')),
                         {'lukemathwalker/cargo-chef:0.1.77-rust-1.97.0-bookworm',
                          'debian:bookworm-slim'})
        self.assertEqual(self.references(self.resolve('web')), {'node:24-alpine', 'nginx:1.31-alpine'})
        self.assertEqual(self.resolve('web')['recipe']['target'], None)

    def test_unreachable_stages_do_not_contribute_images_or_materials(self):
        rust = self.resolve('query')
        by_reference = {image['reference'].split('@')[0]: image for image in rust['images']}
        self.assertEqual(by_reference['debian:bookworm-slim']['stages'], ['component-runtime'])
        self.assertEqual(by_reference['lukemathwalker/cargo-chef:0.1.77-rust-1.97.0-bookworm']['stages'],
                         ['chef'])
        web = self.resolve('web')
        self.assertEqual({image['reference'].split('@')[0]: image['stages'] for image in web['images']},
                         {'node:24-alpine': ['deps'], 'nginx:1.31-alpine': ['runtime']})

    def test_frontend_directive_is_a_recorded_input(self):
        for result in (self.resolve('query'), self.resolve('web')):
            with self.subTest(component=result['component']):
                self.assertEqual(result['recipe']['frontend']['reference'].split('@')[0],
                                 'docker/dockerfile:1.28')

    def test_unresolved_build_time_materials_are_reported(self):
        materials = self.resolve('query')['unresolved_materials']
        self.assertIn('ca-certificates', materials['apt'])
        self.assertIn('build-essential', materials['apt'])
        self.assertIn('curl', materials['apt'])
        self.assertNotIn('update', materials['apt'])
        self.assertEqual(materials['rustup'], ['clippy', 'rustfmt'])
        self.assertEqual(materials['downloads'], [])
        self.assertEqual(self.resolve('web')['unresolved_materials'],
                         {'apt': [], 'rustup': [], 'downloads': []})

    def test_input_digest_tracks_source_platform_and_pinned_digest(self):
        previous = self.resolve()['input_digest']
        self.write('libs/shared/src/lib.rs', '// changed\n')
        self.commit()
        changed_source = self.resolve()['input_digest']
        self.assertNotEqual(previous, changed_source)
        self.assertNotEqual(changed_source, self.resolve(targets=('linux/arm64',))['input_digest'])
        replaced = (self.root / 'Dockerfile').read_text().replace(
            'debian:bookworm-slim@sha256:7ff6ba31527b9e336162fa061746c366c4b2032f4d2cff2078bea43f78efe3f8',
            'debian:bookworm-slim@' + DIGEST)
        self.review_recipe('Dockerfile', replaced)
        self.assertNotEqual(changed_source, self.resolve()['input_digest'])

    def test_platforms_are_validated_and_deduplicated(self):
        for targets in [(), ('linux/amd64', 'linux/amd64'), ('windows/amd64',), ('linux/riscv64',),
                        ('linux/amd64', 7), 'linux/amd64']:
            with self.subTest(targets=targets), self.assertRaises(inputs.InvalidInputs):
                self.resolve(targets=targets)
        self.assertEqual(self.resolve(targets=('linux/arm64', 'linux/amd64'))['platforms'],
                         ['linux/amd64', 'linux/arm64'])

    def test_mutable_tag_recipe_fails_closed_end_to_end(self):
        text = (self.root / 'Dockerfile').read_text().replace(
            'debian:bookworm-slim@sha256:7ff6ba31527b9e336162fa061746c366c4b2032f4d2cff2078bea43f78efe3f8',
            'debian:bookworm-slim')
        self.review_recipe('Dockerfile', text)
        with self.assertRaises(inputs.InvalidInputs):
            self.resolve()

    def test_unreviewed_recipe_change_fails_closed(self):
        self.write('Dockerfile', (self.root / 'Dockerfile').read_text() + '\nCOPY extra /extra\n')
        self.commit()
        with self.assertRaises(inputs.InvalidInputs):
            self.resolve()

    def test_unknown_component_reference_and_layout_fail_without_diagnostics(self):
        for component, ref in [('contracts', 'HEAD'), ('query', '--help'), ('query', 'not-a-ref')]:
            with self.subTest(component=component, ref=ref):
                with self.assertRaises(inputs.InvalidInputs) as error:
                    self.resolve(component, ref=ref)
                self.assertNotIn('fatal:', str(error.exception))

    def test_parse_recipe_requires_digest_pinned_images(self):
        with self.assertRaises(inputs.InvalidInputs):
            inputs.parse_recipe('FROM debian:bookworm-slim\n', 'component-runtime')
        images, materials, frontend = inputs.parse_recipe(f'FROM debian@sha256:{"b" * 64}\n', None)
        self.assertEqual(images[f'debian@sha256:{"b" * 64}'], {'digest': 'sha256:' + 'b' * 64,
                                                              'stages': ['0']})
        self.assertEqual(materials, {'apt': [], 'rustup': [], 'downloads': []})
        self.assertIsNone(frontend)

    def test_external_copy_source_is_a_pinned_input(self):
        source = f'other/thing@sha256:{"c" * 64}'
        images, _, _ = inputs.parse_recipe(
            f'FROM debian@sha256:{"b" * 64} AS one\nCOPY --from={source} /a /b\n', 'one')
        self.assertEqual(images[source]['stages'], ['one'])

    def test_comment_lines_inside_a_continuation_are_not_materials(self):
        text = (f'FROM debian@sha256:{"b" * 64} AS one\n'
                'RUN apt-get install -y curl \\\n'
                '    # apt-get install secret-package https://secret.invalid/x\n'
                '    && true\n')
        _, materials, _ = inputs.parse_recipe(text, 'one')
        self.assertEqual(materials['apt'], ['curl'])
        self.assertEqual(materials['downloads'], [])

    def test_parse_recipe_rejects_unreviewed_layout(self):
        pinned = f'debian@sha256:{"b" * 64}'
        invalid = [f'FROM --platform=linux/amd64 {pinned}\n',
                   f'FROM {pinned} AS bad$name\n',
                   f'FROM {pinned} AS one\nFROM {pinned} extra\n',
                   f'FROM {pinned} AS one\nCOPY --from=7 /a /b\n',
                   f'FROM {pinned} AS one\nCOPY --from=2 /a /b\n',
                   f'FROM {pinned} AS one\nCOPY --from=two /a /b\nFROM {pinned} AS two\n',
                   f'ARG X=1\nFROM {pinned}\n',
                   '# syntax=docker/dockerfile:1.28\n' + f'FROM {pinned}\n',
                   '# syntax=docker/dockerfile:1.28@sha256:short\n' + f'FROM {pinned}\n',
                   'COPY x y\n']
        for text in invalid:
            with self.subTest(text=text), self.assertRaises(inputs.InvalidInputs):
                inputs.parse_recipe(text, '0')
        with self.assertRaises(inputs.InvalidInputs):
            inputs.parse_recipe(f'FROM {pinned}\n', 'missing-stage')

    def test_syntax_directive_outside_the_header_is_ignored(self):
        pinned = f'debian@sha256:{"b" * 64}'
        _, _, frontend = inputs.parse_recipe(f'FROM {pinned}\n# syntax=docker/dockerfile:1.28\n',
                                             '0')
        self.assertIsNone(frontend)

    def test_cli_is_json_or_sanitized_failure(self):
        ok = subprocess.run([sys.executable, str(ROOT / 'scripts/collect_component_inputs.py'),
                             'query', '--repo', str(self.root), '--platform', 'linux/amd64'],
                            capture_output=True, text=True)
        self.assertEqual(ok.returncode, 0, ok.stderr)
        self.assertFalse(json.loads(ok.stdout)['identity_verified'])
        self.write('Dockerfile', (self.root / 'Dockerfile').read_text() + 'COPY extra /extra\n')
        self.commit()
        bad = subprocess.run([sys.executable, str(ROOT / 'scripts/collect_component_inputs.py'),
                              'query', '--repo', str(self.root)], capture_output=True, text=True)
        self.assertEqual(bad.returncode, 1)
        self.assertEqual(bad.stdout, '')
        self.assertNotIn('Traceback', bad.stderr)


if __name__ == '__main__':
    unittest.main()
