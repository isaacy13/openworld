# Request to the FBI: access to poster photos

Why this exists: on 2026-10-19 and 2026-10-20 the poster photos on `www.fbi.gov` answered every non-browser request with Cloudflare's managed challenge page ("Just a moment...", HTTP 403), from GitHub's servers and from the owner's home network, whatever the User-Agent or headers, while a browser on the same network loaded them. The list at `api.fbi.gov` answers normally. OpenWorld will not disguise itself as a browser to get past that control ([eval/README.md](../eval/README.md#where-the-poster-run-can-run)), so real posters need the FBI's agreement. [legal-review.md](legal-review.md) §2.2 and §13 already recommend a courtesy letter to the FBI Office of Public Affairs; this is that letter, with the access question added.

Send it from a OneX Engineering address through the contact route the FBI publishes for its website and the Wanted API (check fbi.gov for the current Office of Public Affairs or webmaster contact; this file does not guess one). Keep the reply with the decision records.

---

**Subject:** OpenWorld: automated access to Wanted poster photos for on-device comparison

To the FBI Office of Public Affairs,

OneX Engineering, a Texas company, is preparing to release OpenWorld, a free, open-source app for iPhone, Android, Mac, Windows, and Linux. A person chooses a photo or video they already have, and the app compares the faces in it with the photos on the FBI's public missing-person and wanted-person posters. The comparison runs entirely on the person's device. Nothing is uploaded, there is no account or server, and the app sends no report to anyone. When a face passes a strict, published cutoff, the app shows "possible candidate, not an identification", tells the person not to approach anyone, and offers a link to the poster's own page on fbi.gov.

We would like to make sure OpenWorld works with your systems the way you intend:

1. **Automated access to poster photos.** The app reads the public list from `https://api.fbi.gov/wanted/v1/list`, which works as documented. The poster photos it then needs (for example `https://www.fbi.gov/wanted/.../@@images/image/large`) are answered with Cloudflare's managed challenge ("Just a moment...", HTTP 403), which only a web browser can pass. A device downloads only when its owner taps Update posters, takes only the open, named missing and wanted posters, pauses between requests, and identifies itself honestly as `OpenWorld/<version> (+https://github.com/isaacy13/openworld)`. Could that User-Agent be allowed past the challenge for poster photos (a Cloudflare skip rule for it, or for the `@@images` paths, would do it), or is there a supported way, such as an API image endpoint, to fetch the photos? We will not disguise the app as a browser.
2. **Use of the name.** OpenWorld does not use "FBI" in its name, icon, or store keywords, never shows the FBI seal or badge, and states on every screen where it matters that it is not part of or endorsed by the FBI or any government agency. If you would like any wording changed, we will change it.
3. **Content we skip.** The app leaves out unidentified-person posters, seeking-information posters that name no one, captured or resolved cases, and ECAP posters, and it does not cache poster data longer than 7 days.

The source code, the measured error rates (including how often it misses a person whose poster photo is old), and our policies are public at https://github.com/isaacy13/openworld. We would welcome any guidance before release.

Sincerely,

[Name], OneX Engineering
support@onexengineering.com
