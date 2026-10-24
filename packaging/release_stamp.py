"""Print the release.json stamp for a release tag (<system>-v<version>[-<build>])."""
import argparse
import json
import re
import sys

TAG = re.compile(r'[a-z]+-v(\d[\d.]*)(?:-(.+))?')


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('tag')
    parser.add_argument('--prerelease', required=True, choices=['true', 'false'])
    args = parser.parse_args()
    match = TAG.fullmatch(args.tag)
    if match is None:
        sys.stderr.write(f'invalid release tag: {args.tag}\n')
        return 1
    version, build = match.groups()
    stamp = {'channel': 'pre-release' if args.prerelease == 'true' else 'release', 'version': version}
    if build is not None:
        stamp['build'] = build
    print(json.dumps(stamp, ensure_ascii=False))
    return 0


if __name__ == '__main__':
    sys.exit(main())
