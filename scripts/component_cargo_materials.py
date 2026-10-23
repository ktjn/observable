"""Verify committed Cargo archives and reconstruct offline directory sources."""

from concurrent.futures import ThreadPoolExecutor, wait, FIRST_COMPLETED
import gzip
import hashlib
from http.client import HTTPSConnection
import io
from pathlib import Path, PurePosixPath
import re
import tarfile
import tomllib
from urllib.parse import urlsplit

from ci_changed_components import load_workspace
from collect_component_source import GitSnapshot, canonical, digest
import component_npm_materials as common

ALGORITHM = 'cargo-materials-v1'
REGISTRY = 'registry+https://github.com/rust-lang/crates.io-index'
CONFIG = b'[source.crates-io]\nreplace-with = "locked-vendor"\n[source.locked-vendor]\ndirectory = "../vendor"\n[net]\noffline = true\n'
MAX_ARCHIVE = common.MAX_ARCHIVE
MAX_EXPANDED = common.MAX_EXPANDED
MAX_TOTAL = common.MAX_TOTAL
MaterialFailed = common.MaterialFailed


def normalized(base, path):
    if not isinstance(path,str) or path.startswith('/') or '\\' in path or ':' in path:
        raise MaterialFailed('unsafe Cargo path')
    parts=[]
    for part in PurePosixPath(base,path).parts:
        if part=='..':
            if not parts: raise MaterialFailed('escaping Cargo path')
            parts.pop()
        elif part not in ('','.'):
            parts.append(part)
    return '/'.join(parts)


def validate_dependency(spec, origin, members):
    if not isinstance(spec,dict):return
    if any(key in spec for key in ('git','registry','registry-index')):
        raise MaterialFailed('non-crates.io dependency requires review')
    if 'path' in spec and normalized(origin,spec['path']) not in members:
        raise MaterialFailed('unresolved local Cargo dependency')


def source_plan(root, ref):
    snapshot=GitSnapshot(root,ref)
    root_manifest=tomllib.loads(snapshot.read_bytes('Cargo.toml').decode())
    graph=load_workspace(root,lambda path:snapshot.read_bytes(path).decode(),strict=True)
    members={member for member,_ in graph.values()}
    metadata={'Cargo.toml':snapshot.read_bytes('Cargo.toml'),'Cargo.lock':snapshot.read_bytes('Cargo.lock')}
    for member in sorted(members):metadata[member+'/Cargo.toml']=snapshot.read_bytes(member+'/Cargo.toml')
    workspace_deps=root_manifest['workspace'].get('dependencies',{})
    for spec in workspace_deps.values():validate_dependency(spec,'',members)
    local_versions={}
    for name,data in metadata.items():
        if not name.endswith('Cargo.toml'):continue
        manifest=tomllib.loads(data.decode())
        if 'patch' in manifest or 'replace' in manifest:raise MaterialFailed('Cargo overrides require review')
        base='' if name=='Cargo.toml' else str(PurePosixPath(name).parent)
        if base:
            version=manifest['package']['version']
            if isinstance(version,dict) and version.get('workspace') is True:
                version=root_manifest['workspace']['package']['version']
            if not isinstance(version,str):raise MaterialFailed('unsupported workspace version')
            local_versions[manifest['package']['name']]=version
        tables=[manifest]
        tables.extend(manifest.get('target',{}).values())
        for table in tables:
            for kind in ('dependencies','dev-dependencies','build-dependencies'):
                for dep_name,spec in table.get(kind,{}).items():
                    origin=base
                    if isinstance(spec,dict) and spec.get('workspace'):
                        spec=workspace_deps[dep_name];origin=''
                    validate_dependency(spec,origin,members)
        for target in [manifest.get('lib',{}),*manifest.get('bin',[]),*manifest.get('test',[]),*manifest.get('bench',[]),*manifest.get('example',[])]:
            if 'path' in target and not normalized(base,target['path']).startswith(base+'/'):
                raise MaterialFailed('escaping Cargo target')
    lock=tomllib.loads(metadata['Cargo.lock'].decode())
    if lock.get('version')!=4 or not isinstance(lock.get('package'),list):raise MaterialFailed('unsupported Cargo lock')
    packages=[];seen=set();local={}
    for item in lock['package']:
        name,version=item['name'],item['version']
        if (not isinstance(name,str) or re.fullmatch(r'[A-Za-z0-9_][A-Za-z0-9_-]*',name) is None
                or not isinstance(version,str) or re.fullmatch(r'[0-9]+\.[0-9]+\.[0-9]+(?:[-+][A-Za-z0-9.+-]+)?',version) is None
                or (name,version) in seen):raise MaterialFailed('invalid Cargo package')
        seen.add((name,version))
        if 'source' not in item:
            if name in local:raise MaterialFailed('duplicate local Cargo package')
            local[name]=version;continue
        checksum=item.get('checksum')
        if item['source']!=REGISTRY or not isinstance(checksum,str) or re.fullmatch('[0-9a-f]{64}',checksum) is None:
            raise MaterialFailed('unsupported Cargo source or checksum')
        packages.append({'name':name,'version':version,'checksum':checksum,'archive':'archives/'+checksum+'.crate',
                         'url':f'https://static.crates.io/crates/{name}/{name}-{version}.crate'})
    if not packages or local!=local_versions:raise MaterialFailed('Cargo lock workspace differs')
    plan={'algorithm':ALGORITHM,'files':[{'path':name,'digest':digest(data)} for name,data in sorted(metadata.items())],
          'packages':sorted(packages,key=lambda item:(item['name'],item['version']))}
    # Reject configuration/toolchain injection and nonregular files before any network or output.
    workspace_files(snapshot,members)
    return snapshot.revision,metadata,plan


