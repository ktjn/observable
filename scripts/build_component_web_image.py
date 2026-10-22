"""Build a local web OCI image from verified materials with RUN networking disabled.

The generated offline recipe differs from the original source recipe; neither
local build evidence nor image labels authorize publication.
"""

import argparse
import io
import json
from pathlib import Path
import subprocess
import sys
import tarfile

import build_component_web as web
import collect_component_inputs as inputs
import component_npm_materials as materials
from collect_component_source import ROOT, canonical, digest
from plan_component_release import DIGEST, matches

ALGORITHM = 'offline-web-image-v1'

INSTALL = r"""import fs from 'node:fs';
import {spawnSync} from 'node:child_process';
const base='/tmp/observable-npm';
fs.mkdirSync(base,{recursive:true});
fs.writeFileSync(base+'/user.npmrc','');
fs.writeFileSync(base+'/global.npmrc','');
const env={PATH:'/usr/local/bin:/usr/bin:/bin',HOME:base,TMPDIR:'/tmp'};
const common=['--offline','--include=dev','--ignore-scripts','--no-audit','--no-fund',
 '--logs-max=0','--loglevel=silent','--cache',base+'/cache',
 '--userconfig',base+'/user.npmrc','--globalconfig',base+'/global.npmrc'];
const archives=JSON.parse(fs.readFileSync('/offline/archives.json'));
function execute(args) {
 const result=spawnSync(process.execPath,['/usr/local/lib/node_modules/npm/bin/npm-cli.js',...args,...common],
  {cwd:'/app',env,stdio:'inherit'});
 if(result.error||result.status!==0) throw new Error('offline npm command failed');
}
for(let start=0;start<archives.length;start+=16) execute(['cache','add',...archives.slice(start,start+16)]);
execute(['ci']);
"""

COMPILE = r"""import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {spawnSync} from 'node:child_process';
const expected=JSON.parse(fs.readFileSync('/offline/source.json'));
function verify() {
 for(const [name,hash] of Object.entries(expected)) {
  const file='/app/'+name;
  if(!fs.lstatSync(file).isFile()||crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex')!==hash)
   throw new Error('committed build input changed');
 }
}
verify();
const env={PATH:'/usr/local/bin:/usr/bin:/bin',HOME:'/tmp/observable-npm',TMPDIR:'/tmp',NODE_ENV:'production',
 VITE_OBSERVABLE_SELF_OBSERVABILITY_MODE:process.env.VITE_OBSERVABLE_SELF_OBSERVABILITY_MODE,
 VITE_OBSERVABLE_SELF_OBSERVABILITY_OTLP_ENDPOINT:process.env.VITE_OBSERVABLE_SELF_OBSERVABILITY_OTLP_ENDPOINT};
const tools=JSON.parse(fs.readFileSync('/offline/tools.json'));
for(const tool of tools) {
 const directory='/app/'+tool.directory;
 const metadata=JSON.parse(fs.readFileSync(directory+'/package.json'));
 if(metadata.name!==tool.name) throw new Error('installed compiler differs');
 const relative=metadata.bin[tool.binary].replace(/^\.\//,'');
 if(!relative||relative.startsWith('/')||/[\\:]/.test(relative)||relative.split('/').some(p=>!p||p==='.'||p==='..'))
  throw new Error('unsafe compiler path');
 const binary=path.join(directory,relative);
 if(!fs.lstatSync(binary).isFile()) throw new Error('unsafe compiler file');
 const result=spawnSync(process.execPath,[binary,...tool.args],{cwd:'/app/apps/frontend',env,stdio:'inherit'});
 if(result.error||result.status!==0) throw new Error('production compiler failed');
}
verify();
const directory='/app/apps/frontend/dist';
let count=0,total=0,hasIndex=false,hasJs=false;
function inspect(dir) {
 for(const name of fs.readdirSync(dir).sort()) {
  const file=path.join(dir,name), stat=fs.lstatSync(file);
  if(stat.isSymbolicLink()) throw new Error('artifact link rejected');
  if(stat.isDirectory()) {inspect(file);continue;}
  if(!stat.isFile()) throw new Error('special artifact rejected');
  total+=stat.size;count++;
  if(total>536870912||count>100000) throw new Error('artifact bound exceeded');
  if(stat.size&&file===directory+'/index.html') hasIndex=true;
  if(stat.size&&file.endsWith('.js')) hasJs=true;
 }
}
if(!fs.lstatSync(directory).isDirectory()) throw new Error('static output missing');
inspect(directory);
if(!hasIndex||!hasJs) throw new Error('production assets missing');
"""


