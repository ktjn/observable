"""Authenticate component image attestations; never authorize release identity.

Usage: python scripts/verify_component_attestations.py COMPONENT SHA256_DIGEST FULL_SOURCE_COMMIT

Requires trusted installed gh and existing GitHub/GHCR authentication. Performs
read-only verification; no login, signing, upload, raw JSON import, or publishing.
"""

import json
import subprocess
import sys

from ci_changed_components import ALL_COMPONENTS
from plan_component_release import DIGEST, matches, unique_object


REPOSITORY = 'ktjn/observable'
WORKFLOW = REPOSITORY + '/.github/workflows/release-artifacts.yml'
PREDICATES = {'provenance': 'https://slsa.dev/provenance/v1', 'sbom': 'https://spdx.dev/Document'}
MAX_RESULT_BYTES = 16 * 1024 * 1024


class VerificationFailed(ValueError):
    """No authenticated component attestation evidence is available."""


def verify(component, image_digest, source_revision, *, run=subprocess.run):
    """Verify both predicates using fixed policy and return normalized evidence.

    The caller must select an approved expected commit. The trusted local gh
    process enforces signature/certificate policy; caller-supplied JSON is never
    accepted as verification. This does not validate complete build inputs.
    """
    try:
        return _verify(component, image_digest, source_revision, run)
    except Exception:
        # CLI/network/authentication diagnostics can contain sensitive data.
        raise VerificationFailed('component attestation verification failed') from None


def _verify(component, image_digest, source_revision, run):
    if (component not in ALL_COMPONENTS or not matches(DIGEST, image_digest)
            or not matches(r'(?:[0-9a-f]{40}|[0-9a-f]{64})', source_revision)):
        raise VerificationFailed('invalid component attestation target')
    image = f'ghcr.io/ktjn/observable-{component}'
    subjects = {}
    for kind, predicate in PREDICATES.items():
        command = [
            'gh', 'attestation', 'verify', f'oci://{image}@{image_digest}',
            '--hostname', 'github.com', '--repo', REPOSITORY,
            '--signer-workflow', WORKFLOW,
            '--source-digest', source_revision, '--signer-digest', source_revision,
            '--cert-oidc-issuer', 'https://token.actions.githubusercontent.com',
            '--deny-self-hosted-runners', '--predicate-type', predicate, '--format', 'json',
        ]
        checked = run(command, capture_output=True, timeout=30, stdin=subprocess.DEVNULL)
        if (checked.returncode != 0 or not isinstance(checked.stdout, bytes)
                or len(checked.stdout) > MAX_RESULT_BYTES):
            raise VerificationFailed('attestation verifier failed')
        verified = json.loads(checked.stdout.decode('utf-8'), object_pairs_hook=unique_object)
        if not isinstance(verified, list) or not verified:
            raise VerificationFailed('no verified attestations')
        for item in verified:
            statement = item['verificationResult']['statement']
            if (statement['_type'] != 'https://in-toto.io/Statement/v1'
                    or statement['predicateType'] != predicate
                    or not isinstance(statement['predicate'], dict) or not statement['predicate']
                    or statement['subject'] != [{'name': image, 'digest': {'sha256': image_digest[7:]}}]):
                raise VerificationFailed('verified attestation targets different evidence')
        subjects[kind + '_subject'] = image_digest
    return {'component': component, 'image': image, 'digest': image_digest,
            'source_revision': source_revision, **subjects,
            'attestations_verified': True, 'identity_verified': False}


def main(argv):
    if argv == ['--help']:
        print(__doc__)
        return 0
    if len(argv) != 3:
        print('usage: verify_component_attestations.py COMPONENT SHA256_DIGEST FULL_SOURCE_COMMIT', file=sys.stderr)
        return 1
    try:
        result = verify(*argv)
    except VerificationFailed:
        print('component attestation verification failed', file=sys.stderr)
        return 1
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
