"""Offline Rust builds consume Git-approved materials and retain runtime contracts."""
import io
import json
from pathlib import Path
import subprocess
import sys
import tarfile
import unittest
from unittest.mock import patch

sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
import build_component_rust_image as image
import component_apt_materials as apt
import test_component_cargo_materials as cargo_fixture


class RustImageTests(unittest.TestCase):
    git=cargo_fixture.CargoMaterialTests.git
    write=cargo_fixture.CargoMaterialTests.write
    commit=cargo_fixture.CargoMaterialTests.commit
    fetch=cargo_fixture.CargoMaterialTests.fetch
    create=cargo_fixture.CargoMaterialTests.create

    def setUp(self):
        cargo_fixture.CargoMaterialTests.setUp(self)
        self.write('distribution/manifest.yaml','components:\n  auth: 0.1.0\n')
        self.commit();self.create();self.cargo_bundle=self.output
        self.blob=b'!<arch>\nfixture';self.calls=[];self.apt_bundles={};records=[]
        for kind in ('builder','runtime'):
            self.output=self.root/('apt-'+kind)
            self.kind=kind
            record=apt.resolve(self.root,'HEAD',kind,'linux/amd64',self.output,run=self.native)
            records.append(record);self.apt_bundles[kind]=self.output/'bundle'
        self.write('distribution/apt-materials.lock.json',json.dumps({'algorithm':apt.LOCK_ALGORITHM,'sets':records}));self.commit()
        self.calls=[];self.failure=None;self.context=None;self.destination=self.root/'image'
        self.identifier='sha256:'+'a'*64

    def native(self,command,**kwargs):
        original=apt.REQUESTED[self.kind]
        # The fixture emits requested roots; native behavior is exercised separately in Docker.
        import hashlib
        self.calls.append((command,kwargs))
        mount=next(command[i+1] for i,arg in enumerate(command) if arg=='--mount')
        out=Path(mount.split('source=',1)[1].split(',target=',1)[0])
        for name in ('archives','control','cache','lists'):(out/name).mkdir()
        uris=[]
        for name in original:
            file=name+'_1.0_amd64.deb';data=self.blob+name.encode();sha=hashlib.sha256(data).hexdigest()
            (out/'archives'/file).write_bytes(data)
            fields=f'Package: {name}\nVersion: 1.0\nArchitecture: amd64\n'
            (out/'control'/(file+'.txt')).write_text(fields)
            (out/'cache'/(file+'.txt')).write_text(fields+f'SHA256: {sha}\nSize: {len(data)}\nFilename: pool/main/f/fixture/{file}\n')
            uris.append(f"'http://deb.debian.org/debian/pool/main/f/fixture/{file}' {file} {len(data)}")
        (out/'uris.txt').write_text('\n'.join(uris))
        (out/'base-status').write_bytes(b'base');(out/'keyring').write_bytes(b'keys')
        (out/'lists/bookworm_InRelease').write_bytes(b'signed fixture')
        return subprocess.CompletedProcess(command,0)

    def execute(self,command,**kwargs):
        self.calls.append(command)
        if 'buildx' in command:
            self.context=kwargs['input']
            labels=dict(command[i+1].split('=',1) for i,arg in enumerate(command) if arg=='--label')
            self.inspected=[{'Id':self.identifier,'Descriptor':{'digest':self.identifier},'Os':'linux','Architecture':'amd64',
                'Config':{'Labels':labels,'User':'65532:65532','Env':['SERVICE=auth-service'],'Entrypoint':['/bin/sh','-c','exec /usr/local/bin/$SERVICE']}}]
            if self.failure=='platform':self.inspected[0]['Architecture']='arm64'
            if self.failure=='labels':self.inspected[0]['Config']['Labels']={}
            if self.failure=='user':self.inspected[0]['Config']['User']='0'
            if self.failure=='service':self.inspected[0]['Config']['Env']=['SERVICE=query-api']
            if self.failure=='entrypoint':self.inspected[0]['Config']['Entrypoint']=['/bin/sh']
            if self.failure=='digest':self.inspected[0]['Descriptor']['digest']='sha256:'+'b'*64
            if self.failure=='classic':
                self.inspected[0].pop('Descriptor')
                self.inspected[0]['Id']='sha256:'+'b'*64
            Path(command[command.index('--metadata-file')+1]).write_text(json.dumps({'containerimage.digest':self.identifier,'containerimage.config.digest':'sha256:'+'b'*64}))
            Path(command[command.index('--iidfile')+1]).write_text('sha256:'+'c'*64 if self.failure=='iid' else ('sha256:'+'b'*64 if self.failure=='classic' else self.identifier))
            return subprocess.CompletedProcess(command,int(self.failure=='build'),b'',b'PRIVATE_DIAGNOSTIC')
        return subprocess.CompletedProcess(command,0,json.dumps(self.inspected).encode())

    def build(self):
        return image.build(self.root,'auth','HEAD',self.cargo_bundle,self.apt_bundles['builder'],self.apt_bundles['runtime'],self.destination,run=self.execute)

    def test_context_is_committed_and_recipe_preserves_offline_runtime(self):
        self.write('services/auth-service/src/lib.rs','dirty')
        result=self.build()
        with tarfile.open(fileobj=io.BytesIO(self.context)) as archive:
            files={m.name:archive.extractfile(m).read() for m in archive.getmembers()}
        self.assertEqual(files['workspace/services/auth-service/src/lib.rs'],b'// source\n')
        recipe=files['Dockerfile.offline'].decode()
        self.assertTrue(all(line.startswith('RUN --network=none ') for line in recipe.splitlines() if line.startswith('RUN ')))
        self.assertIn('USER 65532:65532',recipe)
        self.assertIn('COPY workspace/proto/otlp /proto/otlp',recipe)
        self.assertIn('--frozen --release --package auth-service',files['offline/compile.sh'].decode())
        self.assertIn('--no-cache',self.calls[0]);self.assertNotIn('--push',self.calls[0])
        self.assertFalse(result['identity_verified']);self.assertFalse(result['publication_authorized'])

    def test_build_or_inspection_mismatch_has_no_success_record(self):
        for failure in ('build','platform','labels','user','service','entrypoint','digest','iid'):
            self.failure=failure;self.destination=self.root/('bad-'+failure)
            with self.subTest(failure=failure),self.assertRaises(image.BuildFailed) as caught:self.build()
            self.assertNotIn('PRIVATE_DIAGNOSTIC',str(caught.exception))
            self.assertFalse((self.destination/'result.json').exists())

    def test_corrupt_material_rejected_before_docker(self):
        file=next((self.apt_bundles['runtime']/'archives').iterdir());file.write_bytes(b'changed')
        with self.assertRaises(image.BuildFailed):self.build()
        self.assertEqual(self.calls,[])

    def test_staging_tree_mutations_cannot_enter_immutable_context(self):
        original=image.cargo.stage
        def changed(*args,**kwargs):
            result=original(*args,**kwargs)
            (Path(args[3])/'workspace/services/auth-service/src/lib.rs').write_text('changed staged source')
            return result
        with patch.object(image.cargo,'stage',side_effect=changed):self.build()
        with tarfile.open(fileobj=io.BytesIO(self.context)) as archive:
            self.assertEqual(archive.extractfile('workspace/services/auth-service/src/lib.rs').read(),b'// source\n')

    def test_classic_config_digest_image_binding(self):
        self.failure='classic';result=self.build()
        self.assertEqual(result['image_id'],'sha256:'+'b'*64)

    def test_corrupt_cargo_archive_rejected_before_docker(self):
        file=next((self.cargo_bundle/'archives').iterdir());file.write_bytes(b'changed')
        with self.assertRaises(image.BuildFailed):self.build()
        self.assertEqual(self.calls,[])


class ContextTests(unittest.TestCase):
    def test_vendor_executable_bits_and_canonical_tar_metadata(self):
        value=image.context_bytes({'vendor/tool/run':(b'execute',True),'workspace/source.rs':(b'source',False)})
        self.assertEqual(value,image.context_bytes({'workspace/source.rs':(b'source',False),'vendor/tool/run':(b'execute',True)}))
        with tarfile.open(fileobj=io.BytesIO(value)) as archive:
            self.assertEqual(archive.getmember('vendor/tool/run').mode,0o755)
            self.assertEqual(archive.getmember('workspace/source.rs').mode,0o644)
            self.assertTrue(all(m.isfile() and m.mtime==m.uid==m.gid==0 for m in archive.getmembers()))
        with self.assertRaises(ValueError):image.context_bytes({'../escape':(b'',False)})

if __name__=='__main__':unittest.main()
