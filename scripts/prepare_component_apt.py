"""Propose APT locks or fetch/stage approved native package material."""
import argparse
import json
from pathlib import Path
import sys
import component_apt_materials as apt
from collect_component_source import ROOT


def main(argv):
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('operation',choices=('resolve','fetch','stage'))
    parser.add_argument('kind',choices=tuple(apt.REQUESTED))
    parser.add_argument('--platform',choices=('linux/amd64','linux/arm64'),default='linux/amd64')
    parser.add_argument('--repo',type=Path,default=ROOT)
    parser.add_argument('--ref',default='HEAD')
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--bundle',type=Path)
    args=parser.parse_args(argv)
    try:
        if args.operation=='resolve' and args.bundle is None:result=apt.resolve(args.repo,args.ref,args.kind,args.platform,args.output)
        elif args.operation=='fetch' and args.bundle is None:result=apt.fetch(args.repo,args.ref,args.kind,args.platform,args.output)
        elif args.operation=='stage' and args.bundle is not None:result=apt.stage(args.repo,args.ref,args.kind,args.platform,args.bundle,args.output)
        else:raise apt.MaterialFailed('invalid APT operation')
    except apt.MaterialFailed:
        print('APT material operation failed',file=sys.stderr);return 1
    print(json.dumps(result,sort_keys=True));return 0


if __name__=='__main__':sys.exit(main(sys.argv[1:]))
