"""Resolve native APT closures, then consume only Git-locked archive identities."""

from concurrent.futures import ThreadPoolExecutor, wait, FIRST_COMPLETED
from pathlib import Path
import re
import shlex
import subprocess
from urllib.parse import urlsplit, unquote

import collect_component_inputs as inputs
from collect_component_source import GitSnapshot, canonical, digest
import component_npm_materials as common

ALGORITHM='apt-materials-v1'
LOCK_ALGORITHM='apt-material-lock-v1'
LOCK_PATH='distribution/apt-materials.lock.json'
REQUESTED={'runtime':('ca-certificates','curl','libssl3'),
           'builder':('build-essential','cmake','libcurl4-openssl-dev','libssl-dev','pkg-config','zlib1g-dev')}
MaterialFailed=common.MaterialFailed


def selection(root,ref,kind,platform):
    if kind not in REQUESTED or platform not in ('linux/amd64','linux/arm64'):
        raise MaterialFailed('unsupported APT selection')
    evidence=inputs.resolve(root,'auth',ref,(platform,))
    stage='component-runtime' if kind=='runtime' else 'chef'
    base=next(item['reference'] for item in evidence['images'] if item['stages']==[stage])
    return evidence['source_revision'],base,evidence['recipe']['digest']


def sources(platform):
    arch=platform.split('/')[1]
    return f"""Types: deb
URIs: http://deb.debian.org/debian
Suites: bookworm bookworm-updates
Components: main
Architectures: {arch}
Signed-By: /usr/share/keyrings/debian-archive-keyring.gpg

Types: deb
URIs: http://deb.debian.org/debian-security
Suites: bookworm-security
Components: main
Architectures: {arch}
Signed-By: /usr/share/keyrings/debian-archive-keyring.gpg
""".encode()


OPTIONS="""-o Dir::State::lists=/materials/lists -o Dir::Cache::archives=/materials/archives
-o Dir::Cache::pkgcache=/materials/pkgcache.bin -o Dir::Cache::srcpkgcache=/materials/srcpkgcache.bin
-o Dir::Etc::sourcelist=/materials/debian.sources -o Dir::Etc::sourceparts=-
-o Dir::Etc::main=- -o Dir::Etc::parts=-
-o Acquire::AllowInsecureRepositories=false -o Acquire::AllowDowngradeToInsecureRepositories=false
-o APT::Get::AllowUnauthenticated=false -o APT::Update::Error-Mode=any""".replace('\n',' ')


def resolver_script(kind):
    packages=' '.join(REQUESTED[kind])
    return f"""set -eu
mkdir -p /materials/lists/partial /materials/archives/partial /materials/control /materials/cache
cp /var/lib/dpkg/status /materials/base-status
cp /usr/share/keyrings/debian-archive-keyring.gpg /materials/keyring
aptcmd() {{ command=$1; shift; "$command" {OPTIONS} "$@"; }}
aptcmd apt-get update
aptcmd apt-get --print-uris --yes --download-only --reinstall --no-install-recommends install {packages} > /materials/uris.txt
aptcmd apt-get --yes --download-only --reinstall --no-install-recommends install {packages}
for file in /materials/archives/*.deb; do
 name=$(basename "$file")
 dpkg-deb --field "$file" Package Version Architecture > "/materials/control/$name.txt"
 package=$(dpkg-deb --field "$file" Package)
 version=$(dpkg-deb --field "$file" Version)
 architecture=$(dpkg-deb --field "$file" Architecture)
 aptcmd apt-cache show --no-all-versions "$package:$architecture=$version" > "/materials/cache/$name.txt"
done
cmp /var/lib/dpkg/status /materials/base-status
""".encode()


def paragraphs(data):
    result=[]
    for block in data.decode('utf-8').strip().split('\n\n'):
        fields={}
        for line in block.splitlines():
            if line.startswith((' ','\t')):continue
            key,separator,value=line.partition(':')
            if not separator or key in fields:raise MaterialFailed('ambiguous Debian control metadata')
            fields[key]=value.strip()
        if fields:result.append(fields)
    return result