def workspace_files(snapshot, members):
    files={}
    for name in snapshot.entries:
        if name.startswith(('.cargo/','rust-toolchain')):
            raise MaterialFailed('Cargo configuration/toolchain requires review')
        if name not in ('Cargo.toml','Cargo.lock') and not name.startswith(tuple(member+'/' for member in members)+('proto/','contracts/')):
            continue
        common.safe_path(name)
        parts=PurePosixPath(name).parts
        if any(part.lower() in ('.cargo','target','.git','vendor') or part.lower().startswith(('.env','rust-toolchain')) for part in parts):
            raise MaterialFailed('unsupported Cargo workspace configuration or artifact')
        if name.endswith('/build.rs'):raise MaterialFailed('local build script requires review')
        files[name]=snapshot.read_bytes(name)
    return files


def http_get(url, *, connection_factory=HTTPSConnection):
    parsed=urlsplit(url)
    if (parsed.scheme!='https' or parsed.netloc!='static.crates.io' or parsed.query or parsed.fragment
            or re.fullmatch(r'/crates/[A-Za-z0-9_-]+/[A-Za-z0-9_.+-]+\.crate',parsed.path) is None):
        raise MaterialFailed('unsupported Cargo archive origin')
    return common.http_archive_get('static.crates.io',parsed.path,connection_factory=connection_factory)


def archive_files(data, package):
    try:
        return _archive_files(data, package)
    except Exception:
        raise MaterialFailed('Cargo archive verification failed') from None


