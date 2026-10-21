"""Cryptographic verifier invocation and normalized image evidence boundaries."""

import copy
import io
import json
from pathlib import Path
import subprocess
import sys
import unittest
from contextlib import redirect_stderr, redirect_stdout

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import verify_component_attestations as attest
from plan_component_release import Rejected, record


DIGEST = 'sha256:' + 'a' * 64
REVISION = 'b' * 40
IMAGE = 'ghcr.io/ktjn/observable-query'
PREDICATES = ('https://slsa.dev/provenance/v1', 'https://spdx.dev/Document')


def verified(predicate):
    return [{'verificationResult': {'statement': {
        '_type': 'https://in-toto.io/Statement/v1',
        'subject': [{'name': IMAGE, 'digest': {'sha256': 'a' * 64}}],
        'predicateType': predicate, 'predicate': {'signed': 'assertion'},
    }}}]


class AttestationTests(unittest.TestCase):
    def runner(self, mutate=lambda value: value, fail_at=None):
        calls = []
        def run(command, **kwargs):
            calls.append(command)
            self.assertEqual(kwargs, {'capture_output': True, 'timeout': 30, 'stdin': subprocess.DEVNULL})
            predicate = command[command.index('--predicate-type') + 1]
            if fail_at == len(calls):
                return subprocess.CompletedProcess(command, 1, b'PRIVATE_DIAGNOSTIC', b'PRIVATE_DIAGNOSTIC')
            payload = mutate(verified(predicate))
            body = payload if isinstance(payload, bytes) else json.dumps(payload).encode()
            return subprocess.CompletedProcess(command, 0, body, b'PRIVATE_DIAGNOSTIC')
        return run, calls

    def test_both_predicates_require_fixed_cryptographic_policy_and_immutable_image(self):
        run, calls = self.runner()
        result = attest.verify('query', DIGEST, REVISION, run=run)
        for command, predicate in zip(calls, PREDICATES):
            self.assertEqual(command, [
                'gh', 'attestation', 'verify', 'oci://' + IMAGE + '@' + DIGEST,
                '--hostname', 'github.com', '--repo', 'ktjn/observable',
                '--signer-workflow', 'ktjn/observable/.github/workflows/release-artifacts.yml',
                '--source-digest', REVISION, '--signer-digest', REVISION,
                '--cert-oidc-issuer', 'https://token.actions.githubusercontent.com',
                '--deny-self-hosted-runners', '--predicate-type', predicate, '--format', 'json',
            ])
        self.assertEqual(len(calls), 2)
        self.assertEqual(result, {'component': 'query', 'image': IMAGE, 'digest': DIGEST,
                                 'source_revision': REVISION, 'sbom_subject': DIGEST,
                                 'provenance_subject': DIGEST, 'attestations_verified': True,
                                 'identity_verified': False})
        with self.assertRaises(Rejected):
            record(result)

    def test_invalid_targets_reject_before_running_tool(self):
        for args in [('other', DIGEST, REVISION), ('query', 'latest', REVISION),
                     ('query', DIGEST, 'HEAD'), ('query', DIGEST, 'b'*39),
                     ('query/../../evil', DIGEST, REVISION), ('query', DIGEST+'\n', REVISION)]:
            with self.subTest(args=args):
                run, calls = self.runner()
                with self.assertRaises(attest.VerificationFailed): attest.verify(*args, run=run)
                self.assertEqual(calls, [])

    def test_each_missing_attestation_or_tool_failure_is_fatal(self):
        for position in (1, 2):
            run, calls = self.runner(fail_at=position)
            with self.assertRaises(attest.VerificationFailed) as error:
                attest.verify('query', DIGEST, REVISION, run=run)
            self.assertNotIn('PRIVATE_DIAGNOSTIC', str(error.exception))
            self.assertEqual(len(calls), position)
        for error in (FileNotFoundError('PRIVATE_DIAGNOSTIC'),
                      subprocess.TimeoutExpired('PRIVATE_DIAGNOSTIC', 30, output=b'PRIVATE_DIAGNOSTIC')):
            def fail(*args, **kwargs): raise error
            with self.assertRaises(attest.VerificationFailed) as caught:
                attest.verify('query', DIGEST, REVISION, run=fail)
            self.assertNotIn('PRIVATE_DIAGNOSTIC', str(caught.exception))

    def test_malformed_or_absent_verification_rejected(self):
        for value in ([], {}, [None], [{'statement': {}}], b'{', b'[] trailing',
                      b'[{"verificationResult":{},"verificationResult":{}}]', b'\xff'):
            with self.subTest(value=value):
                run, _ = self.runner(lambda original: value)
                with self.assertRaises(attest.VerificationFailed):
                    attest.verify('query', DIGEST, REVISION, run=run)

    def test_wrong_or_ambiguous_signed_statements_rejected(self):
        variants = [
            ('_type', 'https://example.invalid'), ('predicateType', PREDICATES[1]),
            ('predicate', None), ('predicate', {}), ('subject', []),
            ('subject', [{'name': IMAGE, 'digest': {'sha256': 'c'*64}}]),
            ('subject', [{'name': IMAGE+'-evil', 'digest': {'sha256': 'a'*64}}]),
            ('subject', [{'name': IMAGE, 'digest': {'sha512': 'a'*128}}]),
            ('subject', [{'name': IMAGE, 'digest': {'sha256': 'a'*64}}]*2),
        ]
        for key, value in variants:
            def mutate(original):
                changed=copy.deepcopy(original)
                changed[0]['verificationResult']['statement'][key]=value
                return changed
            with self.subTest(key=key, value=value):
                run, _ = self.runner(mutate)
                with self.assertRaises(attest.VerificationFailed):
                    attest.verify('query', DIGEST, REVISION, run=run)

    def test_every_returned_statement_must_match(self):
        def mutate(original):
            other=copy.deepcopy(original[0]); other['verificationResult']['statement']['subject'][0]['name']='other'
            return original+[other]
        run, _ = self.runner(mutate)
        with self.assertRaises(attest.VerificationFailed): attest.verify('query', DIGEST, REVISION, run=run)

    def test_valid_provenance_cannot_mask_malformed_sbom(self):
        def mutate(original):
            if original[0]['verificationResult']['statement']['predicateType'] == PREDICATES[1]:
                return []
            return original
        run, calls = self.runner(mutate)
        with self.assertRaises(attest.VerificationFailed): attest.verify('query', DIGEST, REVISION, run=run)
        self.assertEqual(len(calls), 2)

    def test_oversized_output_rejected(self):
        run, _ = self.runner(lambda original: b' ' * (attest.MAX_RESULT_BYTES + 1))
        with self.assertRaises(attest.VerificationFailed): attest.verify('query', DIGEST, REVISION, run=run)

    def test_all_known_components_and_full_sha256_git_ids(self):
        for component in attest.ALL_COMPONENTS:
            image='ghcr.io/ktjn/observable-'+component
            def mutate(original):
                original[0]['verificationResult']['statement']['subject'][0]['name']=image
                return original
            with self.subTest(component=component):
                run, calls = self.runner(mutate)
                result=attest.verify(component, DIGEST, 'b'*64, run=run)
                self.assertEqual(result['component'], component)
                self.assertEqual(calls[0][3], 'oci://'+image+'@'+DIGEST)

    def test_cli_diagnostics_never_echo_tool_output(self):
        from unittest.mock import patch
        with patch.object(attest, 'verify', side_effect=attest.VerificationFailed('verification failed')):
            out, err = io.StringIO(), io.StringIO()
            with redirect_stdout(out), redirect_stderr(err):
                status = attest.main(['query', DIGEST, REVISION])
        self.assertEqual(status, 1)
        self.assertEqual(out.getvalue(), '')
        self.assertEqual(err.getvalue(), 'component attestation verification failed\n')


if __name__ == '__main__':
    unittest.main()
