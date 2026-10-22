"""Committed npm archive material bytes; no full release-identity claim."""

import base64
from concurrent.futures import ThreadPoolExecutor, wait, FIRST_COMPLETED
import hashlib
import gzip
from http.client import HTTPSConnection
import io
from pathlib import Path, PurePosixPath
import re
import stat
import tarfile
import time
from urllib.parse import unquote, urlsplit

import collect_component_inputs as inputs
from collect_component_source import GitSnapshot, canonical, digest
import inspect_component_registry as registry


ALGORITHM = 'npm-materials-v1'
SOURCE_FILES = ('package.json', 'package-lock.json', 'apps/frontend/package.json')
MAX_ARCHIVE = 64 * 1024 * 1024
MAX_EXPANDED = 512 * 1024 * 1024
MAX_TOTAL = 1024 * 1024 * 1024


class MaterialFailed(ValueError):
    """No verified material bundle or offline installation is available."""


def safe_path(value):
    if (not isinstance(value, str) or not value or value.startswith('/') or '\\' in value or ':' in value
            or any(ord(char) < 32 or ord(char) == 127 for char in value)
            or any(part in ('', '.', '..') for part in value.split('/'))
            or str(PurePosixPath(value)) != value):
        raise MaterialFailed('unsafe material path')


def archive_url(value):
    parsed = urlsplit(value)
    if (parsed.scheme != 'https' or parsed.netloc != 'registry.npmjs.org' or parsed.query or parsed.fragment
            or not parsed.path.startswith('/') or not parsed.path.endswith('.tgz')):
        raise MaterialFailed('unsupported npm archive origin')
    safe_path(unquote(parsed.path[1:]))
    return parsed


def sri_bytes(value):
    if not isinstance(value, str) or not value.startswith('sha512-'):
        raise MaterialFailed('unsupported npm integrity')
    raw = base64.b64decode(value[7:], validate=True)
    if len(raw) != 64 or base64.b64encode(raw).decode() != value[7:]:
        raise MaterialFailed('invalid npm SHA512 integrity')
    return raw


def fresh_directory(path):
    path = Path(path)
    if path.exists() or path.is_symlink():
        raise MaterialFailed('output already exists')
    path.mkdir()
    return path


def read_file(path, limit):
    path = Path(path)
    if path.is_symlink() or not stat.S_ISREG(path.stat().st_mode):
        raise MaterialFailed('unsupported material file')
    with path.open('rb') as stream:
        value = stream.read(limit + 1)
    if len(value) > limit:
        raise MaterialFailed('material file too large')
    return value


def write_file(path, data):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open('xb') as stream:
        stream.write(data)


def source_plan(root, ref):
    # Reuse the reviewed web workspace/source/version gates, resolving ref once.
    snapshot = GitSnapshot(root, ref)
    inputs.resolve(root, 'web', snapshot.revision)
    files = {name: snapshot.read_bytes(name) for name in SOURCE_FILES}
    lock = registry.parse_json(files['package-lock.json'])
    frontend = registry.parse_json(files['apps/frontend/package.json'])
    name = frontend['name']
    packages = lock['packages']
    local = {'', 'apps/frontend', 'node_modules/' + name}
    if (type(lock.get('lockfileVersion')) is not int or lock['lockfileVersion'] != 3
            or not isinstance(packages, dict) or not local <= packages.keys()
            or packages[''].get('workspaces') != ['apps/frontend']
            or packages['node_modules/' + name].get('resolved') != 'apps/frontend'
            or packages['node_modules/' + name].get('link') is not True):
        raise MaterialFailed('unsupported npm lock workspace')
    records, archives = [], {}
    for path, package in sorted(packages.items()):
        if path in local:
            if path != 'node_modules/' + name and (package.get('resolved') or package.get('integrity') or package.get('link')):
                raise MaterialFailed('unsupported local npm record')
            continue
        safe_path(path)
        if (not path.startswith(('node_modules/', 'apps/frontend/node_modules/'))
                or not isinstance(package, dict) or package.get('link')):
            raise MaterialFailed('unsupported npm package record')
        url, integrity = package['resolved'], package['integrity']
        archive_url(url)
        hashed = sri_bytes(integrity).hex()
        archive = 'archives/' + hashed + '.tgz'
        records.append({'path': path, 'resolved': url, 'integrity': integrity, 'archive': archive})
        entry = archives.setdefault(archive, {'path': archive, 'integrity': integrity, 'urls': set()})
        entry['urls'].add(url)
    if not records:
        raise MaterialFailed('empty npm material closure')
    plan = {'algorithm': ALGORITHM,
            'files': [{'path': name, 'digest': digest(data)} for name, data in sorted(files.items())],
            'packages': records,
            'archives': [entry | {'urls': sorted(entry['urls'])} for _, entry in sorted(archives.items())]}
    return snapshot.revision, files, plan