def archive_url(url):
    parsed=urlsplit(url)
    if (parsed.scheme!='https' or parsed.netloc!='deb.debian.org' or parsed.query or parsed.fragment
            or not parsed.path.startswith(('/debian/pool/','/debian-security/pool/')) or not parsed.path.endswith('.deb')):
        raise MaterialFailed('unsupported Debian archive origin')
    common.safe_path(unquote(parsed.path[1:]))
    return parsed


def verify_deb(data,entry):
    if (not isinstance(data,bytes) or len(data)!=entry['size'] or len(data)>common.MAX_ARCHIVE
            or not data.startswith(b'!<arch>\n') or digest(data)!=entry['digest']):
        raise MaterialFailed('Debian archive bytes differ')


def validate_record(record,kind,platform,base,recipe_digest):
    expected={'algorithm','resolved_from','kind','platform','base','recipe_digest','requested','sources_digest',
              'keyring_digest','base_status_digest','indexes','packages'}
    if (not isinstance(record,dict) or set(record)!=expected or record['algorithm']!=ALGORITHM
            or record['kind']!=kind or record['platform']!=platform or record['base']!=base
            or record['recipe_digest']!=recipe_digest or record['requested']!=list(REQUESTED[kind])
            or record['sources_digest']!=digest(sources(platform))
            or not isinstance(record['resolved_from'],str) or re.fullmatch(r'[0-9a-f]{40}|[0-9a-f]{64}',record['resolved_from']) is None):
        raise MaterialFailed('APT lock selection differs')
    for key in ('keyring_digest','base_status_digest'):
        if not isinstance(record[key],str) or re.fullmatch('sha256:[0-9a-f]{64}',record[key]) is None:
            raise MaterialFailed('APT trust metadata missing')
    if not isinstance(record['indexes'],list) or not record['indexes']:raise MaterialFailed('APT indexes missing')
    seen=set()
    for item in record['indexes']:
        if (not isinstance(item,dict) or set(item)!={'path','digest','size'} or not item['path'].startswith('lists/')
                or type(item['size']) is not int or not 0<item['size']<=common.MAX_ARCHIVE
                or not isinstance(item['digest'],str) or re.fullmatch('sha256:[0-9a-f]{64}',item['digest']) is None
                or item['path'] in seen):raise MaterialFailed('invalid APT index identity')
        common.safe_path(item['path']);seen.add(item['path'])
    if not any(item['path'].endswith('InRelease') for item in record['indexes']):raise MaterialFailed('signed index identity missing')
    packages=record['packages']
    if not isinstance(packages,list) or not packages or len(packages)>1024:raise MaterialFailed('APT package inventory missing')
    seen=set();archives=set();total=0
    for item in packages:
        if (not isinstance(item,dict) or set(item)!={'name','version','architecture','url','digest','size','archive'}
                or not isinstance(item['name'],str) or re.fullmatch('[a-z0-9][a-z0-9+.-]+',item['name']) is None
                or not isinstance(item['version'],str) or re.fullmatch('[A-Za-z0-9.+:~_-]+',item['version']) is None
                or len(item['version'])>256 or item['architecture'] not in (platform.split('/')[1],'all')
                or not isinstance(item['digest'],str) or re.fullmatch('sha256:[0-9a-f]{64}',item['digest']) is None
                or type(item['size']) is not int or not 8<item['size']<=common.MAX_ARCHIVE
                or item['archive']!='archives/'+item['digest'][7:]+'.deb' or item['name'] in seen or item['archive'] in archives):
            raise MaterialFailed('invalid Debian package identity')
        archive_url(item['url']);seen.add(item['name']);archives.add(item['archive']);total+=item['size']
    if total>common.MAX_TOTAL or not set(REQUESTED[kind])<=seen:raise MaterialFailed('APT closure incomplete or oversized')


