"""Prepare verified npm archives or consume them in an isolated offline install.

Usage: python scripts/prepare_component_npm.py fetch --output NEW_DIRECTORY [--ref HEAD] [--repo PATH]
       python scripts/prepare_component_npm.py install --bundle DIRECTORY --output NEW_DIRECTORY [--ref HEAD] [--repo PATH]
"""

import argparse
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys

import component_npm_materials as materials
from collect_component_source import ROOT, canonical


def npm_command():
    node, npm = shutil.which('node'), shutil.which('npm')
    if not node or not npm:
        raise materials.MaterialFailed('trusted Node and npm are required')
    npm = Path(npm).resolve()
    # Unix npm normally resolves to the CLI JS; Windows ships launchers beside Node.
    cli = npm if npm.name == 'npm-cli.js' else npm.parent / 'node_modules/npm/bin/npm-cli.js'
    if not cli.is_file():
        raise materials.MaterialFailed('unsupported npm installation layout')
    return [str(Path(node).resolve()), str(cli)]


def child_environment(node):
    allowed = ('SYSTEMROOT', 'WINDIR', 'COMSPEC', 'PATHEXT', 'TEMP', 'TMP', 'USERPROFILE', 'HOME')
    env = {key: os.environ[key] for key in allowed if key in os.environ}
    env['PATH'] = str(Path(node).parent)
    return env


def install(root, ref, bundle, output, *, run=subprocess.run):
    try:
        command = npm_command()
        output = Path(output).absolute()
        revision, files, manifest = materials.stage_bundle(root, ref, bundle, output)
        workspace = output / 'workspace'
        user, global_config = output / 'empty-user.npmrc', output / 'empty-global.npmrc'
        materials.write_file(user, b''); materials.write_file(global_config, b'')
        common = ['--offline', '--ignore-scripts', '--no-audit', '--no-fund', '--logs-max=0',
                  '--loglevel=silent', '--cache', str(output / 'cache'),
                  '--userconfig', str(user), '--globalconfig', str(global_config)]
        env = child_environment(command[0])
        def execute(args):
            result = run([*command, *args, *common], cwd=workspace, env=env,
                         stdin=subprocess.DEVNULL, capture_output=True, timeout=300)
            if result.returncode:
                raise materials.MaterialFailed('offline npm command failed')
        # Keep argv below Windows process limits; npm accepts multiple local specs.
        archives = [str(output / entry['path']) for entry in manifest['archives']]
        for start in range(0, len(archives), 16):
            execute(['cache', 'add', *archives[start:start + 16]])
        execute(['ci'])
        if (not (workspace / 'node_modules').is_dir()
                or any((workspace / name).read_bytes() != data for name, data in files.items())):
            raise materials.MaterialFailed('offline install changed committed metadata')
        result = {'source_revision': revision, 'material_digest': manifest['material_digest'],
                  'archive_count': len(archives), 'package_records': len(manifest['packages']),
                  'host_platform': platform.system().lower() + '/' + platform.machine().lower(),
                  'offline_install_verified': True, 'identity_verified': False, 'publication_authorized': False}
        materials.write_file(output / 'result.json', canonical(result) + b'\n')
        return result
    except Exception:
        raise materials.MaterialFailed('offline npm installation failed') from None


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('operation', choices=('fetch', 'install'))
    parser.add_argument('--repo', type=Path, default=ROOT)
    parser.add_argument('--ref', default='HEAD')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--bundle', type=Path)
    args = parser.parse_args(argv)
    try:
        if args.operation == 'fetch' and args.bundle is None:
            result = materials.materialize(args.repo, args.ref, args.output)
        elif args.operation == 'install' and args.bundle is not None:
            result = install(args.repo, args.ref, args.bundle, args.output)
        else:
            raise materials.MaterialFailed('invalid material operation')
    except materials.MaterialFailed:
        print('npm material operation failed', file=sys.stderr)
        return 1
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
