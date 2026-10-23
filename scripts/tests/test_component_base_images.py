"""Docker Hub manifest verification through real loopback HTTP responses."""

from collections import deque
from contextlib import redirect_stderr, redirect_stdout
from http.client import HTTPConnection
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from io import StringIO
import json
from pathlib import Path
import sys
import threading
import unittest
from unittest.mock import patch
from urllib.parse import parse_qs, urlsplit

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import verify_component_base_images as bases
from collect_component_source import digest
from plan_component_release import Rejected, record


OCI_INDEX='application/vnd.oci.image.index.v1+json'
OCI_MANIFEST='application/vnd.oci.image.manifest.v1+json'
DOCKER_INDEX='application/vnd.docker.distribution.manifest.list.v2+json'
DOCKER_MANIFEST='application/vnd.docker.distribution.manifest.v2+json'


def encoded(value):
    return json.dumps(value,separators=(',',':')).encode()


def child(arch, media=OCI_MANIFEST):
    return encoded({'schemaVersion':2,'mediaType':media,
                    'config':{'mediaType':'application/vnd.oci.image.config.v1+json',
                              'digest':'sha256:'+'a'*64,'size':12},
                    'layers':[], 'annotations':{'fixture.arch':arch}})


def success(body, media, *, advertised=None):
    return (200,[('Content-Type',media),('Content-Length',str(len(body))),
                 ('Docker-Content-Digest', advertised or digest(body))],body)


class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        self.server.seen.append((self.path,dict(self.headers)))
        status,headers,body=self.server.responses.popleft()
        self.send_response(status)
        for key,value in headers: self.send_header(key,value)
        self.end_headers(); self.wfile.write(body)
    def log_message(self,*args): pass


