# Website

`index.html`, `page.html`, `style.css`, and `_headers` are the site. `scripts/site.py` builds it: it renders the policies from `docs/legal/*.md` and the licenses from `docs/legal/third-party.txt`, the same texts the apps carry, inlines the stylesheet, and puts the hash of each inline script into the Content-Security-Policy in `_headers`. CI builds it on every push.

## Cloudflare Pages

Connect the repository once, in the Cloudflare dashboard: **Workers & Pages → Create → Pages → Connect to Git**, pick `isaacy13/openworld`, and set:

| Setting | Value |
| --- | --- |
| Production branch | `main` |
| Framework preset | None |
| Build command | `python3 scripts/site.py _site` |
| Build output directory | `_site` |

Cloudflare then builds `main` as the production site and every other branch as a preview at its own address, so a pull request can be checked live before it merges. No token or secret goes in this repository. The build needs only Python 3 from the build image.

Pages serves each page without `.html` (`/privacy.html` redirects to `/privacy`), so links work either way; the store listings use the short form. When the new domain exists, add it under the project's **Custom domains**.
