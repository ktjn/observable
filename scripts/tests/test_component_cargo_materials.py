"""Locked Cargo archives are authoritative; vendor checksums are rederived."""
import hashlib
from http.client import HTTPConnection
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import io
import json
from pathlib import Path
import sys
import tarfile
import threading
import unittest
from unittest.mock import patch

ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/'scripts'))
import component_cargo_materials as cargo
import test_component_source as fixtures


def archive(path='fixture-1.0.0/Cargo.toml',kind=None):
    output=io.BytesIO()
    with tarfile.open(fileobj=output,mode='w:gz') as tar:
        data=b'[package]\nname="fixture"\nversion="1.0.0"\n'
        item=tarfile.TarInfo(path);item.size=len(data)
        if kind:item.type=kind;item.linkname='../../outside';item.size=0
        tar.addfile(item,io.BytesIO(data) if item.isfile() else None)
    return output.getvalue()


class CargoMaterialTests(unittest.TestCase):
    git=fixtures.SourceTests.git
    write=fixtures.SourceTests.write
    commit=fixtures.SourceTests.commit

    def setUp(self):
        fixtures.SourceTests.setUp(self)
        self.blob=archive();self.checksum=hashlib.sha256(self.blob).hexdigest()
        self.lock='version = 4\n'+''.join('\n[[package]]\nname = "'+name+'"\nversion = "0.1.0"\n' for name in ('query-api','auth-service','shared','dev'))
        self.lock+='\n[[package]]\nname = "fixture"\nversion = "1.0.0"\nsource = "registry+https://github.com/rust-lang/crates.io-index"\nchecksum = "'+self.checksum+'"\n'
        self.write('Cargo.lock',self.lock);self.commit()
        self.output=self.root/'bundle';self.calls=[]

    def fetch(self,url):self.calls.append(url);return self.blob
    def create(self):return cargo.materialize(self.root,'HEAD',self.output,fetch=self.fetch)

    def test_committed_inventory_and_revalidated_directory_source(self):
        manifest=self.create()
        self.assertEqual(len(manifest['packages']),1)
        self.assertEqual(self.calls,['https://static.crates.io/crates/fixture/fixture-1.0.0.crate'])
        self.write('services/query-api/src/lib.rs','dirty')
        staged=cargo.stage(self.root,'HEAD',self.output,self.root/'staged')
        workspace=self.root/'staged/workspace'
        self.assertEqual((workspace/'services/query-api/src/lib.rs').read_text(),'// source\n')
        vendor=self.root/'staged/vendor/fixture-1.0.0'
        metadata=json.loads((vendor/'.cargo-checksum.json').read_text())
        self.assertEqual(metadata['package'],self.checksum)
        self.assertEqual(metadata['files']['Cargo.toml'],hashlib.sha256((vendor/'Cargo.toml').read_bytes()).hexdigest())
        self.assertIn('replace-with', (workspace/'.cargo/config.toml').read_text())
        self.assertEqual(staged['material_digest'],manifest['material_digest'])
        self.assertFalse(staged['identity_verified'])

    def test_invalid_lock_origin_checksum_and_dev_path_reject_before_fetch(self):
        original=self.lock
        for index,change in enumerate((original.replace('registry+https://github.com/rust-lang/crates.io-index','git+https://example.invalid/repo'),original.replace(self.checksum,'BAD'),original.replace('version = 4','version = 3'))):
            self.write('Cargo.lock',change);self.commit()
            with self.assertRaises(cargo.MaterialFailed):cargo.materialize(self.root,'HEAD',self.root/f'bad-{index}',fetch=self.fetch)
        self.write('Cargo.lock',original)
        self.write('services/query-api/Cargo.toml','[package]\nname="query-api"\nversion="0.1.0"\n[dev-dependencies]\nother={path="../../../outside"}\n');self.commit()
        with self.assertRaises(cargo.MaterialFailed):self.create()
        self.assertEqual(self.calls,[])

    def test_checksum_traversal_links_duplicates_and_wrong_root_reject(self):
        for blob in (b'corrupt',archive('../outside'),archive('wrong/Cargo.toml'),archive('fixture-1.0.0/link',tarfile.SYMTYPE)):
            with self.subTest(),self.assertRaises(cargo.MaterialFailed):
                cargo.archive_files(blob,{'name':'fixture','version':'1.0.0','checksum':hashlib.sha256(blob).hexdigest()})
        with self.assertRaises(cargo.MaterialFailed):cargo.archive_files(self.blob,{'name':'fixture','version':'1.0.0','checksum':'0'*64})

    def test_tampered_manifest_or_archive_and_existing_output_reject(self):
        manifest=self.create();path=self.output/manifest['packages'][0]['archive'];path.write_bytes(b'changed')
        with self.assertRaises(cargo.MaterialFailed):cargo.stage(self.root,'HEAD',self.output,self.root/'tampered')
        self.assertFalse((self.root/'tampered/result.json').exists())
        path.write_bytes(self.blob)
        record=json.loads((self.output/'manifest.json').read_text());record['packages'][0]['checksum']='0'*64
        (self.output/'manifest.json').write_text(json.dumps(record))
        with self.assertRaises(cargo.MaterialFailed):cargo.stage(self.root,'HEAD',self.output,self.root/'metadata')
        sentinel=self.root/'existing';sentinel.mkdir();(sentinel/'keep').write_text('keep')
        with self.assertRaises(cargo.MaterialFailed):cargo.materialize(self.root,'HEAD',sentinel,fetch=self.fetch)
        self.assertEqual((sentinel/'keep').read_text(),'keep')

    def test_unused_workspace_git_or_escaping_path_is_rejected_before_fetch(self):
        original=(self.root/'Cargo.toml').read_text()
        for index,spec in enumerate(('{git="https://example.invalid/repo"}', '{path="../outside"}')):
            self.write('Cargo.toml',original+'\n[workspace.dependencies]\nunused='+spec+'\n');self.commit()
            with self.subTest(spec=spec),self.assertRaises(cargo.MaterialFailed):
                cargo.materialize(self.root,'HEAD',self.root/f'unused-{index}',fetch=self.fetch)
        self.assertEqual(self.calls,[])

    def test_local_lock_version_must_match_committed_manifest(self):
        self.write('Cargo.lock',self.lock.replace('name = "shared"\nversion = "0.1.0"','name = "shared"\nversion = "0.2.0"'));self.commit()
        with self.assertRaises(cargo.MaterialFailed):self.create()
        self.assertEqual(self.calls,[])

    def test_duplicate_tar_entries_and_expansion_bound_reject(self):
        output=io.BytesIO()
        with tarfile.open(fileobj=output,mode='w:gz') as tar:
            for _ in range(2):
                item=tarfile.TarInfo('fixture-1.0.0/Cargo.toml');item.size=1;tar.addfile(item,io.BytesIO(b'x'))
        blob=output.getvalue()
        with self.assertRaises(cargo.MaterialFailed):cargo.archive_files(blob,{'name':'fixture','version':'1.0.0','checksum':hashlib.sha256(blob).hexdigest()})
        with patch.object(cargo,'MAX_EXPANDED',10),self.assertRaises(cargo.MaterialFailed):
            cargo.archive_files(self.blob,{'name':'fixture','version':'1.0.0','checksum':self.checksum})

    def test_failed_download_keeps_submission_window_bounded(self):
        revision,files,plan=cargo.source_plan(self.root,'HEAD')
        plan['packages']=[dict(plan['packages'][0],archive=f'archives/{i}.crate') for i in range(30)]
        calls=[]
        def fail(url):calls.append(url);raise ValueError('fixture failure')
        with patch.object(cargo,'source_plan',return_value=(revision,files,plan)):
            with self.assertRaises(cargo.MaterialFailed):cargo.materialize(self.root,'HEAD',self.output,fetch=fail)
        self.assertLessEqual(len(calls),4)
        self.assertFalse((self.output/'manifest.json').exists())

    def test_static_registry_transport_rejects_redirect(self):
        class Handler(BaseHTTPRequestHandler):
            def do_GET(inner):inner.send_response(302);inner.send_header('Location','https://example.invalid');inner.end_headers()
            def log_message(self,*args):pass
        server=ThreadingHTTPServer(('127.0.0.1',0),Handler)
        thread=threading.Thread(target=lambda:server.serve_forever(poll_interval=.01),daemon=True);thread.start()
        try:
            with self.assertRaises(cargo.MaterialFailed):cargo.http_get('https://static.crates.io/crates/fixture/fixture-1.0.0.crate',connection_factory=lambda host,timeout:HTTPConnection(*server.server_address,timeout=timeout))
        finally:server.shutdown();server.server_close();thread.join()
        for url in ('http://static.crates.io/crates/a/a-1.0.0.crate','https://user:secret@example.invalid/a.crate','https://static.crates.io/crates/a/a-1.0.0.crate?token=fixture'):
            with self.subTest(url=url),self.assertRaises(cargo.MaterialFailed):cargo.http_get(url)


if __name__=='__main__':unittest.main()
