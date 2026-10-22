# Website

`index.html`, `page.html`, `style.css`, and `_headers` are the site. `scripts/site.py` builds it: it renders the policies from `docs/legal/*.md` and the licenses from `docs/legal/third-party.txt`, the same texts the apps carry, inlines the stylesheet, and puts the hash of each inline script into the Content-Security-Policy in `_headers`. CI builds it on every push.

## Cloudflare Pages

`.github/workflows/site.yml` builds the site and deploys it with Wrangler: `main` is the production site, every branch and pull request gets its own preview URL. Set it up once:

1. In the Cloudflare dashboard, **Workers & Pages → Create → Pages → Create using direct upload**, and name the project **`openworld`** (the workflow passes `--project-name=openworld`). One manual upload or the first CI run creates it.
2. **My Profile → API Tokens → Create Token → "Edit Cloudflare Pages"**, restricted to this account. Copy the token.
3. In GitHub, **Settings → Secrets and variables → Actions**, add `CLOUDFLARE_API_TOKEN` (the token) and `CLOUDFLARE_ACCOUNT_ID` (the ID on the dashboard's right sidebar).

The workflow needs nothing else, and no credential lives in this repository. Until the two secrets exist, the job still builds and checks the site, then stops without deploying. Pages serves each page without `.html` (`/privacy.html` → `/privacy`). When the new domain exists, add it under the project's **Custom domains**.
