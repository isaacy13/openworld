# SPDX-License-Identifier: Apache-2.0
"""scripts/pin_actions.py rewrites and checks `uses:` lines. No network: refs are faked."""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import pin_actions  # noqa: E402

A = "a" * 40
B = "b" * 40
C = "c" * 40
REFS = {
    "actions/checkout": {"refs/tags/v4": A, "refs/heads/main": C},
    # An annotated tag: the peeled entry is the commit.
    "gradle/actions": {"refs/tags/v4": "f" * 40, "refs/tags/v4^{}": B},
    "dtolnay/rust-toolchain": {"refs/heads/master": C},
}


def resolver():
    calls = []

    def lister(repo):
        calls.append(repo)
        return REFS[repo]

    r = pin_actions.Resolver(lister)
    r.calls = calls
    return r


WORKFLOW = """jobs:
  core:
    steps:
      - uses: actions/checkout@v4
      - uses: "gradle/actions/setup-gradle@v4"
        with:
          x: 1
      - uses: dtolnay/rust-toolchain@master # the toolchain is named below
      - uses: ./local/action
      - uses: actions/checkout@{A} # v4
  reuse:
    uses: actions/checkout/.github/workflows/x.yml@v4
""".replace("{A}", A)


class PinActions(unittest.TestCase):
    def test_rewrites_tags_and_branches_and_keeps_the_rest(self):
        r = resolver()
        text, changed = pin_actions.pin_text(WORKFLOW, r)
        self.assertEqual(changed, 4)
        self.assertIn(f"      - uses: actions/checkout@{A} # v4\n", text)
        self.assertIn(f'      - uses: "gradle/actions/setup-gradle@{B}" # v4\n', text)
        self.assertIn(f"      - uses: dtolnay/rust-toolchain@{C} # master\n", text)
        self.assertIn(f"    uses: actions/checkout/.github/workflows/x.yml@{A} # v4\n", text)
        self.assertIn("      - uses: ./local/action\n", text)
        self.assertIn("          x: 1\n", text)
        # One ls-remote per repository.
        self.assertEqual(sorted(r.calls), ["actions/checkout", "dtolnay/rust-toolchain", "gradle/actions"])
        # Running again changes nothing.
        again, changed = pin_actions.pin_text(text, resolver())
        self.assertEqual((again, changed), (text, 0))
        self.assertEqual(pin_actions.check_text(text, "w.yml", resolver()), [])

    def test_check_names_each_unpinned_line(self):
        problems = pin_actions.check_text(WORKFLOW, "w.yml")
        self.assertEqual(len(problems), 4, problems)
        self.assertTrue(problems[0].startswith("w.yml:4: not pinned"), problems)
        self.assertTrue(any("docker://alpine" in p for p in pin_actions.check_text("    - uses: docker://alpine:3\n", "w.yml")))
        self.assertEqual(pin_actions.check_text("    - uses: docker://alpine@sha256:" + "0" * 64 + "\n", "w.yml"), [])

    def test_a_sha_without_its_tag_or_with_the_wrong_tag_is_a_problem(self):
        self.assertIn("without a `# <tag>` comment", pin_actions.check_text(f"- uses: actions/checkout@{A}\n", "w.yml")[0])
        wrong = pin_actions.check_text(f"- uses: actions/checkout@{B} # v4\n", "w.yml", resolver())
        self.assertIn("is not actions/checkout@v4", wrong[0])

    def test_an_unknown_ref_refuses(self):
        with self.assertRaises(SystemExit):
            pin_actions.pin_text("- uses: actions/checkout@v9\n", resolver())


if __name__ == "__main__":
    unittest.main()
