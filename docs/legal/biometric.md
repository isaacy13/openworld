# OpenWorld Biometric Data Policy

Effective: **[Owner: release date]**

OneX Engineering ("OneX", "we") publishes OpenWorld. This policy describes the face data OpenWorld processes, where it is kept, and when it is destroyed. OpenWorld processes this data only on your device, and OneX never receives it. OneX publishes this policy so that anyone, including people whose faces are in a scanned file, can see exactly what happens.

## 1. What is processed

- **Face templates.** For each face that passes the quality checks, OpenWorld computes a template: a list of numbers that describes the face. It compares that template with the templates of FBI poster photos.
- **Face crops and frames.** A result keeps a small picture of each face found, and the frame of each possible candidate (at most 1,600 pixels across). These are photographs, not templates.
- **Poster templates.** OpenWorld computes a template for each FBI poster photo it downloads.

OpenWorld does not compute or keep fingerprints, voiceprints, iris scans, hand or body geometry, or any other biometric data. It reads no license plates.

## 2. Purpose

The only purpose is to show you, on your device, whether a face in a file you chose looks like the photo on a public FBI missing-person or wanted-person poster. OpenWorld never uses face data to train a model, to enroll anyone, to build a profile, or for advertising.

## 3. Where it is kept

| Data | Where | Destroyed |
| --- | --- | --- |
| Templates of faces in your files | Your device's memory only. Never written to storage. | As soon as the scan has decided that face, and in any case when the scan ends. |
| Face crops and candidate frames | The result folder on your device | With the result: when you delete it, or the next time the app opens after 7 days or after its poster set expires, whichever is first. |
| Poster templates | The poster set on your device | The poster set stops being used after 7 days, and is replaced at each update. Deleting the app deletes it. |

Results are excluded from cloud and device backups on iPhone, iPad, Mac, and Android ([Privacy Policy](privacy.md), §2).

## 4. Disclosure

OneX does not receive, sell, lease, trade, disclose, or otherwise profit from biometric data, because none of it leaves your device. OpenWorld has no account, no sync, no analytics, and no crash reports. Update posters downloads from FBI sites and sends nothing about your files ([Privacy Policy](privacy.md), §3).

## 5. Retention and destruction

Templates of faces in your files exist only in memory during a scan. Saved crops and frames are destroyed on the schedule in §3: the app checks the 7-day deadline every time it opens, so a result is never kept past the first launch after it expires. Deleting a result, or deleting the app, destroys it at once.

## 6. People in your files

A file can show people other than you. OpenWorld cannot ask them for consent, which is why their faces are handled the same way as yours: on the device, never sent, and destroyed on the schedule above. The [Terms of Use](terms.md) forbid sharing a result and forbid scanning files you do not have the right to use.

## 7. Security and incidents

Face data is protected by your device's own security: file protection on iPhone and iPad, app-private storage on Android, and your account's file permissions on computers. Because OneX holds no biometric data, a breach of OneX could not expose any. If a flaw in OpenWorld could expose face data on a device, OneX will fix it, publish a security notice on the project's GitHub page, and tell users in the next release.

## 8. Contact

**[Owner: contact email and postal address]**
