# Store listing and review notes

Written to [legal-review.md](legal-review.md) §8. Use this text as it is; a change goes through that section first. Every store: **United States only** (§7), publisher **OneX Engineering** as an organization account, privacy policy URL = the hosted [legal/privacy.md](legal/privacy.md).

## Name and subtitle

- Name: **OpenWorld**
- Subtitle (Apple, 30 characters): **Check your photos, on device**
- Short description (Google, 80 characters): **Compare faces in your own photos with public posters, on your device.**

Never in the name, subtitle, icon, keywords, or developer name: "FBI", "official", "police", "identify", "find", "catch", "match found", "bounty", "reward", an accuracy number, or the FBI seal.

## Description

OpenWorld compares faces in a photo or video you already have with public FBI missing-person and wanted-person posters, on your device. Nothing is uploaded.

OpenWorld is not part of, or endorsed by, the FBI or any government agency.

- Choose a photo or video you already have. There is no camera.
- The file stays on your device. There is no account, no analytics, and no server.
- Every face is compared on the device, with a locked cutoff, and the result says how many false candidates chance alone would give.
- A possible candidate shows your crop beside the poster photo and a link to the poster's page.

A possible candidate is not an identification. No candidate is not a clearance. Do not approach anyone, and do not post or share a result. OpenWorld does not report anything to an agency.

For adults in the United States.

Terms of Use, Privacy Policy, and Biometric Data Policy: [hosted URLs]

## Keywords (Apple, 100 characters)

photo,video,face,compare,missing person,poster,privacy,on device,offline

## Screenshots

Demo posters only (the synthetic markers). Never a real wanted or missing person, an fbi.gov page, or the seal. The desktop screenshot run (`--screenshot`) and the phone emulator tests produce the screens.

## Age rating

The highest fitting rating (Apple 18+, Google "Mature 17+" or the closest), with "unrestricted web access: no" and "user-generated content: no". Where a store passes an age signal (Texas SB 2420, California AB 1043), honor it.

## Google Play Data safety

- Data collected: **none**. Data shared: **none**. Everything is processed on the device and never sent off it.
- Encrypted in transit: yes (Update posters uses HTTPS to FBI servers and sends nothing about the user).
- Deletion: the user deletes results in the app, or the app.
- Re-check before each release that the manifest still has only INTERNET (used by Update posters alone).

## Apple App Privacy

- Data Not Collected.
- Privacy manifest: no tracking, no tracking domains. Required-reason APIs: declare only what the app's code and libraries actually call (file timestamps, user defaults).

## Apple review notes

> OpenWorld is not a reporting app. It never contacts law enforcement or anyone else for the user. It compares faces in a photo or video the user picks with public FBI poster photos, entirely on the device, and shows a possible candidate with a link to the poster's own fbi.gov page. A sheet before that link says OpenWorld is not sending a report and repeats the do-not-approach guidance printed on the poster.
>
> It does not compile personal information or build profiles: there is no search for a person, no account, no server, and nothing is uploaded. Poster data is a public government notice, cached on the device for at most 7 days and linked back to its source.
>
> Demo posters: the build ships synthetic demo posters (marked "Demo") for review. Real FBI posters turn on only when the app's published accuracy measurement on poster photos passes its gate; until then "Update posters" explains that real posters stay off. No feature is hidden or enabled remotely (guideline 2.5.19).
>
> Facial recognition is not used for sign-in (2.5.13 does not apply). OneX Engineering is the publisher; contact support@onexengineering.com.

Ask Apple for pre-submission guidance on guidelines 1.7 and 5.1.1(viii) before the first submission (legal-review §8.1), and plan a desktop-first launch in case of rejection.
