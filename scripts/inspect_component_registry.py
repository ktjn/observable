"""Inspect component manifest presence/integrity without publishing anything.

Usage: python scripts/inspect_component_registry.py COMPONENT VERSION [--bearer-stdin]

Found evidence is NOT a trusted release-planner record. No identity, provenance,
SBOM, platform support, or historical publication is authenticated here.
"""

import argparse
import hashlib
from http.client import HTTPResponse, HTTPSConnection
from io import BufferedReader, RawIOBase
import json
import sys
import time
from urllib.parse import urlsplit

from ci_changed_components import ALL_COMPONENTS
from plan_component_release import DIGEST, VERSION, matches, unique_object


MAX_BYTES = 4 * 1024 * 1024
SOCKET_TIMEOUT = 10
REQUEST_BUDGET = 20
MEDIA_TYPES = (
    "application/vnd.oci.image.manifest.v1+json",
    "application/vnd.oci.image.index.v1+json",
    "application/vnd.docker.distribution.manifest.v2+json",
    "application/vnd.docker.distribution.manifest.list.v2+json",
)
INDEX_TYPES = (MEDIA_TYPES[1], MEDIA_TYPES[3])


class InspectionFailed(ValueError):
    """No safe registry observation is available."""


class DeadlineReader(RawIOBase):
    """Apply a shrinking timeout to each underlying read, including headers/trailers."""

    def __init__(self, stream, sock, deadline):
        self.stream, self.sock, self.deadline = stream, sock, deadline

    def readable(self):
        return True

    def readinto(self, buffer):
        remaining = self.deadline - time.monotonic()
        if remaining <= 0:
            raise InspectionFailed("registry request deadline exceeded")
        self.sock.settimeout(min(SOCKET_TIMEOUT, remaining))
        chunk = self.stream.read1(len(buffer))
        buffer[:len(chunk)] = chunk
        return len(chunk)

    def close(self):
        try:
            self.stream.close()
        finally:
            super().close()


class DeadlineResponse(HTTPResponse):
    def __init__(self, sock, deadline, *args, **kwargs):
        super().__init__(sock, *args, **kwargs)
        self.fp = BufferedReader(DeadlineReader(self.fp, sock, deadline))


def one(headers, name, default=None):
    values = [value for key, value in headers if key.lower() == name.lower()]
    if len(values) > 1:
        raise InspectionFailed("ambiguous registry headers")
    return values[0] if values else default


def framing(headers):
    if one(headers, "Content-Encoding", "identity").lower() != "identity":
        raise InspectionFailed("encoded registry response")
    length = one(headers, "Content-Length")
    transfer = one(headers, "Transfer-Encoding")
    if transfer is not None and (transfer.lower() != "chunked" or length is not None):
        raise InspectionFailed("ambiguous response framing")
    if length is not None:
        if not matches(r"[0-9]+", length) or int(length) > MAX_BYTES:
            raise InspectionFailed("invalid response size")
        return int(length)
    return None


def http_get(url, headers, *, connection_factory=HTTPSConnection):
    """Fixed-origin HTTPS, GET only, no redirects/proxy or credential discovery.

    The connection factory is an integration-test seam, never a CLI URL option.
    A socket timeout bounds connection/TLS I/O. Every response socket read uses
    the remaining request budget, including status, headers, and chunk trailers.
    """
    parsed = urlsplit(url)
    if (parsed.scheme != "https" or parsed.netloc != "ghcr.io"
            or parsed.query or parsed.fragment or not parsed.path.startswith("/v2/ktjn/observable-")):
        raise InspectionFailed("unsupported registry origin")
    return bounded_https_get("ghcr.io", parsed.path, headers, connection_factory=connection_factory)


def bounded_https_get(host, target, headers, *, connection_factory=HTTPSConnection):
    """Bounded GET mechanics; callers must enforce their fixed origin/path policy."""
    deadline = time.monotonic() + REQUEST_BUDGET
    connection = connection_factory(host, timeout=SOCKET_TIMEOUT)
    connection.response_class = lambda sock, *args, **kwargs: DeadlineResponse(sock, deadline, *args, **kwargs)
    response = None
    try:
        connection.request("GET", target, headers=headers)
        response = connection.getresponse()
        response_headers = response.getheaders()
        expected = framing(response_headers)
        body = bytearray()
        while True:
            if time.monotonic() > deadline:
                raise InspectionFailed("registry request deadline exceeded")
            chunk = response.read1(min(65536, MAX_BYTES + 1 - len(body)))
            if not chunk:
                break
            body.extend(chunk)
            if len(body) > MAX_BYTES:
                raise InspectionFailed("registry response too large")
        if expected is not None and len(body) != expected:
            raise InspectionFailed("truncated registry response")
        return response.status, response_headers, bytes(body)
    finally:
        if response is not None:
            response.close()
        connection.close()