class BuildFailed(ValueError):
    """No verified local offline image result is available."""


def recipe(evidence):
    images = evidence['images']
    node = next(item['reference'] for item in images if item['stages'] == ['deps'])
    nginx = next(item['reference'] for item in images if item['stages'] == ['runtime'])
    return f"""# syntax={evidence['recipe']['frontend']['reference']}
FROM {node} AS build
WORKDIR /app
COPY workspace/ /app/
COPY archives/ /materials/
COPY offline/ /offline/
RUN --network=none node /offline/install.mjs
ARG VITE_OBSERVABLE_SELF_OBSERVABILITY_MODE=self
ARG VITE_OBSERVABLE_SELF_OBSERVABILITY_OTLP_ENDPOINT=
ENV VITE_OBSERVABLE_SELF_OBSERVABILITY_MODE=$VITE_OBSERVABLE_SELF_OBSERVABILITY_MODE
ENV VITE_OBSERVABLE_SELF_OBSERVABILITY_OTLP_ENDPOINT=$VITE_OBSERVABLE_SELF_OBSERVABILITY_OTLP_ENDPOINT
RUN --network=none node /offline/compile.mjs
FROM {nginx} AS runtime
RUN --network=none rm -rf /usr/share/nginx/html/*
COPY --from=build /app/apps/frontend/dist /usr/share/nginx/html
COPY workspace/apps/frontend/nginx.conf /etc/nginx/conf.d/default.conf
EXPOSE 80
CMD ["nginx","-g","daemon off;"]
""".encode()


def context_bytes(files):
    result = io.BytesIO()
    with tarfile.open(fileobj=result, mode='w', format=tarfile.PAX_FORMAT) as archive:
        for name, data in sorted(files.items()):
            materials.safe_path(name)
            item = tarfile.TarInfo(name)
            item.size = len(data); item.mode = 0o644; item.mtime = item.uid = item.gid = 0
            item.uname = item.gname = ''
            archive.addfile(item, io.BytesIO(data))
    return result.getvalue()


def labels(request):
    return {'org.opencontainers.image.revision':request['source_revision'],
            'io.observable.offline.algorithm':ALGORITHM,
            'io.observable.source.digest':request['source_digest'],
            'io.observable.material.digest':request['material_digest'],
            'io.observable.offline.recipe.digest':request['recipe_digest'],
            'io.observable.offline.context.digest':request['context_digest']}


