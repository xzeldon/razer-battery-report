#!/usr/bin/env python3
"""Print the release notes for a tag in the layout of GitHub's generated notes, built from commits.

GitHub generates notes only from pull requests, and releases here are made from commits pushed
straight to master. This lists every commit since the previous release, the nearest earlier `v*`
tag that is not a pre-release, with its author and a link, then the authors whose first commit
is in the release. Logins come from the GitHub API through `gh`. The release workflow publishes
this text; run it locally to preview the notes of a tag that is already pushed.

    GITHUB_REPOSITORY=xzeldon/razer-battery-report python scripts/release-notes.py v1.0.0
"""

import os
import subprocess
import sys


def run(*args):
    result = subprocess.run(args, capture_output=True, text=True, check=False)
    return result.returncode, result.stdout.strip()


def commit_link(url, sha):
    """A link that reads as the short hash; release bodies do not shorten bare commit URLs."""
    return f"[`{sha[:7]}`]({url}/commit/{sha})"


def main():
    tag = sys.argv[1]
    repository = os.environ["GITHUB_REPOSITORY"]
    url = f"https://github.com/{repository}"

    code, previous = run(
        "git", "describe", "--tags", "--abbrev=0", "--match=v*", "--exclude=v*-*", f"{tag}^"
    )
    has_previous = code == 0
    commits = f"{previous}..{tag}" if has_previous else tag
    _, log = run("git", "log", "--no-merges", "--reverse", "--format=%H%x00%ae%x00%an%x00%s", commits)
    _, earlier = run("git", "log", "--format=%ae", previous) if has_previous else (0, "")
    known = set(earlier.splitlines())

    authors = {}
    newcomers = []
    print("## What's Changed\n")
    for line in log.splitlines():
        sha, email, name, subject = line.split("\0")
        if email not in authors:
            code, login = run("gh", "api", f"repos/{repository}/commits/{sha}", "--jq", ".author.login // empty")
            authors[email] = f"@{login}" if code == 0 and login else name
            if email not in known:
                newcomers.append((authors[email], sha))
        print(f"* {subject} by {authors[email]} in {commit_link(url, sha)}")

    if has_previous and newcomers:
        print("\n## New Contributors\n")
        for author, sha in newcomers:
            print(f"* {author} made their first contribution in {commit_link(url, sha)}")

    changes = f"compare/{previous}...{tag}" if has_previous else f"commits/{tag}"
    print(f"\n**Full Changelog**: {url}/{changes}")


if __name__ == "__main__":
    main()
