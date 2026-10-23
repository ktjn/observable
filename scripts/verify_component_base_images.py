"""Verify committed recipe image pins against read-only Docker Hub manifests.

Usage: python scripts/verify_component_base_images.py COMPONENT [--ref HEAD]
       [--repo PATH] [--platform linux/amd64]... [--build-arg NAME=VALUE]...

Anonymous pull tokens stay in memory. No config/layer downloads, credential
discovery, redirects, writes, publisher-signature or complete-identity claim.
"""

import argparse
from http.client import HTTPSConnection
import json
from pathlib import Path
import re
import sys
from urllib.parse import urlencode, urlsplit

import collect_component_inputs as inputs
from ci_changed_components import ALL_COMPONENTS
from collect_component_source import ROOT, digest
import inspect_component_registry as registry
from plan_component_release import DIGEST, matches


REPOSITORIES = {'debian': 'library/debian', 'node': 'library/node', 'nginx': 'library/nginx',
                'docker/dockerfile': 'docker/dockerfile',
                'lukemathwalker/cargo-chef': 'lukemathwalker/cargo-chef'}
REGISTRY = 'registry-1.docker.io'
AUTHORITY = 'auth.docker.io'
MANIFEST_TYPES = tuple(media for media in registry.MEDIA_TYPES if media not in registry.INDEX_TYPES)


class VerificationFailed(ValueError):
    """No safe registry-bound recipe image evidence is available."""


def token_url(repository):
    if repository not in REPOSITORIES.values():
        raise VerificationFailed('unsupported image repository')
    return 'https://' + AUTHORITY + '/token?' + urlencode({
        'service': 'registry.docker.io', 'scope': f'repository:{repository}:pull'})


def http_get(url, headers, *, connection_factory=HTTPSConnection):
    parsed = urlsplit(url)
    allowed_auth = url in {token_url(repo) for repo in REPOSITORIES.values()}
    allowed_manifest = (parsed.netloc == REGISTRY and not parsed.query
                        and any(re.fullmatch(r'/v2/' + re.escape(repo) + r'/manifests/' + DIGEST,
                                             parsed.path) for repo in REPOSITORIES.values()))
    if (parsed.scheme != 'https' or parsed.fragment or not (allowed_auth or allowed_manifest)):
        raise VerificationFailed('unsupported image registry origin')
    if allowed_auth and any(key.lower() == 'authorization' for key in headers):
        raise VerificationFailed('registry token must not be sent to token authority')
    target = parsed.path + ('?' + parsed.query if parsed.query else '')
    return registry.bounded_https_get(parsed.netloc, target, headers, connection_factory=connection_factory)


def response(fetch, url, headers):
    status, response_headers, body = fetch(url, headers)
    if type(status) is not int or status != 200 or not isinstance(body, bytes) or len(body) > registry.MAX_BYTES:
        raise VerificationFailed('registry image evidence unavailable')
    expected = registry.framing(response_headers)
    if expected is not None and expected != len(body):
        raise VerificationFailed('inconsistent registry response length')
    media = registry.one(response_headers, 'Content-Type', '').split(';', 1)[0].strip().lower()
    return body, response_headers, media


def pull_token(repository, fetch):
    body, _, media = response(fetch, token_url(repository), {'Accept': 'application/json', 'Accept-Encoding': 'identity'})
    if media != 'application/json':
        raise VerificationFailed('invalid token response type')
    value = registry.parse_json(body)
    token = value.get('token', value.get('access_token'))
    if (not matches(r'[A-Za-z0-9._~+/-]+=*', token) or len(token) > 16384
            or ('token' in value and 'access_token' in value and value['token'] != value['access_token'])
            or ('expires_in' in value and (type(value['expires_in']) is not int or value['expires_in'] <= 0))):
        raise VerificationFailed('invalid pull token')
    return token


