"""Build local Rust OCI images from Git-approved Cargo/APT bytes without RUN networking."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import subprocess
import sys
import tarfile

import collect_component_inputs as inputs
from collect_component_source import ROOT, GitSnapshot, canonical, digest
import component_cargo_materials as cargo
import component_apt_materials as apt
import component_npm_materials as common
from ci_changed_components import load_workspace
from plan_component_release import DIGEST, matches

ALGORITHM='offline-rust-image-v1'


class BuildFailed(ValueError):
    """No verified local image result is available."""


def recipe(evidence,service):
    builder=next(i['reference'] for i in evidence['images'] if i['stages']==['chef'])
    runtime=next(i['reference'] for i in evidence['images'] if i['stages']==['component-runtime'])
    return f"""# syntax={evidence['recipe']['frontend']['reference']}
FROM {builder} AS build
COPY builder-materials/ /materials/
RUN --network=none /usr/bin/env -i PATH=/usr/sbin:/usr/bin:/sbin:/bin HOME=/tmp LC_ALL=C /bin/sh /materials/install.sh
WORKDIR /app
COPY workspace/ /app/
COPY vendor/ /vendor/
COPY offline/ /offline/
RUN --network=none /usr/bin/env -i PATH=/usr/local/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin RUSTUP_HOME=/usr/local/rustup CARGO_HOME=/tmp/cargo-home CARGO_TARGET_DIR=/tmp/target HOME=/tmp/cargo-home LC_ALL=C /bin/sh /offline/compile.sh
FROM {runtime} AS runtime
COPY runtime-materials/ /materials/
RUN --network=none /usr/bin/env -i PATH=/usr/sbin:/usr/bin:/sbin:/bin HOME=/tmp LC_ALL=C /bin/sh /materials/install.sh
ENV SERVICE={service}
COPY --from=build /output/{service} /usr/local/bin/{service}
COPY workspace/proto/otlp /proto/otlp
USER 65532:65532
ENTRYPOINT ["/bin/sh", "-c", "exec /usr/local/bin/$SERVICE"]
""".encode()


def context_bytes(files):
    out=io.BytesIO()
    with tarfile.open(fileobj=out,mode='w',format=tarfile.PAX_FORMAT) as archive:
        for name,(data,executable) in sorted(files.items()):
            common.safe_path(name)
            item=tarfile.TarInfo(name);item.size=len(data);item.mode=0o755 if executable else 0o644
            item.mtime=item.uid=item.gid=0;item.uname=item.gname=''
            archive.addfile(item,io.BytesIO(data))
    return out.getvalue()


def labels(request):
    return {'org.opencontainers.image.revision':request['source_revision'],
            'io.observable.offline.algorithm':ALGORITHM,'io.observable.source.digest':request['source_digest'],
            'io.observable.material.digest':request['material_digest'],
            'io.observable.offline.recipe.digest':request['recipe_digest'],
            'io.observable.offline.context.digest':request['context_digest']}


def build(root,component,ref,cargo_bundle,builder_bundle,runtime_bundle,output,*,platform='linux/amd64',run=subprocess.run):
    try:
        if platform not in ('linux/amd64','linux/arm64') or component not in inputs.RUST_COMPONENTS:
            raise BuildFailed('unsupported Rust selection')
        evidence=inputs.resolve(root,component,ref,(platform,));revision=evidence['source_revision']
        service=evidence['build_args']['SERVICE'];snapshot=GitSnapshot(root,revision)
        output=common.fresh_directory(Path(output).absolute())
        staged=cargo.stage(root,revision,cargo_bundle,output/'cargo')
        graph=load_workspace(root,lambda p:snapshot.read_bytes(p).decode(),strict=True)
        source=cargo.workspace_files(snapshot,{member for member,_ in graph.values()})
        if not any(p.startswith('proto/otlp/') for p in source):raise BuildFailed('runtime proto missing')
        files={'workspace/'+p:(data,snapshot.entries[p][0]=='100755') for p,data in source.items()}
        files['workspace/.cargo/config.toml']=(cargo.CONFIG,False)
        # Metadata was checked by stage; archive bytes below are independently verified immediately
        # before constructing the immutable tar, preserving vendor executable bits.
        _,_,plan=cargo.source_plan(root,revision)
        total=0
        for entry in plan['packages']:
            blob=common.read_file(Path(cargo_bundle)/entry['archive'],cargo.MAX_ARCHIVE)
            extracted=cargo.archive_files(blob,entry);checksums={}
            prefix='vendor/'+entry['name']+'-'+entry['version']+'/'
            for name,item in extracted.items():
                total+=len(item['data'])
                if total>cargo.MAX_TOTAL:raise BuildFailed('vendor context too large')
                files[prefix+name]=(item['data'],item['executable'])
                checksums[name]=hashlib.sha256(item['data']).hexdigest()
            files[prefix+'.cargo-checksum.json']=(canonical({'package':entry['checksum'],'files':checksums})+b'\n',False)
        apt_digests={}
        for kind,bundle in (('builder',builder_bundle),('runtime',runtime_bundle)):
            apt.stage(root,revision,kind,platform,bundle,output/kind)
            _,record=apt.locked(root,revision,kind,platform);apt_digests[kind]=digest(canonical(record))
            for item in record['packages']:
                blob=common.read_file(Path(bundle)/item['archive'],common.MAX_ARCHIVE);apt.verify_deb(blob,item)
                files[kind+'-materials/'+item['archive']]=(blob,False)
            checksums=''.join(p['digest'][7:]+'  '+p['archive']+'\n' for p in record['packages'])
            files[kind+'-materials/checksums.sha256']=(checksums.encode(),False)
            files[kind+'-materials/install.sh']=(apt.installer(record),False)
        generated=recipe(evidence,service)
        compile_script=f"""set -eu
