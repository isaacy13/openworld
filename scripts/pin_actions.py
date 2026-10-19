#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Pin every GitHub Action in .github/workflows to a commit SHA.

    uses: actions/checkout@v4
becomes
    uses: actions/checkout@<40-hex commit> # v4

The tag (or branch) is resolved with `git ls-remote https://github.com/<owner>/<repo>`;
an annotated tag is peeled to its commit. The comment keeps the ref, which is how
Dependabot keeps updating a pinned action. Lines that are already pinned are left
alone, so this can be run again after new jobs are added.

    python3 scripts/pin_actions.py           # rewrite the workflows in place
    python3 scripts/pin_actions.py --check   # fail on any `uses:` not pinned to a SHA
    python3 scripts/pin_actions.py --check --verify   # also check each SHA against its comment

Local actions (`./...`) are not pinned. A `docker://` image counts as pinned only with
an `@sha256:` digest.
"""

import argparse
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SHA_RE = re.compile(r"^[0-9a-f]{40}$")
# `uses:` as a step key, a list item, or a job key (reusable workflows), with an
# optional quote around the value and an optional trailing comment.
USES_RE = re.compile(
    r"""^(?P<lead>\s*(?:-\s+)?uses:\s*)(?P<q>["']?)(?P<value>[^\s"'#]+)(?P=q)(?P<rest>\s*(?:\#.*)?)$"""
)


def workflow_files(root: Path) -> list:
    folder = root / ".github" / "workflows"
    return sorted(p for p in folder.iterdir() if p.suffix in (".yml", ".yaml") and p.is_file())


def parse(value: str):
    """(repo 'owner/name', full action path, ref) or None for local and docker actions."""
    if value.startswith("./") or value.startswith("docker://") or "@" not in value:
        return None
    action, ref = value.rsplit("@", 1)
    parts = action.split("/")
    if len(parts) < 2 or not parts[0] or not parts[1]:
        return None
    return f"{parts[0]}/{parts[1]}", action, ref


def is_pinned(value: str) -> bool:
    if value.startswith("./"):
        return True
    if value.startswith("docker://"):
        return "@sha256:" in value
    parsed = parse(value)
    return parsed is not None and bool(SHA_RE.match(parsed[2]))


def ls_remote(repo: str) -> dict:
    """{ref name: sha} for every tag and branch of github.com/<repo>."""
    out = subprocess.run(
        ["git", "ls-remote", "--tags", "--heads", f"https://github.com/{repo}"],
        check=True, capture_output=True, text=True, timeout=120,
    ).stdout
    refs = {}
    for line in out.splitlines():
        sha, name = line.split("\t", 1)
        refs[name] = sha
    return refs


class Resolver:
    def __init__(self, lister=ls_remote):
        self.lister = lister
        self.cache = {}

    def resolve(self, repo: str, ref: str) -> str:
        if repo not in self.cache:
            self.cache[repo] = self.lister(repo)
        refs = self.cache[repo]
        # A peeled annotated tag names the commit; a lightweight tag already does.
        for name in (f"refs/tags/{ref}^{{}}", f"refs/tags/{ref}", f"refs/heads/{ref}"):
            if name in refs:
                sha = refs[name]
                if not SHA_RE.match(sha):
                    raise SystemExit(f"{repo}@{ref}: {sha} is not a commit SHA. Refusing.")
                return sha
        raise SystemExit(f"{repo}@{ref}: no such tag or branch on github.com/{repo}. Refusing.")


def comment_ref(rest: str):
    match = re.match(r"\s*#\s*(\S+)", rest)
    return match.group(1) if match else None


def pin_text(text: str, resolver: Resolver):
    """The text with every unpinned action pinned, and the number of lines changed."""
    lines = text.splitlines(keepends=True)
    changed = 0
    for i, line in enumerate(lines):
        body = line.rstrip("\r\n")
        ending = line[len(body):]
        match = USES_RE.match(body)
        if not match or is_pinned(match["value"]):
            continue
        parsed = parse(match["value"])
        if parsed is None:
            continue
        repo, action, ref = parsed
        sha = resolver.resolve(repo, ref)
        lines[i] = f"{match['lead']}{match['q']}{action}@{sha}{match['q']} # {ref}{ending}"
        changed += 1
    return "".join(lines), changed


def check_text(text: str, path: str, resolver=None) -> list:
    """Problems: unpinned actions, and (with a resolver) a SHA that is not its comment's ref."""
    problems = []
    for number, line in enumerate(text.splitlines(), 1):
        match = USES_RE.match(line)
        if not match:
            if re.match(r"^\s*(?:-\s+)?uses:", line):
                problems.append(f"{path}:{number}: cannot read this `uses:` line: {line.strip()}")
            continue
        value = match["value"]
        if not is_pinned(value):
            problems.append(f"{path}:{number}: not pinned to a commit SHA: {value}")
            continue
        parsed = parse(value)
        if parsed is None:
            continue
        repo, _, sha = parsed
        ref = comment_ref(match["rest"])
        if ref is None:
            problems.append(f"{path}:{number}: pinned without a `# <tag>` comment, so Dependabot cannot update it: {value}")
        elif resolver is not None and resolver.resolve(repo, ref) != sha:
            problems.append(f"{path}:{number}: {sha} is not {repo}@{ref}")
    return problems


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--check", action="store_true", help="change nothing; fail on any unpinned action")
    parser.add_argument("--verify", action="store_true", help="with --check: resolve each comment and compare it with the SHA")
    parser.add_argument("--root", default=str(ROOT), help=argparse.SUPPRESS)
    args = parser.parse_args()
    root = Path(args.root)
    files = workflow_files(root)
    if args.check:
        resolver = Resolver() if args.verify else None
        problems = [p for f in files for p in check_text(f.read_text(), str(f.relative_to(root)), resolver)]
        for problem in problems:
            print(problem, file=sys.stderr)
        if problems:
            print("Run python3 scripts/pin_actions.py to pin them.", file=sys.stderr)
            return 1
        print(f"Every action in {len(files)} workflow files is pinned to a commit SHA.")
        return 0
    resolver = Resolver()
    for path in files:
        text, changed = pin_text(path.read_text(), resolver)
        if changed:
            path.write_text(text)
            print(f"{path.relative_to(root)}: pinned {changed}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
