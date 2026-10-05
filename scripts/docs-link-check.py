#!/usr/bin/env python3
"""Folio link check: guide-to-guide `x.md` links in content/guide that point at no page."""
import glob, os, re, sys
pages = {os.path.basename(f)[:-3] for f in glob.glob('content/guide/*.md')}
bad, total = [], 0
for f in sorted(glob.glob('content/guide/*.md')):
    for m in re.finditer(r'\]\((?:\./)?([A-Za-z0-9_-]+)\.md(#[^)]*)?\)', open(f).read()):
        total += 1
        if m.group(1) not in pages:
            bad.append(f"{f}: {m.group(0)}")
print(f"pages={len(pages)} links={total} broken={len(bad)}")
print("\n".join(bad))
sys.exit(1 if bad else 0)