mkdir -p /tmp/cargo-home /output
cargo build --frozen --release --package {service} --jobs 2
cp /tmp/target/release/{service} /output/{service}
ldd /output/{service} > /output/linkage.txt
! grep -q 'not found' /output/linkage.txt
""".encode()
        files|={'Dockerfile.offline':(generated,False),'.dockerignore':(b'',False),'offline/compile.sh':(compile_script,False)}
        context=context_bytes(files)
        request={'algorithm':ALGORITHM,'component':component,'source_revision':revision,'source_digest':evidence['source_digest'],
                 'workspace_digest':staged['workspace_digest'],'declared_input_digest':evidence['input_digest'],
                 'cargo_material_digest':staged['material_digest'],'apt_material_digests':apt_digests,
                 'material_digest':digest(canonical({'cargo':staged['material_digest'],'apt':apt_digests})),
                 'recipe_digest':digest(generated),'context_digest':digest(context),'platform':platform,'service':service}
        common.write_file(output/'request.json',canonical(request)+b'\n');common.write_file(output/'context.tar',context)
        command=['docker','buildx','build','--load','--network','none','--no-cache','--platform',platform,
                 '--metadata-file',str(output/'metadata.json'),'--iidfile',str(output/'image-id'),'--file','Dockerfile.offline']
        for key,value in labels(request).items():command+=['--label',key+'='+value]
        command.append('-')
        built=run(command,input=context,capture_output=True,timeout=3600)
        if built.returncode:raise BuildFailed('offline Docker build failed')
        record=common.registry.parse_json(common.read_file(output/'metadata.json',4*1024*1024))
        image_digest=record.get('containerimage.digest');config_digest=record.get('containerimage.config.digest')
        identifier=common.read_file(output/'image-id',256).decode().strip()
        if not matches(DIGEST,image_digest) or not matches(DIGEST,identifier) or identifier not in (image_digest,config_digest):
            raise BuildFailed('invalid image metadata')
        inspected=run(['docker','image','inspect',identifier],capture_output=True,timeout=60)
        if inspected.returncode:raise BuildFailed('image inspection failed')
        records=common.registry.parse_json(inspected.stdout)
        if not isinstance(records,list) or len(records)!=1:raise BuildFailed('ambiguous image')
        image=records[0];config=image['Config']
        if 'Descriptor' in image:
            if image['Descriptor']['digest']!=image_digest or image['Id']!=image_digest:raise BuildFailed('image root differs')
        elif not matches(DIGEST,config_digest) or image['Id']!=config_digest:raise BuildFailed('config digest differs')
        if (identifier!=image['Id'] or image['Os']+'/'+image['Architecture']!=platform
                or any(config['Labels'].get(k)!=v for k,v in labels(request).items())
                or config['User']!='65532:65532' or config['Entrypoint']!=['/bin/sh','-c','exec /usr/local/bin/$SERVICE']
                or [v for v in config['Env'] if v.startswith('SERVICE=')]!=['SERVICE='+service]):
            raise BuildFailed('built runtime contract differs')
        result=request|{'image_id':identifier,'image_digest':image_digest,'run_network_disabled':True,
                        'identity_verified':False,'publication_authorized':False}
        common.write_file(output/'result.json',canonical(result)+b'\n');return result
    except Exception:
        raise BuildFailed('offline Rust image build failed; no success record written') from None


def main(argv):
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('component',choices=tuple(inputs.RUST_COMPONENTS))
    parser.add_argument('--repo',type=Path,default=ROOT);parser.add_argument('--ref',default='HEAD')
    for name in ('cargo-bundle','builder-bundle','runtime-bundle','output'):parser.add_argument('--'+name,type=Path,required=True)
    parser.add_argument('--platform',choices=('linux/amd64','linux/arm64'),default='linux/amd64')
    args=parser.parse_args(argv)
    try:result=build(args.repo,args.component,args.ref,args.cargo_bundle,args.builder_bundle,args.runtime_bundle,args.output,platform=args.platform)
    except BuildFailed:print('offline Rust image build failed',file=sys.stderr);return 1
    print(json.dumps(result,sort_keys=True));return 0


if __name__=='__main__':sys.exit(main(sys.argv[1:]))
