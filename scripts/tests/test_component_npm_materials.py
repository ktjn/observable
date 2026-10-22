"""Verified archive bytes and isolated offline npm dependency consumption."""

import base64
import copy
import hashlib
from http.client import HTTPConnection
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import threading
import unittest
from unittest.mock import patch

ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/'scripts'))
import component_npm_materials as materials
import prepare_component_npm as prepare
import test_component_source as fixtures


def archive(path='package/package.json', *, kind=None):
    out=io.BytesIO()
    with tarfile.open(fileobj=out,mode='w:gz') as tar:
        data=b'{"name":"fixture","version":"1.0.0"}'
        item=tarfile.TarInfo(path); item.size=len(data)
        if kind:
            item.type=kind; item.linkname='../../outside'; item.size=0
        tar.addfile(item,io.BytesIO(data) if item.isfile() else None)
    return out.getvalue()


def integrity(data):
    return 'sha512-'+base64.b64encode(hashlib.sha512(data).digest()).decode()


class NpmMaterialTests(unittest.TestCase):
    git=fixtures.SourceTests.git
    write=fixtures.SourceTests.write
    commit=fixtures.SourceTests.commit

    def setUp(self):
        fixtures.SourceTests.setUp(self)
        self.blob=archive(); self.url='https://registry.npmjs.org/fixture/-/fixture-1.0.0.tgz'
        self.lock={'lockfileVersion':3,'packages':{
            '':{'workspaces':['apps/frontend']},
            'apps/frontend':{'name':'fixture-web','version':'0.1.0','dependencies':{'fixture':'1.0.0'}},
            'node_modules/fixture-web':{'resolved':'apps/frontend','link':True},
            'node_modules/fixture':{'version':'1.0.0','resolved':self.url,'integrity':integrity(self.blob)},
            'node_modules/other/node_modules/fixture':{'version':'1.0.0','resolved':self.url,'integrity':integrity(self.blob)},
        }}
        self.write('distribution/manifest.yaml','components:\n  web: 0.1.0\n')
        self.write('apps/frontend/package.json',json.dumps({'name':'fixture-web','version':'0.1.0','dependencies':{'fixture':'1.0.0'}}))
        self.write('package-lock.json',json.dumps(self.lock)); self.commit()
        self.output=self.root/'materials'; self.calls=[]

    def fetch(self,url): self.calls.append(url); return self.blob
    def create(self): return materials.materialize(self.root,'HEAD',self.output,fetch=self.fetch)

    def test_failed_download_does_not_start_entire_inventory(self):
        revision, files, plan = materials.source_plan(self.root, 'HEAD')
        plan['archives'] = [dict(plan['archives'][0], path=f'archives/{i}.tgz') for i in range(30)]
        calls = []
        def fail(url):
            calls.append(url)
            raise RuntimeError('fixture failure')
        with patch.object(materials, 'source_plan', return_value=(revision, files, plan)):
            with self.assertRaises(materials.MaterialFailed):
                materials.materialize(self.root, 'HEAD', self.output, fetch=fail)
        self.assertLessEqual(len(calls), 4)
        self.assertFalse((self.output / 'manifest.json').exists())

    def test_committed_metadata_archive_integrity_dedup_and_repeatable_identity(self):
        first=self.create()
        second=materials.materialize(self.root,'HEAD',self.root/'second',fetch=self.fetch)
        self.assertEqual(first,second)
        self.assertEqual(len(first['packages']),2); self.assertEqual(len(first['archives']),1)
        self.assertEqual(self.calls,[self.url,self.url])
        self.assertEqual(first['algorithm'],'npm-materials-v1')
        self.assertTrue(first['material_bytes_verified']); self.assertFalse(first['identity_verified'])
        self.assertTrue((self.output/'manifest.json').is_file())
        self.write('package-lock.json','DIRTY PRIVATE SENTINEL')
        self.assertEqual(materials.materialize(self.root,'HEAD',self.root/'dirty',fetch=self.fetch),first)

    def test_invalid_lock_records_fail_before_download_or_output(self):
        variants=[]
        for change in [{'integrity':'sha512:bad'}, {'integrity':'sha1-Zm9v'},
                       {'resolved':'https://user:PRIVATE_SENTINEL@registry.npmjs.org/a.tgz'},
                       {'resolved':'https://evil.example/a.tgz'}, {'resolved':self.url+'?token=PRIVATE_SENTINEL'},
                       {'resolved':'file:../../outside'}, {'link':True}]:
            value=copy.deepcopy(self.lock); value['packages']['node_modules/fixture'].update(change); variants.append(value)
        value=copy.deepcopy(self.lock); value['lockfileVersion']=2; variants.append(value)
        for index,value in enumerate(variants):
            self.write('package-lock.json',json.dumps(value)); self.commit()
            with self.subTest(index=index),self.assertRaises(materials.MaterialFailed) as caught: self.create()
            self.assertNotIn('PRIVATE_SENTINEL',str(caught.exception))
            self.assertFalse(self.output.exists()); self.assertEqual(self.calls,[])

    def test_archive_checksum_or_layout_failure_never_writes_success(self):
        for index,blob in enumerate([b'not the expected bytes',archive('../outside'),
                                    archive('/absolute'),archive('package/link',kind=tarfile.SYMTYPE),
                                    archive('package/a\\..\\outside')]):
            self.output=self.root/f'bad-{index}'
            if index:
                self.lock['packages']['node_modules/fixture']['integrity']=integrity(blob)
                self.lock['packages']['node_modules/other/node_modules/fixture']['integrity']=integrity(blob)
                self.write('package-lock.json',json.dumps(self.lock)); self.commit()
            with self.subTest(index=index),self.assertRaises(materials.MaterialFailed):
                materials.materialize(self.root,'HEAD',self.output,fetch=lambda url:blob)
            self.assertFalse((self.output/'manifest.json').exists())

    def test_existing_output_is_preserved(self):
        self.output.mkdir(); (self.output/'keep').write_text('keep')
        with self.assertRaises(materials.MaterialFailed): self.create()
        self.assertEqual((self.output/'keep').read_text(),'keep'); self.assertEqual(self.calls,[])

    def test_workspace_local_node_modules_records_are_in_the_material_closure(self):
        self.lock['packages']['apps/frontend/node_modules/fixture']=dict(self.lock['packages']['node_modules/fixture'])
        self.write('package-lock.json',json.dumps(self.lock)); self.commit()
        result=self.create()
        self.assertEqual(len(result['packages']),3)
        self.assertEqual(len(result['archives']),1)
        self.assertIn('apps/frontend/node_modules/fixture',{entry['path'] for entry in result['packages']})

    def test_offline_commands_use_private_cache_configs_environment_and_local_archives(self):
        expected=self.create(); calls=[]; output=self.root/'install'
        def run(command,**kwargs):
            calls.append((command,kwargs))
            self.assertIn('--offline',command); self.assertIn('--ignore-scripts',command)
            self.assertIn('--no-audit',command); self.assertIn('--no-fund',command)
            self.assertIn('--logs-max=0',command)
            self.assertNotIn('NPM_CONFIG_TOKEN',kwargs['env']); self.assertNotIn('NODE_OPTIONS',kwargs['env'])
            self.assertNotIn('HTTPS_PROXY',kwargs['env'])
            self.assertEqual(kwargs['stdin'],subprocess.DEVNULL)
            if command[2]=='ci': (output/'workspace/node_modules').mkdir()
            return subprocess.CompletedProcess(command,0,b'',b'PRIVATE_DIAGNOSTIC')
        with patch.dict(os.environ,{'NPM_CONFIG_TOKEN':'PRIVATE_SENTINEL','NODE_OPTIONS':'PRIVATE_SENTINEL','HTTPS_PROXY':'PRIVATE_SENTINEL'}),\
             patch.object(prepare,'npm_command',return_value=['node','npm-cli.js']):
            result=prepare.install(self.root,'HEAD',self.output,output,run=run)
        self.assertEqual([command[2] for command,_ in calls],['cache','ci'])
        self.assertTrue(result['offline_install_verified']); self.assertFalse(result['identity_verified'])
        self.assertEqual(result['material_digest'],expected['material_digest'])
        self.assertTrue((output/'result.json').is_file())
        self.assertEqual((output/'workspace/package-lock.json').read_bytes(),self.git('show','HEAD:package-lock.json').encode())

    def test_tampered_bundle_rejects_before_npm_and_failed_install_has_no_success(self):
        manifest=self.create(); blob=self.output/manifest['archives'][0]['path']
        original=blob.read_bytes(); blob.write_bytes(b'changed')
        calls=[]
        def fail(command,**kwargs): calls.append(command); return subprocess.CompletedProcess(command,1,b'PRIVATE_DIAGNOSTIC',b'PRIVATE_DIAGNOSTIC')
        with patch.object(prepare,'npm_command',return_value=['node','npm-cli.js']):
            with self.assertRaises(materials.MaterialFailed): prepare.install(self.root,'HEAD',self.output,self.root/'tampered',run=fail)
            self.assertEqual(calls,[])
            blob.write_bytes(original)
            with self.assertRaises(materials.MaterialFailed) as caught: prepare.install(self.root,'HEAD',self.output,self.root/'failed',run=fail)
            self.assertNotIn('PRIVATE_DIAGNOSTIC',str(caught.exception))
        self.assertFalse((self.root/'failed/result.json').exists())

    def test_token_free_fixed_registry_transport_rejects_redirects_and_bad_origins(self):
        class Handler(BaseHTTPRequestHandler):
            def do_GET(inner):
                inner.send_response(302); inner.send_header('Location','https://evil.example/archive'); inner.end_headers()
            def log_message(self,*args): pass
        server=ThreadingHTTPServer(('127.0.0.1',0),Handler)
        thread=threading.Thread(target=lambda:server.serve_forever(poll_interval=0.01),daemon=True); thread.start()
        try:
            with self.assertRaises(materials.MaterialFailed):
                materials.http_get(self.url,connection_factory=lambda host,timeout:HTTPConnection(*server.server_address,timeout=timeout))
            for url in ['http://registry.npmjs.org/a.tgz','https://evil.example/a.tgz',self.url+'?token=bad']:
                with self.subTest(url=url),self.assertRaises(materials.MaterialFailed): materials.http_get(url)
        finally: server.shutdown(); server.server_close(); thread.join()


if __name__=='__main__': unittest.main()