def http_get(url, *, connection_factory=HTTPSConnection):
    parsed = archive_url(url)
    deadline = time.monotonic() + 60
    connection = connection_factory('registry.npmjs.org', timeout=10)
    connection.response_class = lambda sock, *args, **kwargs: registry.DeadlineResponse(sock, deadline, *args, **kwargs)
    response = None
    try:
        connection.request('GET', parsed.path, headers={'Accept-Encoding': 'identity'})
        response = connection.getresponse()
        headers = response.getheaders()
        length = registry.one(headers, 'Content-Length')
        transfer = registry.one(headers, 'Transfer-Encoding')
        if (response.status != 200 or registry.one(headers, 'Content-Encoding', 'identity').lower() != 'identity'
                or (transfer is not None and (transfer.lower() != 'chunked' or length is not None))
                or (length is not None and (re.fullmatch(r'[0-9]+', length) is None or int(length) > MAX_ARCHIVE))):
            raise MaterialFailed('npm archive response rejected')
        body = bytearray()
        while True:
            if time.monotonic() > deadline:
                raise MaterialFailed('npm archive request timed out')
            part = response.read1(min(65536, MAX_ARCHIVE + 1 - len(body)))
            if not part:
                break
            body.extend(part)
            if len(body) > MAX_ARCHIVE:
                raise MaterialFailed('npm archive too large')
        if length is not None and len(body) != int(length):
            raise MaterialFailed('truncated npm archive')
        return bytes(body)
    finally:
        if response is not None:
            response.close()
        connection.close()


