# Record classes: missing, wanted, and what else

Same scanner; different **labels**. A missing-person candidate must never read as wanted, guilty, or armed. Locks unchanged: local, no-upload, user-initiated, no auto-report, no hidden capture, no user watchlist, no KYC, no LE pattern tool ([project context](project-context.md), [design gaps](design-gaps.md)).

## v0 classes

Device cache of FBI public posters, typed. A tap opens that record’s poster URL, or the plain public tips page when this table names it. No clipboard or prefill.

| Class | Meaning | On-screen text |
| --- | --- | --- |
| `missing` | Public locate request | That FBI missing/kidnap poster. Not a criminal accusation; do not approach. Missing-child posters typically NCMEC **1-800-THE-LOST**. |
| `wanted` | Public warrant-style ask (incl. parental-kidnapping **abductor**) | That FBI wanted poster. Do not approach. Typically **1-800-CALL-FBI** / tips.fbi.gov. |

User may disable a whole class (missing-only is valid). May not cherry-pick names. **Same locked gate** for both — do not loosen it “to find more missing people.”

Sampled 28 Sep 2026: `https://api.fbi.gov/wanted/v1/list` = **1253** posters, all imaged. `subjects` query does **not** filter; classify from each item. Skip status in {resolved, captured, deceased, surrendered} (29 that day).

**`missing`:** `Kidnappings and Missing Persons` (135; FBI index listed 129), `ViCAP Missing Persons` (166).

**`wanted`:** named warrant-style posters in the **same** API — Ten Most Wanted, Cyber, Terrorists, named Crimes Against Children offenders, Fraudster / white-collar, Criminal Enterprise, Indian Country, Law Enforcement Assistance, Parental Kidnapping abductors (10), etc. Not a second database.

Children on a kidnap poster = `missing`; abductor parent = `wanted`. Example: [Kandil children](https://www.fbi.gov/wanted/kidnap/amina-and-belel-kandil) vs [Ahmed Kandil](https://www.fbi.gov/wanted/parental-kidnappings/ahmed-abdallah-taha-kandil).

Age-progressed NCMEC composites: allowed if that’s the public byte; **label** them; do not train. If the source drops or marks resolved, matching that record **stops**. Show class counts on the inspectable gallery. This list is tiny and incomplete.

### Skip even on the FBI list

| Tag | n (28 Sep 2026) | Why |
| --- | --- | --- |
| ECAP / Endangered Child Alert | ~12 | Unknown CSAM suspects. **Never.** |
| ViCAP Unidentified Persons | 57 | Unidentified deceased. Out. |
| Seeking Information unnamed / CCTV | 250 mixed | Unknown-suspect search. **No.** Named people also tagged kidnap/ViCAP-missing are already `missing`. |
| ViCAP Homicides / Sexual Assaults if victim reconstruction or unnamed series | 46 mixed | Skip unless clearly a **named** wanted person. |

No public national stolen-car/plate gallery. OCR a plate only if an enabled record publishes it.

## Amber is not a face database

Amber/Silver/WEA are short broadcasts (often plate + description, often **no** usable face). Do not replace, spoof, or delay WEA; do not push “OpenWorld matched an Amber”; do not treat [OpenFEMA IPAWS](https://www.fema.gov/openfema-data-page/ipaws-archived-alerts-v1) as live (24h delay). If that person already has an FBI missing poster, they are already `missing`.

**v1 `alert` overlay (parked):** hours-scale TTL from a *cleared* feed with photo and/or plate. Same user-initiated local scan. Still no push, auto-report, or confrontation.

## Other sources (28 Sep 2026)

No third v0 class. Extra FBI *tags* are still `missing`/`wanted` from the same pull.

| Source | Pullable? | Verdict |
| --- | --- | --- |
| [FBI Wanted API](https://www.fbi.gov/wanted/api) | JSON, no key | **v0.** Only first-party US face-poster API. |
| NCMEC Poster API | Yes, after credentials | **v1 after FR notice** (Oct 2024 terms). Current-content only. Do not scrape the map. Real missing-child corpus. |
| NamUs | No public photo API | **No scrape.** |
| INTERPOL Red/Yellow/UN `ws-public.interpol.int` | JSON (~6388 Red, ~11693 Yellow; 160-hit cap; most notices police-only) | **Not until written photo-reuse permission.** [Terms](https://www.interpol.int/en/Who-we-are/Terms-of-use) forbid reusing wanted/missing pictures and robots. A Red Notice is not a warrant. |
| USMS / DEA (~540 HTML) / ATF / ICE / USSS / DSS | HTML only | **v1+**, first-party after terms. Never OpenSanctions. |
| Europol [eumostwanted.eu](https://eumostwanted.eu/) (~47) | HTML | Photos need the **uploading state’s** permission. |
| Canada’s Missing, state/local missing + Silver | Search UI / HTML | **v1+**, one source at a time. No civic scrapers. Never a national scrape. |
| OpenFEMA IPAWS | Yes | Usually no face. Not live Amber. |
| NCIC (incl. stolen vehicle/plate) | Never public | **Never.** |
| OFAC / sanctions names | Yes | **No.** Name screening, not posters. |
| OpenSanctions mirrors | Yes | **No.** Third-party face CDN. |
| Sex-offender / NSOR | — | **Never.** |
| Foreign missing APIs (e.g. Korea Safety Dream) | Sometimes | Not v0 geography. Same FR/terms bar. |

## Missing-person copy

On every `missing` candidate, plus [trust](trust.md): not accused of a crime (linked `wanted` is a **separate** record); age-progressed/old photos can look unlike them now; **do not approach, follow, or detain — especially a child**; parental kidnapping is often family conflict; empty scan ≠ they weren’t in the file.

## Parked vs never

**v0:** FBI missing + named wanted; class on/off; clothing/last-seen **text** display-only; published plate/vehicle as cues; labeled age-progressed refs.

**v1+:** licensed Amber/Silver `alert`; NCMEC after notice; other federal HTML most-wanted first-party; INTERPOL with photo-reuse permission; Europol with member-state permission; state clearinghouses one at a time.

**Never:** unknown-suspect Seeking Information; unidentified remains; sex-offender registries; OFAC names; user “find my family”; push/auto-tip; live public-space Amber camera; clothing/gait/voice as ID; training on poster photos.
