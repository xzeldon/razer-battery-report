#!/usr/bin/env python3
"""Print the release notes for a tag: the feat and fix commit subjects since the previous release.

The previous release is the nearest earlier `v*` tag that is not a pre-release, so a stable
release lists everything since the last stable one. Other commit types are left out. The
release workflow publishes this text; run it locally to preview the notes of a tag.

    python scripts/release-notes.py v1.0.0
"""

import os
import re
import subprocess
import sys

SECTIONS = (("New", "feat"), ("Fixed", "fix"))
SUBJECT = re.compile(r"^(?P<type>\w+)(?:\([^)]*\))?!?: (?P<text>.+)$")


def git(*args):
    result = subprocess.run(["git", *args], capture_output=True, text=True, check=False)
    return result.returncode, result.stdout.strip()


def main():
    tag = sys.argv[1]
    code, previous = git("describe", "--tags", "--abbrev=0", "--match=v*", "--exclude=v*-*", f"{tag}^")
    commits = f"{previous}..{tag}" if code == 0 else tag
    _, log = git("log", "--no-merges", "--format=%s", commits)

    entries = {kind: [] for _, kind in SECTIONS}
    for subject in log.splitlines():
        match = SUBJECT.match(subject)
        if match and match["type"] in entries:
            text = match["text"]
            entries[match["type"]].append(text[0].upper() + text[1:])

    for title, kind in SECTIONS:
        if entries[kind]:
            print(f"## {title}\n")
            print("\n".join(f"- {text}" for text in entries[kind]))
            print()

    repository = os.environ.get("GITHUB_REPOSITORY")
    if code == 0 and repository:
        print(f"**Full changelog**: https://github.com/{repository}/compare/{previous}...{tag}")


if __name__ == "__main__":
    main()