def _archive_files(data, package):
    if not isinstance(data,bytes) or len(data)>MAX_ARCHIVE or hashlib.sha256(data).hexdigest()!=package['checksum']:
        raise MaterialFailed('Cargo archive checksum differs')
    with gzip.GzipFile(fileobj=io.BytesIO(data)) as compressed:expanded=compressed.read(MAX_EXPANDED+1)
    if len(expanded)>MAX_EXPANDED:raise MaterialFailed('Cargo archive expansion exceeded')
    prefix=package['name']+'-'+package['version'];seen=set();files={};total=0;end=0
    with tarfile.open(fileobj=io.BytesIO(expanded),mode='r:') as archive:
        for member in archive:
            path=member.name.rstrip('/') if member.isdir() else member.name
            common.safe_path(path)
            if (path in seen or len(seen)>=100000 or not (member.isfile() or member.isdir())
                    or (path!=prefix and not path.startswith(prefix+'/'))):raise MaterialFailed('unsafe Cargo archive layout')
            seen.add(path);total+=member.size
            if total>MAX_EXPANDED or member.size<0:raise MaterialFailed('Cargo archive expansion exceeded')
            if member.isfile():
                relative=path[len(prefix)+1:];common.safe_path(relative)
                if relative=='.cargo-checksum.json':raise MaterialFailed('reserved Cargo checksum file')
                files[relative]={'data':archive.extractfile(member).read(),'executable':bool(member.mode&0o111)}
            end=member.offset_data+((member.size+511)//512)*512
    if 'Cargo.toml' not in files or any(expanded[end:]):raise MaterialFailed('Cargo archive metadata missing')
    return files


def materialize(root, ref, output, *, fetch=http_get):
    try:
        revision,metadata,plan=source_plan(root,ref)
        output=common.fresh_directory(output)
        for name,data in metadata.items():common.write_file(output/'sources'/name,data)
        verified={};total=0
        def download(package):
            data=fetch(package['url']);archive_files(data,package);return data
        with ThreadPoolExecutor(max_workers=4) as pool:
            entries=iter(plan['packages']);pending={}
            def submit():
                entry=next(entries,None)
                if entry is not None:pending[pool.submit(download,entry)]=entry
            for _ in range(4):submit()
            try:
                while pending:
                    done,_=wait(pending,return_when=FIRST_COMPLETED)
                    for future in done:
                        entry=pending.pop(future);data=future.result();total+=len(data)
                        if total>MAX_TOTAL:raise MaterialFailed('Cargo bundle too large')
                        common.write_file(output/entry['archive'],data)
                        verified[(entry['name'],entry['version'])]=entry|{'size':len(data)}
                    for _ in done:submit()
            finally:
                for future in pending:future.cancel()
        payload=plan|{'packages':[verified[(p['name'],p['version'])] for p in plan['packages']]}
        result=payload|{'source_revision':revision,'material_digest':digest(canonical(payload)),
                        'material_bytes_verified':True,'identity_verified':False}
        common.write_file(output/'manifest.json',canonical(result)+b'\n')
        return result
    except Exception:
        raise MaterialFailed('Cargo material preparation failed') from None


def stage(root, ref, bundle, output):
    try:
        revision,metadata,plan=source_plan(root,ref)
        bundle=Path(bundle)
        record=common.registry.parse_json(common.read_file(bundle/'manifest.json',4*1024*1024))
        if (not isinstance(record,dict) or set(record)!=set(plan)|{'source_revision','material_digest','material_bytes_verified','identity_verified'}
                or record['material_bytes_verified'] is not True or record['identity_verified'] is not False
                or not isinstance(record['source_revision'],str) or re.fullmatch(r'[0-9a-f]{40}|[0-9a-f]{64}',record['source_revision']) is None
                or any(record[key]!=plan[key] for key in ('algorithm','files'))
                or not isinstance(record['packages'],list) or len(record['packages'])!=len(plan['packages'])):
            raise MaterialFailed('Cargo bundle metadata differs')
        for entry,expected in zip(record['packages'],plan['packages']):
            if (not isinstance(entry,dict) or set(entry)!=set(expected)|{'size'} or any(entry[k]!=v for k,v in expected.items())
                    or type(entry['size']) is not int or not 0<=entry['size']<=MAX_ARCHIVE):raise MaterialFailed('Cargo archive descriptor differs')
        payload={key:record[key] for key in plan}
        if record['material_digest']!=digest(canonical(payload)):raise MaterialFailed('Cargo material digest differs')
        for name,data in metadata.items():
            if common.read_file(bundle/'sources'/name,4*1024*1024)!=data:raise MaterialFailed('Cargo metadata bytes differ')
        snapshot=GitSnapshot(root,revision)
        graph=load_workspace(root,lambda p:snapshot.read_bytes(p).decode(),strict=True)
        source=workspace_files(snapshot,{member for member,_ in graph.values()})
        output=common.fresh_directory(output)
        for name,data in source.items():common.write_file(output/'workspace'/name,data)
        common.write_file(output/'workspace/.cargo/config.toml',CONFIG)
        total=0;expanded=0
        for entry in record['packages']:
            data=common.read_file(bundle/entry['archive'],MAX_ARCHIVE);total+=len(data)
            if len(data)!=entry['size'] or total>MAX_TOTAL:raise MaterialFailed('Cargo archive size differs')
            files=archive_files(data,entry)
            directory=output/'vendor'/(entry['name']+'-'+entry['version'])
            checksums={}
            for name,item in sorted(files.items()):
                expanded+=len(item['data'])
                if expanded>MAX_TOTAL:raise MaterialFailed('Cargo vendor output too large')
                target=directory/name;common.write_file(target,item['data'])
                if item['executable']:target.chmod(0o755)
                checksums[name]=hashlib.sha256(item['data']).hexdigest()
            common.write_file(directory/'.cargo-checksum.json',canonical({'package':entry['checksum'],'files':checksums})+b'\n')
        result={'source_revision':revision,'material_digest':record['material_digest'],'package_count':len(record['packages']),
                'workspace_digest':digest(canonical({name:digest(data) for name,data in sorted(source.items())})),
                'directory_source_verified':True,'identity_verified':False,'publication_authorized':False}
        common.write_file(output/'result.json',canonical(result)+b'\n')
        return result
    except Exception:
        raise MaterialFailed('Cargo directory-source staging failed') from None