def reject_constant(value):
    raise InspectionFailed("invalid JSON constant")


def parse_json(body):
    return json.loads(body.decode("utf-8"), object_pairs_hook=unique_object,
                      parse_constant=reject_constant)


def descriptor(value):
    if (not isinstance(value, dict) or not isinstance(value.get("mediaType"), str)
            or not value["mediaType"] or not matches(DIGEST, value.get("digest"))
            or type(value.get("size")) is not int or value["size"] < 0):
        raise InspectionFailed("invalid manifest descriptor")


def manifest(value, media_type):
    if (not isinstance(value, dict) or type(value.get("schemaVersion")) is not int
            or value["schemaVersion"] != 2 or value.get("mediaType", media_type) != media_type):
        raise InspectionFailed("invalid manifest schema or media type")
    if media_type in INDEX_TYPES:
        descriptors = value.get("manifests")
        if not isinstance(descriptors, list) or not descriptors:
            raise InspectionFailed("empty or invalid component index")
    else:
        descriptor(value.get("config"))
        descriptors = value.get("layers")
        if not isinstance(descriptors, list):
            raise InspectionFailed("invalid manifest layers")
    for item in descriptors:
        descriptor(item)


def inspect(component, version, *, fetch=http_get, bearer=None):
    """Return manifest evidence; every failure is sanitized and fails closed.

    fetch(url, headers) may authenticate an explicitly supplied scoped bearer.
    It must not follow redirects or write registry state. No automatic challenge
    exchange or host credential lookup is performed.
    """
    try:
        return _inspect(component, version, fetch, bearer)
    except Exception:
        # Registry bodies, URLs and exception messages may contain credentials.
        raise InspectionFailed("registry inspection failed; no safe observation") from None


def _inspect(component, version, fetch, bearer):
    if component not in ALL_COMPONENTS or not matches(VERSION, version) or len(version) > 128:
        raise InspectionFailed("invalid component/version")
    headers = {"Accept": ", ".join(MEDIA_TYPES), "Accept-Encoding": "identity"}
    if bearer is not None:
        if not matches(r"[A-Za-z0-9._~+/-]+=*", bearer) or len(bearer) > 16384:
            raise InspectionFailed("invalid bearer")
        headers["Authorization"] = "Bearer " + bearer
    image = f"ghcr.io/ktjn/observable-{component}:{version}"
    url = f"https://ghcr.io/v2/ktjn/observable-{component}/manifests/{version}"
    status, response_headers, body = fetch(url, headers)
    if type(status) is not int or not isinstance(body, bytes) or len(body) > MAX_BYTES:
        raise InspectionFailed("invalid registry response")
    expected = framing(response_headers)
    if expected is not None and expected != len(body):
        raise InspectionFailed("inconsistent response length")
    media_type = one(response_headers, "Content-Type", "").split(";", 1)[0].strip().lower()
    if status == 404 and media_type == "application/json":
        value = parse_json(body)
        errors = value.get("errors") if isinstance(value, dict) else None
        if (isinstance(errors, list) and errors
                and all(isinstance(error, dict) and error.get("code") == "MANIFEST_UNKNOWN"
                        for error in errors)):
            return {"status": "missing", "component": component, "version": version}
        raise InspectionFailed("ambiguous missing manifest")
    if status != 200 or media_type not in MEDIA_TYPES:
        raise InspectionFailed("registry manifest unavailable")
    advertised = one(response_headers, "Docker-Content-Digest")
    actual = "sha256:" + hashlib.sha256(body).hexdigest()
    if not matches(DIGEST, advertised) or advertised != actual:
        raise InspectionFailed("manifest digest mismatch")
    manifest(parse_json(body), media_type)
    return {"status": "found", "component": component, "version": version,
            "image": image, "digest": actual, "media_type": media_type,
            "identity_verified": False}


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("component", choices=ALL_COMPONENTS)
    parser.add_argument("version")
    parser.add_argument("--bearer-stdin", action="store_true")
    args = parser.parse_args(argv)
    try:
        raw_bearer = sys.stdin.read(16387) if args.bearer_stdin else None
        if raw_bearer is not None and len(raw_bearer) > 16386:
            raise InspectionFailed("overlong bearer input")
        bearer = raw_bearer.rstrip("\r\n") if raw_bearer is not None else None
        result = inspect(args.component, args.version, bearer=bearer)
    except (InspectionFailed, OSError, UnicodeError):
        print("registry inspection failed; no safe observation", file=sys.stderr)
        return 1
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
