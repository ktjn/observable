"""Fetch approved Cargo archives or reconstruct a verified offline directory source."""
import argparse
import json
from pathlib import Path
import sys
import component_cargo_materials as cargo
from collect_component_source import ROOT


def main(argv):
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('operation',choices=('fetch','stage'))
    parser.add_argument('--repo',type=Path,default=ROOT)
    parser.add_argument('--ref',default='HEAD')
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--bundle',type=Path)
    args=parser.parse_args(argv)
    try:
        if args.operation=='fetch' and args.bundle is None:result=cargo.materialize(args.repo,args.ref,args.output)
        elif args.operation=='stage' and args.bundle is not None:result=cargo.stage(args.repo,args.ref,args.bundle,args.output)
        else:raise cargo.MaterialFailed('invalid Cargo material operation')
    except cargo.MaterialFailed:
        print('Cargo material operation failed',file=sys.stderr);return 1
    print(json.dumps(result,sort_keys=True));return 0


if __name__=='__main__':sys.exit(main(sys.argv[1:]))
