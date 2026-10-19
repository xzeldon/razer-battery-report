#!/usr/bin/env python3
"""Print the release notes for a tag in the layout of GitHub's generated notes, built from commits.

GitHub generates notes only from pull requests, and releases here are made from commits pushed
straight to master. This groups the commits since the previous release, the nearest earlier `v*`
tag that is not a pre-release, by their Conventional Commits type into sections, each line with
its author and a link, then lists the authors whose first commit is in the release. Types that
mean nothing to a user are left out; the Full Changelog link still shows them. Logins come from
the GitHub API through `gh`. The release workflow publishes this text; run it locally to preview
the notes of a tag that is already pushed.

    GITHUB_REPOSITORY=xzeldon/razer-battery-report python scripts/release-notes.py v1.0.0
"""

import os
import re
import subprocess
import sys

SUBJECT = re.compile(r"^(?P<type>\w+)(?:\((?P<scope>[^)]+)\))?(?P<breaking>!)?: (?P<description>.+)$")
BREAKING = "Breaking changes"
OTHER = "Other changes"
SECTIONS = {"feat": "New features", "fix": "Bug fixes"}
ORDER = [BREAKING, *SECTIONS.values(), OTHER]
HIDDEN_TYPES = {"build", "chore", "ci", "style", "test"}


def run(*args):
    result = subprocess.run(args, capture_output=True, text=True, check=False)
    return result.returncode, result.stdout.strip()


def commit_link(url, sha):
    """A link that reads as the short hash; release bodies do not shorten bare commit URLs."""
    return f"[`{sha[:7]}`]({url}/commit/{sha})"


def classify(subject, body):
    """The section of a commit and its line text, or no section when the notes leave it out."""
    match = SUBJECT.match(subject)
    if not match:
        return OTHER, subject
    kind = match["type"].lower()
    scope = match["scope"]
    text = f"{scope}: {match['description']}" if scope else match["description"]
    if match["breaking"] or "BREAKING CHANGE" in body:
        return BREAKING, text
    if kind in HIDDEN_TYPES:
        return None, text
    return SECTIONS.get(kind, OTHER), text


def main():
    tag = sys.argv[1]
    repository = os.environ["GITHUB_REPOSITORY"]
    url = f"https://github.com/{repository}"

    code, previous = run(
        "git", "describe", "--tags", "--abbrev=0", "--match=v*", "--exclude=v*-*", f"{tag}^"
    )
    has_previous = code == 0
    commits = f"{previous}..{tag}" if has_previous else tag
    _, log = run(
        "git", "log", "--no-merges", "--reverse", "--format=%H%x00%ae%x00%an%x00%s%x00%b%x1e", commits
    )
    _, earlier = run("git", "log", "--format=%ae", previous) if has_previous else (0, "")
    known = set(earlier.splitlines())

    authors = {}
    newcomers = []
    sections = {name: [] for name in ORDER}
    for record in filter(None, (part.strip() for part in log.split("\x1e"))):
        sha, email, name, subject, body = record.split("\0")
        if email not in authors:
            code, login = run("gh", "api", f"repos/{repository}/commits/{sha}", "--jq", ".author.login // empty")
            authors[email] = f"@{login}" if code == 0 and login else name
            if email not in known:
                newcomers.append((authors[email], sha))
        section, text = classify(subject, body)
        if section:
            sections[section].append(f"* {text} by {authors[email]} in {commit_link(url, sha)}")

    for name in ORDER:
        if sections[name]:
            print(f"## {name}\n")
            print("\n".join(sections[name]) + "\n")

    if has_previous and newcomers:
        print("## New Contributors\n")
        for author, sha in newcomers:
            print(f"* {author} made their first contribution in {commit_link(url, sha)}")
        print()

    changes = f"compare/{previous}...{tag}" if has_previous else f"commits/{tag}"
    print(f"**Full Changelog**: {url}/{changes}")


if __name__ == "__main__":
    main()