def build(root, ref, bundle, output, *, platform='linux/amd64', build_args=None, run=subprocess.run):
    try:
        if platform not in ('linux/amd64', 'linux/arm64'):
            raise BuildFailed('unsupported build platform')
        revision, metadata, plan, source, arguments, selected = web.prepare_source(root, ref, build_args)
        if 'apps/frontend/nginx.conf' not in selected:
            raise BuildFailed('runtime config missing')
        evidence = inputs.resolve(root, 'web', revision, (platform,), build_args=build_args)
        generated = recipe(evidence)
        output = materials.fresh_directory(Path(output).absolute())
        _, _, manifest = materials.stage_bundle(root, revision, bundle, output / 'materials')
        files = {'workspace/' + name:data for name,data in (metadata | selected).items()}
        for entry in manifest['archives']:
            data = materials.read_file(output / 'materials' / entry['path'], materials.MAX_ARCHIVE)
            # Verify the exact immutable bytes later sent to Docker, not only the staged file.
            materials.verify_archive(data, entry['integrity'])
            if len(data) != entry['size']:
                raise BuildFailed('staged archive size changed')
            files[entry['path']] = data
        tools = []
        for name,binary,args in (('typescript','tsc',[]),('vite','vite',['build','--mode','production'])):
            directory = next((path for path in ('apps/frontend/node_modules/'+name,'node_modules/'+name)
                              if any(item['path']==path for item in plan['packages'])), None)
            if directory is None:
                raise BuildFailed('locked compiler missing')
            tools.append({'name':name,'binary':binary,'directory':directory,'args':args})
        files |= {'Dockerfile.offline':generated, '.dockerignore':b'',
                  'offline/install.mjs':INSTALL.encode(), 'offline/compile.mjs':COMPILE.encode(),
                  'offline/archives.json':canonical(['/materials/'+Path(entry['path']).name for entry in manifest['archives']]),
                  'offline/tools.json':canonical(tools),
                  'offline/source.json':canonical({name:digest(data)[7:] for name,data in (metadata | selected).items()})}
        context = context_bytes(files)
        request = {'algorithm':ALGORITHM,'source_revision':revision,
                   'source_algorithm':source['source_algorithm'],'source_digest':source['source_digest'],
                   'declared_input_digest':evidence['input_digest'],'material_digest':manifest['material_digest'],
                   'recipe_digest':digest(generated),'context_digest':digest(context),
                   'platform':platform,'build_args':arguments}
        materials.write_file(output / 'request.json', canonical(request)+b'\n')
        materials.write_file(output / 'context.tar', context)
        command = ['docker','buildx','build','--load','--network','none','--no-cache','--platform',platform,
                   '--metadata-file',str(output/'metadata.json'),'--iidfile',str(output/'image-id'),
                   '--file','Dockerfile.offline']
        for key,value in sorted(arguments.items()):
            command += ['--build-arg',key+'='+value]
        for key,value in labels(request).items():
            command += ['--label',key+'='+value]
        command.append('-')
        built = run(command,input=context,capture_output=True,timeout=1800)
        if built.returncode:
            raise BuildFailed('offline Docker build failed')
        record = materials.registry.parse_json(materials.read_file(output/'metadata.json',4*1024*1024))
        image_digest, config_digest = record.get('containerimage.digest'), record.get('containerimage.config.digest')
        image_id = materials.read_file(output/'image-id',256).decode().strip()
        if (not matches(DIGEST,image_digest) or not matches(DIGEST,image_id)
                or image_id not in (image_digest,config_digest)):
            raise BuildFailed('invalid Docker image metadata')
        inspected = run(['docker','image','inspect',image_id],capture_output=True,timeout=60)
        if inspected.returncode:
            raise BuildFailed('built image inspection failed')
        records = materials.registry.parse_json(inspected.stdout)
        if not isinstance(records,list) or len(records)!=1:
            raise BuildFailed('ambiguous built image')
        image = records[0]
        if 'Descriptor' in image:
            if image['Descriptor']['digest']!=image_digest or image['Id']!=image_digest:
                raise BuildFailed('built root digest differs')
        elif not matches(DIGEST,config_digest) or image['Id']!=config_digest:
            raise BuildFailed('built config digest differs')
        if (image_id!=image['Id'] or image['Os']+'/'+image['Architecture']!=platform
                or any(image['Config']['Labels'].get(key)!=value for key,value in labels(request).items())):
            raise BuildFailed('built image evidence differs')
        result = request | {'image_id':image_id,'image_digest':image_digest,
                            'run_network_disabled':True,'identity_verified':False,'publication_authorized':False}
        materials.write_file(output/'result.json',canonical(result)+b'\n')
        return result
    except Exception:
        raise BuildFailed('offline web image build failed; no success record written') from None


def main(argv):
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo',type=Path,default=ROOT)
    parser.add_argument('--ref',default='HEAD')
    parser.add_argument('--bundle',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--platform',choices=('linux/amd64','linux/arm64'),default='linux/amd64')
    parser.add_argument('--build-arg',action='append',default=[])
    args=parser.parse_args(argv)
    try:
        result=build(args.repo,args.ref,args.bundle,args.output,platform=args.platform,
                     build_args=inputs.parse_build_args(args.build_arg))
    except (BuildFailed,ValueError):
        print('offline web image build failed',file=sys.stderr)
        return 1
    print(json.dumps(result,sort_keys=True))
    return 0


if __name__=='__main__':
    sys.exit(main(sys.argv[1:]))
