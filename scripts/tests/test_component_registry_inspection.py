"""Read-only OCI inspection through a real local HTTP fixture."""

from collections import deque
from contextlib import redirect_stderr, redirect_stdout
import hashlib
from http.client import HTTPConnection
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from io import StringIO
from pathlib import Path
import subprocess
import sys
import threading
import time
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
import inspect_component_registry as registry
from plan_component_release import plan_release, Rejected


OCI_MANIFEST = 'application/vnd.oci.image.manifest.v1+json'
OCI_INDEX = 'application/vnd.oci.image.index.v1+json'
DOCKER_MANIFEST = 'application/vnd.docker.distribution.manifest.v2+json'
DOCKER_INDEX = 'application/vnd.docker.distribution.manifest.list.v2+json'


def descriptor(media_type):
    return dict(mediaType=media_type, digest='sha256:'+'a'*64, size=12)


def document(media_type=OCI_MANIFEST):
    if media_type in (OCI_INDEX, DOCKER_INDEX):
        value = dict(schemaVersion=2, mediaType=media_type,
                     manifests=[descriptor(OCI_MANIFEST)])
    else:
        value = dict(schemaVersion=2, mediaType=media_type,
                     config=descriptor('application/vnd.oci.image.config.v1+json'), layers=[])
    return json.dumps(value).encode()


def success(body=None, media_type=OCI_MANIFEST):
    body = body if body is not None else document(media_type)
    return 200, [('Content-Type', media_type),
                 ('Docker-Content-Digest', 'sha256:'+hashlib.sha256(body).hexdigest())], body


def missing(*codes):
    return 404, [('Content-Type', 'application/json')], json.dumps(
        {'errors': [{'code': code, 'message': 'not echoed'} for code in (codes or ['MANIFEST_UNKNOWN'])]}
    ).encode()


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def do_GET(self):
        self.server.seen.append((self.path, dict(self.headers)))
        status, headers, body = self.server.responses.popleft()
        if self.server.drip:
            try:
                if self.server.drip == 'headers':
                    self.connection.sendall(b'HTTP/1.1 200 OK\r\nX-Slow: ')
                else:
                    prefix = ('HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n'+
                              '\r\n'.join(f'{key}: {value}' for key,value in headers)+'\r\n\r\n').encode()
                    self.connection.sendall(prefix+f'{len(body):x}\r\n'.encode()+body+b'\r\n0\r\nX-Slow: ')
                for _ in range(40):
                    time.sleep(0.04)
                    self.connection.sendall(b'a')
                self.connection.sendall(b'\r\n\r\n')
            except OSError:
                pass
            return
        self.send_response(status)
        for key, value in headers:
            self.send_header(key, value)
        self.end_headers()
        try:
            self.wfile.write(body)
        except (BrokenPipeError, ConnectionResetError, ConnectionAbortedError):
            # Oversize/deadline tests intentionally close the client mid-response.
            pass