def pinned_repository(value):
    reference = value['reference']
    name, separator, pinned = reference.partition('@')
    repository, colon, tag = name.partition(':')
    if (repository not in REPOSITORIES or not separator or not matches(DIGEST, pinned)
            or value['digest'] != pinned
            or (colon and re.fullmatch(r'[A-Za-z0-9_][A-Za-z0-9_.-]{0,127}', tag) is None)):
        raise VerificationFailed('unsupported pinned image reference')
    return REPOSITORIES[repository], pinned


def image_manifest(repository, pinned, token, fetch):
    url = f'https://{REGISTRY}/v2/{repository}/manifests/{pinned}'
    body, headers, media = response(fetch, url, {'Accept': ', '.join(registry.MEDIA_TYPES),
                                                'Accept-Encoding': 'identity', 'Authorization': 'Bearer ' + token})
    if (media not in registry.MEDIA_TYPES or digest(body) != pinned
            or registry.one(headers, 'Docker-Content-Digest') != pinned):
        raise VerificationFailed('registry image digest or media mismatch')
    value = registry.parse_json(body)
    registry.manifest(value, media)
    return value, media, len(body)


def compatible(value, platform):
    if not isinstance(value, dict):
        return False
    os, arch = platform.split('/')
    variants = ('', 'v1') if arch == 'amd64' else ('', 'v8')
    return (value.get('os') == os and value.get('architecture') == arch
            and value.get('variant', '') in variants and value.get('os.features', []) == []
            and value.get('features', []) == [] and value.get('os.version', '') == '')


def verify(root, component, ref='HEAD', targets=('linux/amd64',), *, build_args=None, fetch=http_get):
    try:
        declared = inputs.resolve(root, component, ref, targets, build_args=build_args)
        references = [(image, 'base') for image in declared['images']]
        references.append((declared['recipe']['frontend'], 'frontend'))
        # Validate the complete reference set before requesting any token.
        approved = [(value, role, *pinned_repository(value)) for value, role in references]
        verified = []
        for value, role, repository, pinned in approved:
            token = pull_token(repository, fetch)
            index, media, _ = image_manifest(repository, pinned, token, fetch)
            if media not in registry.INDEX_TYPES:
                raise VerificationFailed('recipe pin must identify a platform index')
            children = []
            for platform in declared['platforms']:
                candidates = [item for item in index['manifests'] if compatible(item.get('platform'), platform)]
                if len(candidates) != 1 or candidates[0]['mediaType'] not in MANIFEST_TYPES:
                    raise VerificationFailed('missing or ambiguous platform manifest')
                selected = candidates[0]
                _, child_media, child_size = image_manifest(repository, selected['digest'], token, fetch)
                if child_media != selected['mediaType'] or child_size != selected['size']:
                    raise VerificationFailed('platform manifest differs from index descriptor')
                children.append({'platform': platform, 'digest': selected['digest'],
                                 'media_type': child_media, 'size': child_size})
            verified.append({'reference': value['reference'], 'digest': pinned, 'role': role,
                             'manifests': children})
        return {key: declared[key] for key in ('component', 'source_revision', 'input_algorithm', 'input_digest', 'platforms')} | {
            'images': verified, 'registry_images_verified': True,
            'identity_verified': False, 'publication_authorized': False}
    except Exception:
        # Token responses, headers and upstream errors must never reach diagnostics.
        raise VerificationFailed('component base-image verification failed') from None


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('component', choices=ALL_COMPONENTS)
    parser.add_argument('--repo', type=Path, default=ROOT)
    parser.add_argument('--ref', default='HEAD')
    parser.add_argument('--platform', action='append', dest='targets')
    parser.add_argument('--build-arg', action='append', default=[], dest='build_args')
    args = parser.parse_args(argv)
    try:
        result = verify(args.repo, args.component, args.ref, args.targets or ['linux/amd64'],
                        build_args=inputs.parse_build_args(args.build_args))
    except (VerificationFailed, inputs.InvalidInputs):
        print('component base-image verification failed', file=sys.stderr)
        return 1
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
