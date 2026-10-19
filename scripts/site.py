#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Build the OpenWorld website into a folder: the home page and the policies the apps carry.

    python3 scripts/site.py _site              # the site GitHub Pages serves
    python3 scripts/site.py _site --fragment   # index.html without its document wrapper, for a preview

The policy pages are rendered from docs/legal/*.md and the licenses from
docs/legal/third-party.txt, the same texts the library builds in, so the site and the
apps never disagree. The stylesheet is inlined into every page.
"""
import html
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SITE = ROOT / "site"
LEGAL = ROOT / "docs/legal"
# Published name, source, title.
POLICIES = [
    ("terms.html", "terms.md", "Terms of Use"),
    ("privacy.html", "privacy.md", "Privacy Policy"),
    ("biometric.html", "biometric.md", "Biometric Data Policy"),
    ("health-data.html", "washington-health-data.md", "Consumer Health Data Privacy Policy"),
]
LINKS = {source: page for page, source, _ in POLICIES}
HEAD = '<!doctype html>\n<html lang="en">\n<head>\n<meta charset="utf-8">\n<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">\n'


def inline(text: str) -> str:
    """Escape, then **bold**, `code`, and [links](target). Links to another policy go to its page."""
    text = html.escape(text, quote=False)
    text = re.sub(r"\*\*(.+?)\*\*", r"<strong>\1</strong>", text)
    text = re.sub(r"`(.+?)`", r"<code>\1</code>", text)

    def link(m):
        label, target = m.group(1), m.group(2)
        page = LINKS.get(target.split("#")[0])
        if page:
            return f'<a href="{page}">{label}</a>'
        return f'<a href="{target}">{label}</a>' if target.startswith("http") else label
    return re.sub(r"\[(.+?)\]\((.+?)\)", link, text)


def markdown(text: str) -> str:
    """The Markdown the policies use: headings, paragraphs, and bulleted or numbered lists."""
    out = []
    for block in text.replace("\r\n", "\n").split("\n\n"):
        lines = [line for line in block.strip("\n").split("\n") if line.strip()]
        if not lines:
            continue
        first = lines[0]
        if first.startswith("#"):
            level = min(len(first) - len(first.lstrip("#")), 2)
            out.append(f"<h{level}>{inline(first.lstrip('#').strip())}</h{level}>")
        elif all(line.startswith("- ") for line in lines):
            out.append("<ul>" + "".join(f"<li>{inline(line[2:])}</li>" for line in lines) + "</ul>")
        elif all(re.match(r"\d+\. ", line) for line in lines):
            out.append("<ol>" + "".join(f"<li>{inline(line.split('. ', 1)[1])}</li>" for line in lines) + "</ol>")
        else:
            out.append(f"<p>{inline(' '.join(lines))}</p>")
    return "\n".join(out)


def page(template: str, style: str, title: str, content: str, article_class: str = "") -> str:
    return (template.replace("/*{{style}}*/", style).replace("{{title}}", html.escape(title))
            .replace("{{article_class}}", article_class).replace("{{content}}", content))


def build(out: Path, fragment: bool) -> None:
    out.mkdir(parents=True, exist_ok=True)
    style = (SITE / "style.css").read_text()
    index = (SITE / "index.html").read_text().replace("/*{{style}}*/", style)
    head, body = index.split("<!-- body -->", 1)
    if fragment:
        (out / "index.html").write_text(head + body)
    else:
        (out / "index.html").write_text(HEAD + head + "</head>\n<body>\n" + body + "</body>\n</html>\n")
    template = (SITE / "page.html").read_text()
    for name, source, title in POLICIES:
        (out / name).write_text(page(template, style, title, markdown((LEGAL / source).read_text())))
    licenses = html.escape((LEGAL / "third-party.txt").read_text(), quote=False)
    (out / "licenses.html").write_text(page(template, style, "Licenses", f"<h1>Licenses</h1>\n<pre>{licenses}</pre>", "licenses"))
    (out / ".nojekyll").write_text("")


if __name__ == "__main__":
    if len(sys.argv) < 2:
        raise SystemExit(__doc__)
    build(Path(sys.argv[1]), "--fragment" in sys.argv[2:])