class InspectionTests(unittest.TestCase):
    def setUp(self):
        self.server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
        self.server.responses = deque()
        self.server.seen = []
        self.server.drip = None
        self.thread = threading.Thread(target=lambda: self.server.serve_forever(poll_interval=0.01), daemon=True)
        self.thread.start()

    def tearDown(self):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join()

    def fetch(self, url, headers):
        return registry.http_get(url, headers, connection_factory=lambda host, timeout:
                                 HTTPConnection(*self.server.server_address, timeout=timeout))

    def inspect(self, response=None, **kwargs):
        self.server.responses.append(response or success())
        return registry.inspect('query', '0.1.0', fetch=self.fetch, **kwargs)

    def assertRejected(self, response, **kwargs):
        with self.assertRaises(registry.InspectionFailed):
            self.inspect(response, **kwargs)

    def test_supported_manifests_and_indexes_verify_raw_bytes(self):
        for media_type in (OCI_MANIFEST, OCI_INDEX, DOCKER_MANIFEST, DOCKER_INDEX):
            with self.subTest(media_type=media_type):
                body = document(media_type)
                result = self.inspect(success(body, media_type))
                self.assertEqual(result['status'], 'found')
                self.assertEqual(result['digest'], 'sha256:'+hashlib.sha256(body).hexdigest())
                self.assertEqual(result['media_type'], media_type)
                self.assertFalse(result['identity_verified'])
        path, headers = self.server.seen[-1]
        self.assertEqual(path, '/v2/ktjn/observable-query/manifests/0.1.0')
        self.assertIn(OCI_INDEX, headers['Accept'])
        self.assertEqual(headers['Accept-Encoding'], 'identity')

    def test_all_component_image_paths(self):
        from ci_changed_components import ALL_COMPONENTS
        for component in ALL_COMPONENTS:
            self.server.responses.append(success())
            result = registry.inspect(component, '1.2.3-rc.1', fetch=self.fetch)
            self.assertEqual(result['image'], f'ghcr.io/ktjn/observable-{component}:1.2.3-rc.1')

    def test_only_explicit_manifest_unknown_means_missing(self):
        self.assertEqual(self.inspect(missing()), dict(status='missing', component='query', version='0.1.0'))
        for response in [missing('NAME_UNKNOWN'), missing('MANIFEST_UNKNOWN', 'DENIED'),
                         (404, [], b''), (404, [('Content-Type','text/html')], b'<html>missing</html>'),
                         (404, [('Content-Type','application/json')], b'{"errors":[]}')]:
            with self.subTest(response=response):
                self.assertRejected(response)

    def test_auth_rate_limit_server_and_redirect_fail_closed(self):
        for status in (301, 302, 307, 308, 401, 403, 429, 500, 503):
            with self.subTest(status=status):
                self.assertRejected((status, [('Location', 'https://other.invalid/stolen')], b'private diagnostic'))

    def test_redirect_does_not_forward_bearer(self):
        self.assertRejected((307, [('Location','https://other.invalid/stolen')], b''), bearer='fixture-token')
        self.assertEqual(len(self.server.seen), 1)
        self.assertEqual(self.server.seen[0][1]['Authorization'], 'Bearer fixture-token')

    def test_digest_header_missing_wrong_or_ambiguous_rejected(self):
        status, headers, body = success()
        for changed in [[headers[0]], [headers[0], ('Docker-Content-Digest','sha256:'+'b'*64)],
                        [headers[0], ('Docker-Content-Digest','sha512:'+'a'*128)], headers+[headers[1]]]:
            with self.subTest(headers=changed):
                self.assertRejected((status, changed, body))
        self.assertRejected((status, headers, body+b' '))

    def test_bad_json_schema_media_and_descriptors_rejected(self):
        bodies = [b'[]', b'{', b'{"schemaVersion":2,"schemaVersion":2}', b'{}',
                  json.dumps(dict(schemaVersion=True, mediaType=OCI_MANIFEST)).encode(),
                  json.dumps(dict(schemaVersion=2, mediaType=OCI_INDEX, manifests=[])).encode(),
                  json.dumps(dict(schemaVersion=2, mediaType=OCI_MANIFEST, config={}, layers=[])).encode()]
        for body in bodies:
            with self.subTest(body=body):
                self.assertRejected(success(body))
        self.assertRejected(success(document(), 'text/plain'))
        self.assertRejected(success(document(OCI_INDEX), OCI_MANIFEST))

    def test_content_type_parameters_allowed_but_ambiguous_headers_rejected(self):
        status, headers, body = success()
        headers[0] = ('Content-Type', OCI_MANIFEST+'; charset=utf-8')
        self.assertEqual(self.inspect((status, headers, body))['status'], 'found')
        self.assertRejected((status, headers+[headers[0]], body))

    def test_oversized_encoded_and_truncated_responses_rejected(self):
        status, headers, body = success()
        for extra in [('Content-Length', str(registry.MAX_BYTES+1)),
                      ('Content-Length', str(len(body)+10)), ('Content-Encoding','gzip')]:
            with self.subTest(extra=extra):
                self.assertRejected((status, headers+[extra], body))
        self.assertRejected((404, [('Content-Type','application/json')], b' '* (registry.MAX_BYTES+1)))

    def test_deadline_exceeded_rejects_without_waiting_for_more_body(self):
        self.server.responses.append(success())
        with patch.object(registry.time, 'monotonic', side_effect=[0, registry.REQUEST_BUDGET+1]):
            with self.assertRaises(registry.InspectionFailed):
                registry.inspect('query', '0.1.0', fetch=self.fetch)

    def test_slow_headers_and_chunk_trailers_cannot_reset_request_budget(self):
        for mode in ['headers', 'trailers']:
            with self.subTest(mode=mode):
                self.server.drip = mode
                self.server.responses.append(success())
                started = time.monotonic()
                with patch.object(registry, 'REQUEST_BUDGET', 0.15):
                    with self.assertRaises(registry.InspectionFailed):
                        registry.inspect('query', '0.1.0', fetch=self.fetch)
                self.assertLess(time.monotonic()-started, 1.0)

    def test_chunked_body_is_verified_and_ambiguous_framing_is_rejected(self):
        status, headers, body = success()
        framed = f'{len(body):x}\r\n'.encode()+body+b'\r\n0\r\n\r\n'
        self.assertEqual(self.inspect((status, headers+[('Transfer-Encoding','chunked')], framed))['status'], 'found')
        self.assertRejected((status, headers+[('Transfer-Encoding','chunked'),
                                             ('Content-Length',str(len(body)))], framed))

    def test_cli_rejects_overlong_stdin_before_inspection(self):
        output, errors = StringIO(), StringIO()
        with patch('sys.stdin', StringIO('x'*16384+'\n'*10)), redirect_stdout(output), redirect_stderr(errors):
            with patch.object(registry, 'inspect', side_effect=AssertionError('must reject before request')):
                self.assertEqual(registry.main(['query','0.1.0','--bearer-stdin']), 1)
        self.assertEqual(output.getvalue(), '')
        self.assertNotIn('xxx', errors.getvalue())

    def test_network_failures_and_tokens_do_not_escape_diagnostics(self):
        def failed(url, headers):
            raise OSError('private-upstream-value')
        with self.assertRaises(registry.InspectionFailed) as error:
            registry.inspect('query', '0.1.0', fetch=failed, bearer='private-token')
        self.assertNotIn('private', str(error.exception))
        for bearer in ['', 'bad\r\nHeader: injected', 'x' * 16385]:
            with self.assertRaises(registry.InspectionFailed):
                registry.inspect('query', '0.1.0', fetch=failed, bearer=bearer)

    def test_bad_target_rejected_before_transport(self):
        def forbidden(*args):
            self.fail('transport should not be called')
        for component, version in [('contracts','0.1.0'), ('query','../x'),
                                   ('query','0.1.0+build'), ('query','latest')]:
            with self.assertRaises(registry.InspectionFailed):
                registry.inspect(component, version, fetch=forbidden)

    def test_found_integrity_evidence_cannot_be_used_as_trusted_planner_record(self):
        from test_component_releases import identity
        result = self.inspect()
        with self.assertRaises(Rejected):
            plan_release(identity(), lambda: result)

    def test_two_live_missing_reads_support_planner_reinspection(self):
        from test_component_releases import identity
        self.server.responses.extend([missing(), missing()])
        result = plan_release(identity(), lambda: registry.inspect('query', '0.1.0', fetch=self.fetch))
        self.assertEqual(result['action'], 'publish')
        self.assertFalse(result['publication_authorized'])
        self.assertEqual(len(self.server.seen), 2)

    def test_cli_bad_input_emits_no_traceback_or_plan(self):
        result = subprocess.run([sys.executable, str(ROOT/'scripts/inspect_component_registry.py'),
                                 'query', '../invalid'], capture_output=True, text=True)
        self.assertEqual(result.returncode, 1)
        self.assertEqual(result.stdout, '')
        self.assertNotIn('Traceback', result.stderr)


if __name__ == '__main__':
    unittest.main()