class BaseImageTests(unittest.TestCase):
    def setUp(self):
        self.server=ThreadingHTTPServer(('127.0.0.1',0),Handler)
        self.server.responses=deque(); self.server.seen=[]
        self.thread=threading.Thread(target=lambda:self.server.serve_forever(poll_interval=0.01),daemon=True)
        self.thread.start()
        self.addCleanup(self.cleanup)
        self.children=[child('amd64'),child('arm64')]
        self.descriptors=[{'mediaType':OCI_MANIFEST,'digest':digest(body),'size':len(body),
                           'platform':{'os':'linux','architecture':arch}}
                          for arch,body in zip(('amd64','arm64'),self.children)]
        self.index=encoded({'schemaVersion':2,'mediaType':OCI_INDEX,'manifests':self.descriptors})
        self.evidence={'component':'web','source_revision':'b'*40,'input_algorithm':'component-input-v2',
                       'input_digest':'sha256:'+'c'*64,'platforms':['linux/amd64','linux/arm64'],
                       'identity_verified':False,
                       'images':[{'reference':'node:24-alpine@'+digest(self.index),
                                  'digest':digest(self.index),'stages':['deps']}],
                       'recipe':{'frontend':{'reference':'docker/dockerfile:1.28@'+digest(self.index),
                                             'digest':digest(self.index)}}}
        self.resolve=patch.object(bases.inputs,'resolve',return_value=self.evidence).start()
        self.addCleanup(patch.stopall)
        self.origins=[]

    def cleanup(self):
        self.server.shutdown(); self.server.server_close(); self.thread.join()

    def fetch(self,url,headers):
        self.origins.append((url,dict(headers)))
        return bases.http_get(url,headers,connection_factory=lambda host,timeout:
                              HTTPConnection(*self.server.server_address,timeout=timeout))

    def queue_image(self,index=None,children=None,token=b'{"token":"FIXTURE_TOKEN"}'):
        self.server.responses.append((200,[('Content-Type','application/json'),('Content-Length',str(len(token)))],token))
        self.server.responses.append(success(index or self.index,OCI_INDEX))
        for body in children or self.children:
            self.server.responses.append(success(body,OCI_MANIFEST))

    def verify(self):
        return bases.verify(Path('.'),'web',targets=('linux/amd64','linux/arm64'),fetch=self.fetch)

    def test_scoped_tokens_and_root_and_platform_manifests_are_verified_without_full_identity(self):
        self.queue_image(); self.queue_image()
        result=self.verify()
        self.resolve.assert_called_once_with(Path('.'),'web','HEAD',('linux/amd64','linux/arm64'),build_args=None)
        self.assertTrue(result['registry_images_verified'])
        self.assertFalse(result['identity_verified']); self.assertFalse(result['publication_authorized'])
        self.assertEqual(len(result['images']),2)
        self.assertEqual({entry['role'] for entry in result['images']},{'base','frontend'})
        self.assertEqual(result['images'][0]['manifests'][0]['digest'],digest(self.children[0]))
        self.assertNotIn('FIXTURE_TOKEN',json.dumps(result))
        for url,headers in self.origins:
            parsed=urlsplit(url)
            if parsed.netloc=='auth.docker.io':
                self.assertNotIn('Authorization',headers)
                self.assertEqual(parse_qs(parsed.query)['service'],['registry.docker.io'])
                self.assertIn(parse_qs(parsed.query)['scope'][0],
                              ['repository:library/node:pull','repository:docker/dockerfile:pull'])
            else:
                self.assertEqual(parsed.netloc,'registry-1.docker.io')
                self.assertEqual(headers['Authorization'],'Bearer FIXTURE_TOKEN')
                self.assertIn('/manifests/sha256:',parsed.path)
        with self.assertRaises(Rejected): record(result)

    def test_unapproved_references_fail_before_network(self):
        for reference in ['evil.example/node@'+digest(self.index),'other/team@'+digest(self.index),
                          'node:latest','node@sha256:short','node@'+digest(self.index)+'?x=1']:
            self.evidence['images'][0]['reference']=reference
            with self.subTest(reference=reference),self.assertRaises(bases.VerificationFailed): self.verify()
            self.assertEqual(self.origins,[])

    def test_token_auth_errors_and_malformed_tokens_are_sanitized(self):
        for token in [b'{}',b'{"token":"PRIVATE\nDIAGNOSTIC"}',
                      b'{"token":"ONE","access_token":"OTHER"}',b'{"token":"one","token":"two"}']:
            self.queue_image(token=token); self.queue_image()
            with self.subTest(token=token),self.assertRaises(bases.VerificationFailed) as failure: self.verify()
            self.assertNotIn('PRIVATE',str(failure.exception)); self.server.responses.clear()
        for status in (301,401,403,404,429,500):
            self.server.responses.append((status,[('Content-Type','application/json')],b'PRIVATE_DIAGNOSTIC'))
            with self.subTest(status=status),self.assertRaises(bases.VerificationFailed): self.verify()

    def test_root_digest_headers_and_body_must_match_recipe_pin(self):
        for response in [success(self.index+b' ',OCI_INDEX),
                         success(self.index,OCI_INDEX,advertised='sha256:'+'d'*64),
                         (200,[('Content-Type',OCI_INDEX)],self.index),
                         success(self.index,OCI_MANIFEST)]:
            self.queue_image(); self.server.responses[1]=response
            with self.subTest(response=response[:2]),self.assertRaises(bases.VerificationFailed): self.verify()
            self.server.responses.clear()

    def test_child_digest_size_and_media_are_bound_to_descriptor(self):
        for response in [success(self.children[0]+b' ',OCI_MANIFEST),
                         success(self.children[0],DOCKER_MANIFEST),
                         success(self.children[0],OCI_MANIFEST,advertised='sha256:'+'e'*64)]:
            self.queue_image(); self.server.responses[2]=response
            with self.subTest(response=response[:2]),self.assertRaises(bases.VerificationFailed): self.verify()
            self.server.responses.clear()
        wrong=dict(self.descriptors[0]); wrong['size']+=1
        index=encoded({'schemaVersion':2,'mediaType':OCI_INDEX,'manifests':[wrong,self.descriptors[1]]})
        self.evidence['images'][0]={'reference':'node:24-alpine@'+digest(index),'digest':digest(index)}
        self.queue_image(index=index)
        with self.assertRaises(bases.VerificationFailed): self.verify()

    def test_missing_duplicate_or_incompatible_platforms_fail(self):
        for descriptors in [self.descriptors[:1],self.descriptors+[self.descriptors[0]],
                            [dict(self.descriptors[0],platform={'os':'linux','architecture':'amd64','os.features':False}),self.descriptors[1]],
                            [dict(self.descriptors[0],platform={'os':'linux','architecture':'amd64','variant':'v3'}),self.descriptors[1]]]:
            index=encoded({'schemaVersion':2,'mediaType':OCI_INDEX,'manifests':descriptors})
            self.evidence['images'][0]={'reference':'node:24-alpine@'+digest(index),'digest':digest(index)}
            self.queue_image(index=index); self.queue_image()
            with self.subTest(descriptors=descriptors),self.assertRaises(bases.VerificationFailed): self.verify()
            self.server.responses.clear()

    def test_transport_rejects_other_origins_queries_and_redirects(self):
        for url in ['http://auth.docker.io/token','https://evil.example/token',
                    'https://auth.docker.io/token?service=evil&scope=repository:library/node:pull',
                    'https://registry-1.docker.io/v2/library/node/manifests/latest',
                    'https://registry-1.docker.io/v2/other/team/manifests/sha256:'+'a'*64]:
            with self.subTest(url=url),self.assertRaises(bases.VerificationFailed): bases.http_get(url,{})
        self.queue_image(); self.server.responses[1]=(307,[('Location','https://evil.example/token')],b'')
        with self.assertRaises(bases.VerificationFailed): self.verify()
        self.assertEqual(len(self.origins),2)

    def test_docker_schema_two_and_baseline_variants_are_supported(self):
        self.children=[child('amd64',DOCKER_MANIFEST),child('arm64',DOCKER_MANIFEST)]
        descriptors=[{'mediaType':DOCKER_MANIFEST,'digest':digest(body),'size':len(body),
                      'platform':{'os':'linux','architecture':arch,'variant':variant}}
                     for arch,variant,body in zip(('amd64','arm64'),('v1','v8'),self.children)]
        body=encoded({'schemaVersion':2,'mediaType':DOCKER_INDEX,'manifests':descriptors})
        for value in [self.evidence['images'][0],self.evidence['recipe']['frontend']]:
            value['reference']=value['reference'].split('@')[0]+'@'+digest(body); value['digest']=digest(body)
        for _ in range(2):
            self.queue_image(token=b'{"access_token":"FIXTURE_TOKEN"}')
            self.server.responses[-3]=success(body,DOCKER_INDEX)
            self.server.responses[-2]=success(self.children[0],DOCKER_MANIFEST)
            self.server.responses[-1]=success(self.children[1],DOCKER_MANIFEST)
        self.assertTrue(self.verify()['registry_images_verified'])

    def test_attestation_descriptors_do_not_match_output_platforms_and_nested_indexes_reject(self):
        other={'mediaType':OCI_MANIFEST,'digest':'sha256:'+'f'*64,'size':12,
               'platform':{'os':'unknown','architecture':'unknown'}}
        body=encoded({'schemaVersion':2,'mediaType':OCI_INDEX,'manifests':self.descriptors+[other]})
        for value in [self.evidence['images'][0],self.evidence['recipe']['frontend']]:
            value['reference']=value['reference'].split('@')[0]+'@'+digest(body); value['digest']=digest(body)
        self.queue_image(index=body); self.queue_image(index=body)
        self.assertTrue(self.verify()['registry_images_verified'])
        self.assertFalse(any('f'*64 in url for url,_ in self.origins))
        selected=dict(self.descriptors[0],mediaType=OCI_INDEX)
        nested=encoded({'schemaVersion':2,'mediaType':OCI_INDEX,'manifests':[selected,self.descriptors[1]]})
        self.evidence['images'][0]={'reference':'node@'+digest(nested),'digest':digest(nested)}
        self.queue_image(index=nested)
        with self.assertRaises(bases.VerificationFailed): self.verify()

    def test_duplicate_json_oversize_and_ambiguous_framing_fail_closed(self):
        duplicate=self.index.replace(b'{"schemaVersion":2,',b'{"schemaVersion":2,"schemaVersion":2,',1)
        self.evidence['images'][0]={'reference':'node@'+digest(duplicate),'digest':digest(duplicate)}
        self.queue_image(index=duplicate); self.queue_image()
        with self.assertRaises(bases.VerificationFailed): self.verify()
        self.server.responses.clear()
        self.evidence['images'][0]={'reference':'node@'+digest(self.index),'digest':digest(self.index)}
        responses=[(200,[('Content-Type',OCI_INDEX),('Content-Encoding','gzip')],self.index),
                   (200,[('Content-Type',OCI_INDEX),('Content-Length','4'),('Content-Length','8')],self.index),
                   (200,[('Content-Type',OCI_INDEX),('Content-Length',str(bases.registry.MAX_BYTES+1))],b'')]
        for response in responses:
            self.queue_image(); self.server.responses[1]=response
            with self.subTest(headers=response[1]),self.assertRaises(bases.VerificationFailed): self.verify()
            self.server.responses.clear()

    def test_all_references_validated_before_tokens_and_authorization_never_reaches_authority(self):
        self.evidence['recipe']['frontend']['reference']='evil.example/frontend@'+digest(self.index)
        with self.assertRaises(bases.VerificationFailed): self.verify()
        self.assertEqual(self.origins,[])
        with self.assertRaises(bases.VerificationFailed):
            bases.http_get(bases.token_url('library/node'),{'authorization':'Bearer PRIVATE_DIAGNOSTIC'})

    def test_cli_failure_has_no_raw_diagnostics_or_partial_output(self):
        with patch.object(bases,'verify',side_effect=bases.VerificationFailed('PRIVATE_DIAGNOSTIC')):
            out,err=StringIO(),StringIO()
            with redirect_stdout(out),redirect_stderr(err): status=bases.main(['web'])
        self.assertEqual(status,1); self.assertEqual(out.getvalue(),'')
        self.assertNotIn('PRIVATE_DIAGNOSTIC',err.getvalue())


if __name__=='__main__': unittest.main()
