"""Compile committed web source using verified npm materials, without publishing.

Native execution is not network isolated and does not verify OCI build identity.
"""

import argparse
import json
from pathlib import Path
import stat
import subprocess
import sys

import component_npm_materials as materials
import collect_component_inputs as inputs
from collect_component_source import ROOT, GitSnapshot, canonical, collect, digest
import prepare_component_npm as npm

ALGORITHM = 'native-web-build-v1'


class BuildFailed(ValueError):
    """No verified native compilation result is available."""


def entrypoint(workspace, packages, name, binary):
    # Match Node's nearest dependency resolution, including workspace-local overrides.
    locations = ('apps/frontend/node_modules/' + name, 'node_modules/' + name)
    package = next((path for path in locations if any(item['path'] == path for item in packages)), None)
    if package is None:
        raise BuildFailed('compiler absent from locked dependency inventory')
    directory = workspace / package
    metadata = materials.registry.parse_json(materials.read_file(directory / 'package.json', 1024 * 1024))
    if metadata['name'] != name:
        raise BuildFailed('unexpected installed compiler')
    path = metadata['bin'][binary]
    if path.startswith('./'):
        path = path[2:]
    materials.safe_path(path)
    target = directory / path
    materials.read_file(target, 16 * 1024 * 1024)
    return target


def artifact_inventory(directory):
    if directory.is_symlink() or not directory.is_dir():
        raise BuildFailed('build output missing')
    result, total = [], 0
    for path in sorted(directory.rglob('*')):
        if path.is_symlink():
            raise BuildFailed('unsupported artifact link')
        if path.is_dir():
            continue
        if not stat.S_ISREG(path.stat().st_mode):
            raise BuildFailed('unsupported artifact file')
        relative = path.relative_to(directory).as_posix()
        materials.safe_path(relative)
        data = materials.read_file(path, materials.MAX_EXPANDED)
        total += len(data)
        if total > materials.MAX_EXPANDED or len(result) >= 100000:
            raise BuildFailed('artifact inventory too large')
        result.append({'path': relative, 'size': len(data), 'digest': digest(data)})
    paths = {item['path'] for item in result if item['size'] > 0}
    if 'index.html' not in paths or not any(path.endswith('.js') for path in paths):
        raise BuildFailed('production static assets missing')
    return result


def prepare_source(root, ref, build_args=None):
    """Review committed web inputs without touching output or invoking tools."""
    snapshot = GitSnapshot(root, ref)
    revision, metadata, plan = materials.source_plan(root, snapshot.revision)
    source = collect(root, 'web', revision)
    recipe = snapshot.read_bytes('apps/frontend/Dockerfile').decode('utf-8')
    arguments = inputs.effective_build_args('web', recipe, build_args)
    package = materials.registry.parse_json(metadata['apps/frontend/package.json'])
    if package.get('scripts', {}).get('build') != 'tsc && vite build':
        raise BuildFailed('production build script requires review')
    selected = {}
    for item in source['files']:
        name = item['path']
        if not name.startswith('apps/frontend/') or name == 'apps/frontend/package.json':
            continue
        materials.safe_path(name)
        parts = Path(name).parts[2:]
        if any(part.lower() in ('node_modules', 'dist', '.npmrc') for part in parts):
            raise BuildFailed('unsupported committed build artifacts or configuration')
        env_files = [part for part in parts if part.lower().startswith('.env')]
        if env_files:
            if len(parts) == 1 and parts[0] in ('.env.playground', '.env.example'):
                continue
            raise BuildFailed('production environment files require review')
        selected[name] = snapshot.read_bytes(name)
    return revision, metadata, plan, source, arguments, selected


def build(root, ref, bundle, output, *, build_args=None, run=subprocess.run):
    try:
        revision, metadata, plan, source, arguments, selected = prepare_source(root, ref, build_args)
        output = materials.fresh_directory(Path(output).absolute())
        installation = npm.install(root, revision, bundle, output / 'dependencies', run=run)
        workspace = output / 'dependencies/workspace'
        for name, data in selected.items():
            materials.write_file(workspace / name, data)
        node = npm.npm_command()[0]
        env = npm.child_environment(node) | arguments | {'NODE_ENV': 'production'}
        commands = [
            [node, str(entrypoint(workspace, plan['packages'], 'typescript', 'tsc'))],
            [node, str(entrypoint(workspace, plan['packages'], 'vite', 'vite')), 'build', '--mode', 'production'],
        ]
        for command in commands:
            result = run(command, cwd=workspace / 'apps/frontend', env=env,
                         stdin=subprocess.DEVNULL, capture_output=True, timeout=600)
            if result.returncode:
                raise BuildFailed('production compiler failed')
        for name, data in (metadata | selected).items():
            if (workspace / name).read_bytes() != data:
                raise BuildFailed('committed build source changed')
        artifacts = artifact_inventory(workspace / 'apps/frontend/dist')
        result = {'algorithm': ALGORITHM, 'source_revision': revision,
                  'source_algorithm': source['source_algorithm'], 'source_digest': source['source_digest'],
                  'material_digest': installation['material_digest'], 'host_platform': installation['host_platform'],
                  'build_args': arguments, 'artifacts': artifacts, 'artifact_digest': digest(canonical(artifacts)),
                  'compilation_verified': True, 'network_isolation_verified': False,
                  'identity_verified': False, 'publication_authorized': False}
        materials.write_file(output / 'result.json', canonical(result) + b'\n')
        return result
    except Exception:
        raise BuildFailed('committed web compilation failed; no build success record written') from None


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', type=Path, default=ROOT)
    parser.add_argument('--ref', default='HEAD')
    parser.add_argument('--bundle', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--build-arg', action='append', default=[])
    args = parser.parse_args(argv)
    try:
        result = build(args.repo, args.ref, args.bundle, args.output,
                       build_args=inputs.parse_build_args(args.build_arg))
    except (BuildFailed, inputs.InvalidInputs, ValueError):
        print('committed web compilation failed', file=sys.stderr)
        return 1
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
