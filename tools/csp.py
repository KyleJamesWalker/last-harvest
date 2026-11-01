#!/usr/bin/env python3
"""Keep each web page's Content-Security-Policy hashes in step with its inline scripts.

Usage: tools/csp.py [--check]

A page's `<meta http-equiv="Content-Security-Policy">` lets its inline `<script>` blocks run only by their sha256, so
any edit to one must update the hash. This rewrites the script-src hashes of every page below; --check exits 1 when a
page is out of date instead of writing it.
"""
import base64
import hashlib
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PAGES = ['assets/site/index.html', 'crates/app/web/index.html', 'crates/editor/web/index.html']
INLINE = re.compile(r'<script(?![^>]*\bsrc=)[^>]*>(.*?)</script>', re.DOTALL)
CSP = re.compile(r'(<meta http-equiv="Content-Security-Policy" content="[^"]*script-src)([^;"]*)')


def hashed(html):
    """The page with its CSP's script-src listing exactly the hashes of its inline scripts."""
    hashes = [f"'sha256-{base64.b64encode(hashlib.sha256(s.encode()).digest()).decode()}'" for s in INLINE.findall(html)]
    match = CSP.search(html)
    if not match:
        raise SystemExit('no Content-Security-Policy meta tag with a script-src')
    sources = [s for s in match.group(2).split() if not s.startswith("'sha256-")] + hashes
    return html[:match.start(2)] + ' ' + ' '.join(sources) + html[match.end(2):]


def main():
    check = '--check' in sys.argv[1:]
    stale = []
    for page in PAGES:
        path = ROOT / page
        html = path.read_text()
        want = hashed(html)
        if want != html:
            stale.append(page)
            if not check:
                path.write_text(want)
    if stale:
        print(('out of date: ' if check else 'updated: ') + ', '.join(stale) + ('; run tools/csp.py' if check else ''))
        sys.exit(1 if check else 0)


if __name__ == '__main__':
    main()