def resolve(root,ref,kind,platform,output,*,run=subprocess.run):
    try:
        revision,base,recipe_digest=selection(root,ref,kind,platform)
        output=common.fresh_directory(Path(output).absolute())
        common.write_file(output/'debian.sources',sources(platform))
        command=['docker','run','--rm','--interactive','--network','bridge','--platform',platform,'--mount',
                 f'type=bind,source={output},target=/materials','--entrypoint','/usr/bin/env',base,
                 '-i','PATH=/usr/sbin:/usr/bin:/sbin:/bin','HOME=/tmp','LC_ALL=C','/bin/sh','-se']
        result=run(command,input=resolver_script(kind),capture_output=True,timeout=900)
        if result.returncode:raise MaterialFailed('native APT resolution failed')
        uris={}
        for line in common.read_file(output/'uris.txt',4*1024*1024).decode().splitlines():
            if not line.startswith("'"):continue
            fields=shlex.split(line)
            if len(fields) not in (3,4):raise MaterialFailed('invalid APT archive URI record')
            url,name,size=fields[:3];common.safe_path(name)
            if '/' in name or name in uris:raise MaterialFailed('ambiguous APT archive name')
            parsed=urlsplit(url)
            if parsed.scheme not in ('http','https'):raise MaterialFailed('invalid native archive scheme')
            url='https://'+parsed.netloc+parsed.path
            archive_url(url)
            if parsed.query or parsed.fragment:raise MaterialFailed('ambiguous native archive URI')
            uris[name]=(url,int(size))
        entries=[];archive_data={}
        for file in sorted((output/'archives').glob('*.deb')):
            data=common.read_file(file,common.MAX_ARCHIVE)
            control=paragraphs(common.read_file(output/'control'/(file.name+'.txt'),1024*1024))
            if len(control)!=1:raise MaterialFailed('ambiguous Debian package control')
            control=control[0]
            candidates=[item for item in paragraphs(common.read_file(output/'cache'/(file.name+'.txt'),4*1024*1024))
                        if all(item.get(key)==control.get(key) for key in ('Package','Version','Architecture')) and 'SHA256' in item]
            identities={(item['SHA256'],item['Size'],item['Filename']) for item in candidates}
            if len(identities)!=1 or file.name not in uris:raise MaterialFailed('Debian index metadata differs')
            checksum,size,filename=identities.pop();url,uri_size=uris[file.name]
            if (int(size)!=uri_size or unquote(urlsplit(url).path) not in ('/debian/'+filename,'/debian-security/'+filename)):
                raise MaterialFailed('Debian index path differs')
            entry={'name':control['Package'],'version':control['Version'],'architecture':control['Architecture'],
                   'url':url,'digest':'sha256:'+checksum,'size':int(size),'archive':'archives/'+checksum+'.deb'}
            verify_deb(data,entry);entries.append(entry);archive_data[entry['archive']]=data
        if set(uris)!={file.name for file in (output/'archives').glob('*.deb')}:
            raise MaterialFailed('APT archive closure differs')
        indexes=[]
        for file in sorted((output/'lists').iterdir()):
            if file.name=='lock' or file.is_dir():continue
            data=common.read_file(file,common.MAX_ARCHIVE)
            indexes.append({'path':'lists/'+file.name,'digest':digest(data),'size':len(data)})
        record={'algorithm':ALGORITHM,'resolved_from':revision,'kind':kind,'platform':platform,'base':base,
                'recipe_digest':recipe_digest,'requested':list(REQUESTED[kind]),'sources_digest':digest(sources(platform)),
                'base_status_digest':digest(common.read_file(output/'base-status',16*1024*1024)),
                'keyring_digest':digest(common.read_file(output/'keyring',4*1024*1024)),
                'indexes':indexes,'packages':sorted(entries,key=lambda item:item['name'])}
        validate_record(record,kind,platform,base,recipe_digest)
        for name,data in archive_data.items():common.write_file(output/'bundle'/name,data)
        common.write_file(output/'bundle/manifest.json',canonical(record)+b'\n')
        common.write_file(output/'proposal.json',canonical(record)+b'\n')
        return record
    except Exception:
        raise MaterialFailed('APT material resolution failed') from None


