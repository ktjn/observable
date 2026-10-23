"""APT resolution proposes locks; Git-approved hashes govern later consumption."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import unittest
from unittest.mock import patch

ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/'scripts'))
import component_apt_materials as apt
import test_component_source as fixtures


class AptMaterialTests(unittest.TestCase):
    git=fixtures.SourceTests.git
    write=fixtures.SourceTests.write
    commit=fixtures.SourceTests.commit

    def setUp(self):
        fixtures.SourceTests.setUp(self)
        self.write('distribution/manifest.yaml','components:\n  auth: 0.1.0\n')
        self.commit();self.output=self.root/'resolved';self.calls=[]
        self.blob=b'!<arch>\nfixture';self.sha=hashlib.sha256(self.blob).hexdigest()

    def native(self,command,**kwargs):
        self.calls.append((command,kwargs))
        mount=next(command[i+1] for i,arg in enumerate(command) if arg=='--mount' and 'target=/materials' in command[i+1])
        output=Path(mount.split('source=',1)[1].split(',target=',1)[0])
        (output/'archives').mkdir();(output/'control').mkdir();(output/'cache').mkdir();(output/'lists').mkdir()
        lines=[]
        for name in ('ca-certificates','curl','libssl3'):
            filename=name+'_1.0.0_amd64.deb'
            (output/'archives'/filename).write_bytes(self.blob+name.encode())
            data=(output/'archives'/filename).read_bytes();sha=hashlib.sha256(data).hexdigest()
            fields=f'Package: {name}\nVersion: 1.0.0\nArchitecture: amd64\n'
            (output/'control'/(filename+'.txt')).write_text(fields)
            (output/'cache'/(filename+'.txt')).write_text(fields+f'SHA256: {sha}\nSize: {len(data)}\nFilename: pool/main/f/fixture/{filename}\n')
            lines.append(f"'http://deb.debian.org/debian/pool/main/f/fixture/{filename}' {filename} {len(data)} SHA256:{sha}")
        (output/'uris.txt').write_text('\n'.join(lines)+'\n')
        (output/'base-status').write_bytes(b'base status')
        (output/'keyring').write_bytes(b'public archive keys')
        (output/'lists/deb.debian.org_debian_dists_bookworm_InRelease').write_bytes(b'signed index fixture')
        return subprocess.CompletedProcess(command,0,b'',b'')

    def resolve(self):return apt.resolve(self.root,'HEAD','runtime','linux/amd64',self.output,run=self.native)

    def lock(self):
        proposal=self.resolve()
        self.write('distribution/apt-materials.lock.json',json.dumps({'algorithm':apt.LOCK_ALGORITHM,'sets':[proposal]}));self.commit()
        return proposal

    def test_native_resolution_records_exact_bytes_and_disables_insecure_sources(self):
        result=self.resolve();command,kwargs=self.calls[0]
        self.assertEqual(result['requested'],list(apt.REQUESTED['runtime']))
        self.assertEqual(len(result['packages']),3)
        self.assertEqual(command[command.index('--platform')+1],'linux/amd64')
        self.assertIn('-i',command)
        script=kwargs['input'].decode()
        self.assertIn('APT::Update::Error-Mode=any',script)
        self.assertIn('Acquire::AllowInsecureRepositories=false',script)
        self.assertIn('--download-only',script)
        self.assertIn('--reinstall',script)
        self.assertNotIn('--allow-unauthenticated',script)
        self.assertTrue((self.output/'proposal.json').is_file())

    def test_git_locked_stage_ignores_dirty_lock_and_writes_offline_guarded_installer(self):
        expected=self.lock()
        self.write('distribution/apt-materials.lock.json','dirty')
        result=apt.stage(self.root,'HEAD','runtime','linux/amd64',self.output/'bundle',self.root/'stage')
        self.assertEqual(result['package_count'],3)
        self.assertFalse(result['identity_verified'])
        self.assertFalse(result['publication_authorized'])
        script=(self.root/'stage/install.sh').read_text()
        self.assertIn('--no-download',script)
        self.assertIn('sha256sum -c',script)
        self.assertIn(expected['base_status_digest'][7:],script)
        self.assertNotIn('apt-get update',script)

    def test_installer_passes_only_locked_archive_arguments(self):
        import shlex
        record=self.resolve()
        script=apt.installer(record).decode()
        command=next(line for line in script.splitlines() if 'apt-get ' in line)
        args=shlex.split(command)
        self.assertEqual(args[args.index('install')+1:],['/materials/'+p['archive'] for p in record['packages']])

    def test_native_three_field_uris_are_supported(self):
        def native(command,**kwargs):
            result=self.native(command,**kwargs)
            p=self.output/'uris.txt'
            p.write_text('\n'.join(line.rsplit(' ',1)[0] for line in p.read_text().splitlines()))
            return result
        self.assertEqual(len(apt.resolve(self.root,'HEAD','runtime','linux/amd64',self.output,run=native)['packages']),3)

    def test_missing_planned_dependency_archive_is_rejected(self):
        def native(command,**kwargs):
            result=self.native(command,**kwargs)
            with (self.output/'uris.txt').open('a') as out:
                out.write("'http://deb.debian.org/debian/pool/main/f/fixture/dependency.deb' dependency.deb 100\n")
            return result
        with self.assertRaises(apt.MaterialFailed):
            apt.resolve(self.root,'HEAD','runtime','linux/amd64',self.output,run=native)

    def test_fetch_rechecks_git_approved_bytes_and_rejects_corruption(self):
        record=self.lock()
        blobs={p['url']:(self.output/'bundle'/p['archive']).read_bytes() for p in record['packages']}
        result=apt.fetch(self.root,'HEAD','runtime','linux/amd64',self.root/'fetch',download=blobs.__getitem__)
        self.assertEqual(result['package_count'],3)
        self.assertEqual(json.loads((self.root/'fetch/manifest.json').read_text()),record)
        with self.assertRaises(apt.MaterialFailed):
            apt.fetch(self.root,'HEAD','runtime','linux/amd64',self.root/'bad-fetch',download=lambda url:b'corrupt')
        self.assertFalse((self.root/'bad-fetch/manifest.json').exists())

    def test_native_index_checksum_mismatch_rejects_proposal(self):
        def native(command,**kwargs):
            result=self.native(command,**kwargs)
            file=next((self.output/'cache').iterdir())
            content=file.read_text()
            import re
            file.write_text(re.sub(r'SHA256: [0-9a-f]+','SHA256: '+'0'*64,content))
            return result
        with self.assertRaises(apt.MaterialFailed):
            apt.resolve(self.root,'HEAD','runtime','linux/amd64',self.output,run=native)
        self.assertFalse((self.output/'proposal.json').exists())

    def test_installer_populates_only_locked_canonical_cache_names(self):
        record=self.resolve();script=apt.installer(record).decode()
        for item in record['packages']:
            canonical=item['name']+'_'+item['version'].split(':')[-1]+'_'+item['architecture']+'.deb'
            self.assertIn('cp /materials/'+item['archive']+' "$cache/archives/'+canonical+'"',script)
        self.assertIn('mktemp -d',script)

    def test_tampered_archive_or_bundle_metadata_fail(self):
        expected=self.lock();bundle=self.output/'bundle'
        target=bundle/expected['packages'][0]['archive'];target.write_bytes(b'changed')
        with self.assertRaises(apt.MaterialFailed):apt.stage(self.root,'HEAD','runtime','linux/amd64',bundle,self.root/'bad')
        manifest=json.loads((bundle/'manifest.json').read_text());manifest['base']='evil'
        (bundle/'manifest.json').write_text(json.dumps(manifest))
        with self.assertRaises(apt.MaterialFailed):apt.stage(self.root,'HEAD','runtime','linux/amd64',bundle,self.root/'metadata')

    def test_bad_kind_platform_and_existing_output_reject_before_native(self):
        for kind,platform in (('unknown','linux/amd64'),('runtime','windows/amd64')):
            with self.assertRaises(apt.MaterialFailed):apt.resolve(self.root,'HEAD',kind,platform,self.output,run=self.native)
        self.output.mkdir();(self.output/'keep').write_text('keep')
        with self.assertRaises(apt.MaterialFailed):self.resolve()
        self.assertEqual((self.output/'keep').read_text(),'keep');self.assertEqual(self.calls,[])

    def test_unapproved_url_or_changed_base_lock_is_rejected(self):
        proposal=self.lock()
        for index,field in enumerate(('url','base')):
            changed=json.loads(json.dumps(proposal))
            if field=='url':changed['packages'][0]['url']='https://user:fixture@evil.invalid/a.deb'
            else:changed['base']='debian:unreviewed'
            self.write('distribution/apt-materials.lock.json',json.dumps({'algorithm':apt.LOCK_ALGORITHM,'sets':[changed]}));self.commit()
            with self.assertRaises(apt.MaterialFailed):apt.stage(self.root,'HEAD','runtime','linux/amd64',self.output/'bundle',self.root/f'wrong-{index}')


if __name__=='__main__':unittest.main()