def verify_archive(data, integrity):
    if not isinstance(data, bytes) or len(data) > MAX_ARCHIVE or hashlib.sha512(data).digest() != sri_bytes(integrity):
        raise MaterialFailed('npm archive integrity mismatch')
    with gzip.GzipFile(fileobj=io.BytesIO(data)) as compressed:
        expanded = compressed.read(MAX_EXPANDED + 1)
    if len(expanded) > MAX_EXPANDED:
        raise MaterialFailed('npm archive expansion rejected')
    seen, roots, total, metadata, end = set(), set(), 0, False, 0
    with tarfile.open(fileobj=io.BytesIO(expanded), mode='r:') as archive:
        for member in archive:
            path = member.name.rstrip('/') if member.isdir() else member.name
            safe_path(path)
            parts = path.split('/')
            if path in seen or len(seen) >= 100000 or not (member.isfile() or member.isdir()):
                raise MaterialFailed('unsupported npm archive layout')
            if member.isfile() and len(parts) < 2:
                raise MaterialFailed('npm archive lacks package root')
            seen.add(path); roots.add(parts[0]); total += member.size
            if len(roots) != 1 or total > MAX_EXPANDED or member.size < 0:
                raise MaterialFailed('npm archive expansion rejected')
            metadata |= member.isfile() and len(parts) == 2 and parts[1] == 'package.json'
            end = member.offset_data + ((member.size + 511) // 512) * 512
    if not metadata or any(expanded[end:]):
        raise MaterialFailed('npm archive lacks package metadata')


def materialize(root, ref, output, *, fetch=http_get):
    try:
        revision, files, plan = source_plan(root, ref)
        output = fresh_directory(output)
        for name, data in files.items():
            write_file(output / 'sources' / name, data)
        total = 0
        verified = {}
        def download(entry):
            data = fetch(entry['urls'][0])
            verify_archive(data, entry['integrity'])
            return data
        with ThreadPoolExecutor(max_workers=4) as pool:
            entries = iter(plan['archives'])
            pending = {}
            def submit_next():
                entry = next(entries, None)
                if entry is not None:
                    pending[pool.submit(download, entry)] = entry
            for _ in range(4):
                submit_next()
            try:
                while pending:
                    done, _ = wait(pending, return_when=FIRST_COMPLETED)
                    for future in done:
                        entry = pending.pop(future)
                        data = future.result()
                        total += len(data)
                        if total > MAX_TOTAL:
                            raise MaterialFailed('npm material bundle too large')
                        write_file(output / entry['path'], data)
                        verified[entry['path']] = entry | {'size': len(data)}
                    for _ in done:
                        submit_next()
            finally:
                for future in pending:
                    future.cancel()
        payload = plan | {'archives': [verified[entry['path']] for entry in plan['archives']]}
        result = payload | {'source_revision': revision, 'material_digest': digest(canonical(payload)),
                            'material_bytes_verified': True, 'identity_verified': False}
        write_file(output / 'manifest.json', canonical(result) + b'\n')
        return result
    except Exception:
        raise MaterialFailed('npm material preparation failed') from None


def stage_bundle(root, ref, bundle, output):
    """Revalidate against approved Git metadata, then stage only verified bytes."""
    revision, files, plan = source_plan(root, ref)
    bundle = Path(bundle)
    record = registry.parse_json(read_file(bundle / 'manifest.json', 4 * 1024 * 1024))
    if (not isinstance(record, dict) or set(record) != set(plan) | {
            'source_revision', 'material_digest', 'material_bytes_verified', 'identity_verified'}
            or record['material_bytes_verified'] is not True or record['identity_verified'] is not False
            or re.fullmatch(r'(?:[0-9a-f]{40}|[0-9a-f]{64})', record['source_revision']) is None):
        raise MaterialFailed('invalid npm material manifest')
    for key in ('algorithm', 'files', 'packages'):
        if record[key] != plan[key]:
            raise MaterialFailed('npm bundle differs from committed metadata')
    archives = record['archives']
    if not isinstance(archives, list) or len(archives) != len(plan['archives']):
        raise MaterialFailed('invalid npm archive inventory')
    for entry, expected in zip(archives, plan['archives']):
        if (not isinstance(entry, dict) or set(entry) != set(expected) | {'size'}
                or any(entry[key] != expected[key] for key in expected)
                or type(entry['size']) is not int or not 0 <= entry['size'] <= MAX_ARCHIVE):
            raise MaterialFailed('invalid npm archive descriptor')
    payload = {key: record[key] for key in plan}
    if record['material_digest'] != digest(canonical(payload)):
        raise MaterialFailed('npm material digest mismatch')
    for name, data in files.items():
        if read_file(bundle / 'sources' / name, 4 * 1024 * 1024) != data:
            raise MaterialFailed('npm bundle metadata changed')
    output = fresh_directory(output)
    for name, data in files.items():
        write_file(output / 'workspace' / name, data)
    total = 0
    for entry in archives:
        data = read_file(bundle / entry['path'], MAX_ARCHIVE)
        verify_archive(data, entry['integrity']); total += len(data)
        if len(data) != entry['size'] or total > MAX_TOTAL:
            raise MaterialFailed('npm archive size mismatch')
        write_file(output / entry['path'], data)
    return revision, files, record