def locked(root,ref,kind,platform):
    revision,base,recipe_digest=selection(root,ref,kind,platform)
    snapshot=GitSnapshot(root,revision)
    lock=common.registry.parse_json(snapshot.read_bytes(LOCK_PATH))
    if not isinstance(lock,dict) or set(lock)!={'algorithm','sets'} or lock['algorithm']!=LOCK_ALGORITHM or not isinstance(lock['sets'],list):
        raise MaterialFailed('invalid committed APT lock')
    seen=set();selected=None
    for record in lock['sets']:
        key=(record['kind'],record['platform'])
        if key in seen:raise MaterialFailed('duplicate APT lock selection')
        seen.add(key)
        _,expected_base,expected_recipe=selection(root,revision,*key)
        validate_record(record,*key,expected_base,expected_recipe)
        if key==(kind,platform):selected=record
    if selected is None:raise MaterialFailed('APT lock selection absent')
    return revision,selected


def fetch(root,ref,kind,platform,output,*,download=None):
    try:
        _,record=locked(root,ref,kind,platform)
        output=common.fresh_directory(output)
        def get(item):
            if download is None:
                parsed=archive_url(item['url']);data=common.http_archive_get('deb.debian.org',parsed.path)
            else:data=download(item['url'])
            verify_deb(data,item);return data
        with ThreadPoolExecutor(max_workers=4) as pool:
            items=iter(record['packages']);pending={}
            def submit():
                item=next(items,None)
                if item is not None:pending[pool.submit(get,item)]=item
            for _ in range(4):submit()
            try:
                while pending:
                    done,_=wait(pending,return_when=FIRST_COMPLETED)
                    for future in done:
                        item=pending.pop(future);common.write_file(output/item['archive'],future.result())
                    for _ in done:submit()
            finally:
                for future in pending:future.cancel()
        common.write_file(output/'manifest.json',canonical(record)+b'\n')
        return {'material_digest':digest(canonical(record)),'package_count':len(record['packages']),
                'identity_verified':False,'publication_authorized':False}
    except Exception:
        raise MaterialFailed('locked APT fetch failed') from None


def installer(record):
    arch=record['platform'].split('/')[1]
    archives=' '.join('/materials/'+item['archive'] for item in record['packages'])
    cache_files='\n'.join('cp /materials/'+item['archive']+' "$cache/archives/'+item['name']+'_'+item['version'].split(':')[-1]+'_'+item['architecture']+'.deb"' for item in record['packages'])
    return f"""#!/bin/sh
set -eu
cd /materials
test "$(dpkg --print-architecture)" = "{arch}"
echo '{record['base_status_digest'][7:]}  /var/lib/dpkg/status' | sha256sum -c - >/dev/null
sha256sum -c checksums.sha256 >/dev/null
cache=$(mktemp -d /tmp/observable-apt.XXXXXXXX)
mkdir -p "$cache/archives/partial" "$cache/lists/partial" "$cache/sources"
touch "$cache/sources.list"
{cache_files}
DEBIAN_FRONTEND=noninteractive apt-get -o Dir::Cache::archives="$cache/archives" -o Dir::State::lists="$cache/lists" -o Dir::Etc::sourcelist="$cache/sources.list" -o Dir::Etc::sourceparts="$cache/sources" -o Dir::Etc::main=- -o Dir::Etc::parts=- --yes --no-download --no-install-recommends install {archives}
""".encode()


def stage(root,ref,kind,platform,bundle,output):
    try:
        revision,record=locked(root,ref,kind,platform)
        bundle=Path(bundle)
        supplied=common.registry.parse_json(common.read_file(bundle/'manifest.json',4*1024*1024))
        if supplied!=record:raise MaterialFailed('APT bundle differs from committed lock')
        data={}
        for item in record['packages']:
            blob=common.read_file(bundle/item['archive'],common.MAX_ARCHIVE);verify_deb(blob,item);data[item['archive']]=blob
        output=common.fresh_directory(output)
        for name,blob in data.items():common.write_file(output/name,blob)
        common.write_file(output/'checksums.sha256',''.join(item['digest'][7:]+'  '+item['archive']+'\n' for item in record['packages']).encode())
        common.write_file(output/'install.sh',installer(record))
        result={'source_revision':revision,'kind':kind,'platform':platform,'base':record['base'],
                'material_digest':digest(canonical(record)),'package_count':len(record['packages']),
                'archive_bytes_verified':True,'identity_verified':False,'publication_authorized':False}
        common.write_file(output/'result.json',canonical(result)+b'\n')
        return result
    except Exception:
        raise MaterialFailed('locked APT staging failed') from None
