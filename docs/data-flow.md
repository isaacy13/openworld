# Data flow

Every piece of data OpenWorld touches, where it lives, and when it goes away. Nothing on this page leaves the device. The public [policies](legal/README.md) say the same thing to users, so a change here changes them in the same commit.

| Data | Created by | Stored | Deleted |
| --- | --- | --- | --- |
| The original file | The user | Where it already was. OpenWorld only reads it. | Never by OpenWorld. |
| Imported copy (phone share sheet, a file with no local path) | The shell | App cache, `imports/` | When the next file is chosen, when the result is deleted, and at every launch for leftovers. |
| Decoded frames | The platform decoder | Memory only, one at a time | After each frame. Never written to disk. |
| Face crops (the faces in the file) | The core | The result folder, `crops/`, at most one per face | With the result. |
| Evidence frame (a candidate's frame, at most 1,600 px) | The core | The result folder, `frames/`, candidates only | With the result. |
| Face templates (embeddings) | The core | Memory only, while the face is on screen | When the face's row is decided. Never written in a shipped build. |
| `result.json` | The core | The result folder | After 7 days, or when the poster pack expires, whichever is first (`sweep` runs at launch), or when the user deletes it. A folder a crashed scan left without `result.json` goes 7 days after it was made. |
| Poster pack (snapshot, vectors, poster photos) | Update posters (off), or the demo pack | App data, `posters/` | Refused after 7 days; replaced on update. |
| Bundle weights | The app bundle / `fetch_weights.py` | App data, `bundles/` | With the app. |
| Settings (last bundle and coverage) | The shell | Platform preferences | With the app. |
| Accepted terms version (`legal`) | The first-run screen's Continue | Platform preferences | With the app. Never sent. |
| Policies and licenses | The library (`docs/legal/`) | Inside the app | With the app. Reading them uses no network. |

## What is never collected

No account, no analytics, no crash reports with media, no location, no contacts, no camera, no microphone. Results are excluded from cloud backup on iOS and Android (`allowBackup=false`) and protected with the platform's file protection where it exists.

## Network

A scan makes no network request. The only network action is **Update posters**, which stays off until an official curve allows real posters. When it is on, it fetches the public FBI list and poster photos over HTTPS from `api.fbi.gov`, `www.fbi.gov`, and `fbi.gov` only, hashes each response body, builds the poster vectors on the device, and sends nothing about the user's files ([api](api.md#update-posters)). **Open FBI page** hands one URL to the browser after the leaving sheet; OpenWorld does not send a report.
