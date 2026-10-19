# OpenWorld legal-risk review

Prepared 2026-10-19 for the owner, OneX Engineering.

> **This is not legal advice, and the author is not a lawyer.** This document is a structured risk review, written to help the owner brief a licensed attorney and to decide what to change before release. It does not create an attorney-client relationship. Nothing here says OpenWorld is lawful anywhere. Laws, court decisions and store policies change; several items below changed in 2026. Have a licensed attorney confirm every conclusion before a public release.
>
> **Questions that most need a licensed attorney** (details in [section 12](#12-questions-for-a-licensed-attorney)):
>
> 1. **False candidates.** If a false "Possible candidate" leads someone to confront, report, or publicly accuse a stranger, can OneX be sued (defamation, false light, negligence, product liability, state consumer-protection law)? Are the current copy, a Terms of Use, and insurance enough? (US litigation counsel.)
> 2. **EU, UK, and other countries.** Does publishing ready-to-run binaries on GitHub or in stores count as "placing on the market" a high-risk remote biometric identification system under the EU AI Act, or as controlling biometric processing under GDPR? Is limiting store countries enough? (EU and UK data-protection and AI counsel.)
> 3. **The FBI name.** Does the final app name, store listing, screenshots, and UI comply with 18 U.S.C. §709 and §701? Should OneX ask the FBI for written permission or a no-objection letter? (US government or regulatory counsel.)
> 4. **State biometric laws.** Given the Seventh Circuit's August 2026 *G.T. v. Samsung* decision and the pending *Hazlitt v. Apple* trial, is OneX outside Illinois BIPA, Texas CUBI, Washington's My Health My Data Act, and Colorado's biometric rules because the processing stays on the user's device? (US privacy counsel.)
> 5. **Store strategy.** How to present the app to Apple and Google given Apple guidelines 1.7 and 5.1.1(viii) and Google's misleading-claims policy. (Counsel with app-store experience.)

**How this was researched.** Every claim was checked by web search on 2026-10-19. Direct page fetches failed from this environment (no host resolved), so some citations point to law-firm summaries, news reports or court-docket pages rather than the primary text. Those are marked **[secondary]**. An attorney should read the primary text before relying on any of them. Each citation gives the date of the source or the event.

---

## Executive summary

OpenWorld's design already avoids the worst legal patterns in facial recognition. It has no camera and no live scanning. It uploads nothing, runs no server, keeps no gallery of users' faces, sends no reports, and shows no raw scores or near-miss lists. It skips the FBI's unknown-suspect and unidentified-remains posters. Real posters stay off until a measured curve exists. Most of the remaining legal risk comes from four things:

1. **Being treated as an official or law-enforcement tool**, through the FBI name, store listing, or user behavior.
2. **A false candidate harming a real person.**
3. **Distribution outside the US.**
4. **Store review.**

### Top risks and recommended decisions

| # | Risk | Rating | Recommended decision |
| --- | --- | --- | --- |
| 1 | **EU/EEA, UK, Canada, Australia distribution.** Under the EU AI Act, an app that identifies people in media against a reference database is likely a high-risk "remote biometric identification system". The open-source exemption does not cover high-risk systems. Under GDPR, biometric data and criminal-offence data have no workable legal basis for private users. The UK ICO has shown it can reach foreign companies. | **High** | **Launch in the US only.** Exclude the EU/EEA, UK, Switzerland, Canada and Australia from store availability. Say on GitHub releases that the app is not offered there. Revisit with local counsel before December 2027, when the AI Act's high-risk duties start. |
| 2 | **App store rejection or removal.** Apple guideline 5.1.1(viii) bars apps that compile personal information from public databases. Guideline 1.7 requires crime-reporting apps to involve local law enforcement. Google bars false government affiliation. | **High** (business risk, not liability) | Write the listing to the rules in [section 8](#8-app-stores). Enroll as an organization. Ask Apple for pre-submission guidance. Keep desktop builds and the GitHub source available as a fallback. |
| 3 | **A false candidate harms someone.** This could lead to defamation or false-light claims, negligence or product-liability claims, or FTC "unfairness" scrutiny. At about 8e-7 false matches per comparison, a 2,000-face video against about 1,000 posters still expects about 1.6 false candidates. Real posters are lower-quality than LFW, so the real rate is probably higher, and it is unmeasured across demographic groups. | **Medium-High** | Keep the strict cutoff. Before real posters are on: (a) finish the demographic and poster-like evaluation; (b) add the copy fixes in [section 10](#10-copy-review-what-works-and-what-to-change); (c) add Terms of Use with an acceptable-use clause; (d) consider a per-scan guard when chance alone predicts at least about 0.5 false candidates. This touches a locked decision, so it is the owner's call. (e) Get technology E&O / media-liability insurance. |
| 4 | **FBI name and seal** (18 U.S.C. §709, §701). Using "FBI" in a way that suggests FBI approval is a federal crime, and so is reproducing the seal or badge. | **Medium** if careless; **Low** with the rules below | Never put "FBI" in the app name, subtitle, icon, keywords, or developer name. Never show the seal or badge, including in screenshots of poster pages. Add "OpenWorld is not part of, or endorsed by, the FBI or any government agency" to the start screen, the leaving sheet, About, and the listing. Consider a courtesy letter to the FBI Office of Public Affairs. |
| 5 | **State biometric laws** (Illinois BIPA, Texas CUBI, Washington MHMDA, Colorado). | **Medium-Low** | The 7th Circuit held in Aug 2026 that a company whose software keeps face templates only on the user's device does not "possess" or "collect" them under BIPA. Protect that position: no cloud sync, no telemetry, no crash reports with face data, templates never written to disk. Also publish a short biometric data policy (retention: 7 days, on device) and a Washington "consumer health data" policy that says nothing is collected. Both are cheap. |
| 6 | **FTC deception claims about privacy and accuracy.** | **Medium** | Fix "OpenWorld does not contact an agency." It is not literally true once Update posters and Open FBI page exist. Back "Nothing is uploaded" with a published packet-capture test. Say that LFW is celebrity photos, not posters. Never claim accuracy, fairness, or "no bias". |
| 7 | **Open-source licensing.** The Windows bundle copies MSYS2's FFmpeg, which MSYS2 labels GPL-3.0-or-later, plus every DLL it pulls in. The app has no third-party notices screen. Model training-data provenance is unclear. The Accurate bundle names research-only weights. | **Medium** (fixable) | Build an LGPL-only, decode-only FFmpeg for Windows, as the Flatpak already does. Generate a notices file and an in-app Licenses screen. Document the training data for YuNet and SFace. Do not ship any Accurate weights trained on MS1M, Glint360K, WebFace or CASIA-WebFace without a license review. |
| 8 | **Poster image rights.** Some FBI poster photos were taken by families, local police, or NCMEC, so they are not US-government works. NCMEC's terms restrict reuse. | **Low-Medium** | Skip or flag posters whose images carry a third-party or NCMEC credit until counsel clears them. Show each poster's source link. Never redistribute poster images in releases (the design already avoids this). |
| 9 | **Export controls.** | **Low** | Publicly available open-source and free-download software is generally "not subject to the EAR". The 2024 proposal to control facial-recognition systems was still a proposal in every source found. Re-check before release. |
| 10 | **Section 230.** | **No protection expected** | Treat every candidate card as OneX's own speech. Plan as if Section 230 will not help. |

### Decisions to make now

1. **US-only store launch** (risk 1).
2. **Listing and naming rules for the FBI name** (risk 4), adopted before any store submission or public announcement.
3. **Copy fixes** in [section 10](#10-copy-review-what-works-and-what-to-change), mainly the non-affiliation line, the "contact an agency" line, the follow/detain line for missing persons, and LFW qualifiers.
4. **Release-blocking legal artifacts:** Terms of Use, privacy policy, biometric data policy, Washington consumer health data policy, and third-party notices.
5. **Windows FFmpeg rebuild** (risk 7).
6. **Keep the zero-telemetry, no-sync architecture as a legal requirement**, not just a preference, because it underpins the BIPA defense (risk 5).

---

## Contents

1. [What OpenWorld does, in legal terms](#1-what-openworld-does-in-legal-terms)
2. [US federal law](#2-us-federal-law)
3. [US state and local law](#3-us-state-and-local-law)
4. [Civil liability for false candidates](#4-civil-liability-for-false-candidates)
5. [EU and UK](#5-eu-and-uk)
6. [Canada, Australia, and elsewhere](#6-canada-australia-and-elsewhere)
7. [Geo-restriction recommendation](#7-geo-restriction-recommendation)
8. [App stores](#8-app-stores)
9. [Open-source licensing and model provenance](#9-open-source-licensing-and-model-provenance)
10. [Copy review: what works and what to change](#10-copy-review-what-works-and-what-to-change)
11. [Product-design mitigations](#11-product-design-mitigations)
12. [Questions for a licensed attorney](#12-questions-for-a-licensed-attorney)
13. [Launch checklist](#13-launch-checklist)
14. [Sources](#14-sources)

---

## 1. What OpenWorld does, in legal terms

These facts drive most of the analysis. Sources: [README](../README.md), [decisions](decisions.md), [data flow](data-flow.md), [threat model](threat-model.md), [record classes](record-classes.md), [UX](ux.md), `core/openworld-core/src/copy.rs`, `android/app/src/main/AndroidManifest.xml`, `desktop/packaging/`.

| Fact | Why it matters legally |
| --- | --- |
| The user picks a file they already have. There is no camera, no watch folder, and no background scan. | It is not live or real-time surveillance. That keeps the app out of the EU AI Act's real-time ban and away from Portland-style bans on businesses using face recognition on premises. |
| Faces are detected, and some are embedded, **on the user's device**. Templates stay in memory and are never written to disk. Face crops, and candidate frames up to 1,600 px, are saved locally for at most 7 days. | This is the core of the "no possession or control" argument under BIPA ([§3.1](#31-illinois-bipa)). Crops are photographs; templates are biometric identifiers. |
| OneX runs no server and receives no media, crops, templates, results, analytics, or crash reports. The Android manifest sets `allowBackup=false`; its INTERNET permission is used only by Update posters, which reaches api.fbi.gov and fbi.gov. | It is hard to call OneX a "collector", "possessor" or "processor" of anything. But laws that turn on "determining the purposes and means" of processing (GDPR, Washington MHMDA, Colorado) can still reach a software designer. |
| The reference gallery is the FBI's public Wanted API. It is fetched by the device and limited to named missing and wanted persons. ECAP, unidentified remains, and unnamed "Seeking Information" posters are skipped. | It is a targeted source, not untargeted scraping (relevant to AI Act Art. 5(1)(e)). Poster photos may include third-party copyrighted images. |
| A match above a strict cutoff shows "Possible candidate. Not an identification." A tap, after a leaving sheet, opens the poster's fbi.gov page. No report is sent. | The output is a statement **by OneX's software** linking a real, identifiable person in the user's file to a wanted or missing poster. This is the main source of civil-liability risk. |
| Bystanders' faces are processed without their knowledge or consent. | Consent-based biometric laws (BIPA, CUBI, GDPR Art. 9) assume someone collects consent. Here nobody can. |
| Measured false-match rate is about 8e-7 per comparison on LFW (celebrity photos). It misses most cross-age matches. There is no demographic breakdown. | Accuracy claims must be backed by evidence (FTC). An unknown demographic error rate is a known unaddressed risk (FTC biometric policy statement). |
| Free, Apache-2.0, published by a US company. Ships on iPhone, Mac, Android, Linux and Windows. | Free does not take it out of the FTC Act, the EU AI Act, or GDPR. The EU AI Act's open-source exemption does not apply to high-risk systems. |

---

## 2. US federal law

### 2.1 FTC Act §5: deceptive and unfair practices

**Rating: Medium.**

**The law.** Section 5 of the FTC Act prohibits "unfair or deceptive acts or practices in or affecting commerce." The FTC applies it to facial-recognition vendors and users:

- **Rite Aid (announced Dec 19, 2023).** The FTC banned Rite Aid from using facial recognition for surveillance for five years. It said Rite Aid deployed the technology without reasonable safeguards. The system produced thousands of false-positive matches that led staff to follow and accuse customers, disproportionately women and people of color. The order requires testing, image-quality controls, notice, and retention limits ([FTC press release, Dec 2023](https://www.ftc.gov/news-events/news/press-releases/2023/12/rite-aid-banned-using-ai-facial-recognition-after-ftc-says-retailer-deployed-technology-without-reasonable-safeguards); [Fierce Healthcare, Dec 2023](https://fiercehealthcare.com/regulatory/rite-aids-reckless-use-ai-facial-recognition-tech-earns-5-year-ban-ftc) **[secondary]**).
- **IntelliVision (proposed Dec 3, 2024; finalized Jan 2025 by a 5-0 vote).** The FTC said a facial-recognition vendor lacked evidence for claims of top accuracy and "zero gender or racial bias". The order bars accuracy and bias claims without competent and reliable testing ([FTC, Jan 2025](https://search.ftc.gov/news-events/news/press-releases/2025/01/ftc-finalizes-order-prohibiting-intellivision-making-deceptive-claims-about-its-facial-recognition); [Covington Global Policy Watch, Dec 2024](https://www.globalpolicywatch.com/2024/12/intellivision-settles-ftc-allegations-regarding-its-facial-recognition-technology/) **[secondary]**). The final vote came during the change of FTC leadership. That suggests accuracy-claim enforcement in facial recognition is not a partisan priority likely to disappear.
- **Biometric policy statement (May 18, 2023).** The FTC listed examples of deception, such as unsubstantiated claims about the accuracy or fairness of biometric technology and false statements about data collection. It also listed examples of unfairness: failing to assess foreseeable harms before deployment, failing to address known risks such as demographic differences in error rates, surreptitious collection, and weak oversight. Policy statements are not law, but they show how the FTC reads §5 ([Wiley, May 2023](https://www.wiley.law/wiley-connect/ftc-issues-policy-statement-on-biometric-information-signaling-a-new-enforcement-priority) **[secondary]**; [Steptoe, 2023](https://www.steptoe.com/en/news-publications/ftc-policy-statement-on-use-of-biometric-information-technology-lays-the-groundwork-for-future-enforcement-activity.html) **[secondary]**). No source found on 2026-10-19 reported that the statement has been withdrawn ([search summary of 2025 practitioner material](https://www.privacysecurityacademy.com/wp-content/uploads/2025/05/PS-Spring_The-FTCs-Privacy-Program_-Freeman-and-Bowling-May-8-2025.pdf) **[secondary]**). Confirm its status before release.

**How it applies to OpenWorld.**

- **Deception about privacy.** OpenWorld makes absolute claims: "Nothing is uploaded.", "This file stays on this device.", "OpenWorld does not contact an agency." The FTC treats express claims as material. Each must be literally true in every build, including later builds with Update posters on.
  - "OpenWorld does not contact an agency" will be **untrue** once Update posters contacts api.fbi.gov, an FBI server, and once Open FBI page sends the user to fbi.gov. See the fix in [§10](#10-copy-review-what-works-and-what-to-change).
  - When posters update, the FBI's servers will see the device's IP address and the time of the request. When the user opens a poster, fbi.gov will see which poster they viewed. The disclosure should say this.
- **Deception about accuracy.** "Measured on LFW" and the chance line ("chance alone gives about X false candidates (measured on LFW)") are accuracy claims. LFW is celebrity press photos, not posters or the user's video. The repo's own [README](../README.md) says so. A reasonable user could read the chance line as the real-world rate. It needs a qualifier. Never claim fairness, "no bias", or "accurate".
- **Unfairness.** The people harmed by a false candidate are not the app's users, and they cannot avoid the harm. The FTC's unfairness test asks whether the harm is substantial, not reasonably avoidable, and not outweighed by benefits. The project already does several things the policy statement asks for:
  - assesses harms in writing ([threat model](threat-model.md), [feasibility](feasibility.md));
  - uses a strict cutoff;
  - suppresses scores and near-misses;
  - writes non-accusatory copy.

  The visible gap is the **demographic evaluation**, which [decisions §"Still open"](decisions.md) lists as open. The policy statement names unaddressed demographic differentials as a risk factor. **Do not turn on real posters until a demographic evaluation exists, or until the UI plainly says the error rate across groups is unknown.**

**Mitigations.**

- Copy changes in [§10](#10-copy-review-what-works-and-what-to-change).
- Publish the airplane-mode and packet-capture proofs that [trust.md](trust.md) already plans, so "Nothing is uploaded" is substantiated.
- Keep a dated substantiation file: curves, test sets, and the exact build tested. That is what IntelliVision lacked.
- Keep marketing within the "may claim" column of [trust.md](trust.md).

### 2.2 18 U.S.C. §709 and §701: the FBI name and seal

**Rating: Medium if careless, Low with the rules below. This is the most important federal item for naming and marketing.**

**The law.**

- **18 U.S.C. §709** makes it a federal misdemeanor to knowingly use the words "Federal Bureau of Investigation", the initials "F.B.I.", or any colorable imitation without the FBI Director's written permission, in connection with an advertisement, publication, broadcast, or "other production", "in a manner reasonably calculated to convey the impression" that the FBI approved, endorsed, or authorized it. The penalty is a fine or up to one year in prison ([statute, Cornell LII](https://www.law.cornell.edu/uscode/text/18/709)). The FBI provision was added in 1954 to protect the Bureau's name from commercial exploitation ([UNT Monthly Catalog record of the 1954 amendment](https://mocat.library.unt.edu/catalog/717-16803) **[secondary]**). *The statute text could not be fetched on 2026-10-19; the wording above should be checked against the official text.*
- **18 U.S.C. §701** criminalizes making, selling, or possessing any badge, identification card, or insignia of a federal agency, or a colorable imitation, except as authorized by regulation ([statute, Cornell LII](https://www.law.cornell.edu/uscode/text/18/701)). Courts read §701 as aimed at deceptive use of official authority ([2010 letter discussing *United States v. Goeltz*](https://en.wikisource.org/wiki/Page:20100803-wiki-LetterToLarson.pdf/1) **[secondary]**).
- **The FBI's own position.** The FBI's website says its published information may generally be copied. It also says the FBI seal, name, and initials are restricted under §701 and §709 ([FBI privacy and website policy page](https://www.fbi.gov/privacy-policy)). The FBI has publicly objected when people used its seal in ways that suggested it had created or authorized something ([Federal Register, Anti-Piracy Warning Seal Program, July 13, 2012](https://www.federalregister.gov/documents/2012/07/13/2012-16506/federal-bureau-of-investigation-anti-piracy-warning-seal-program)). The seal is public domain for copyright purposes, but these statutes restrict its use separately ([Wikimedia Commons file notes](https://commons.wikimedia.org/wiki/File:Seal_of_the_Federal_Bureau_of_Investigation.svg) **[secondary]**).

**How it applies.** Describing the data source ("public FBI missing and wanted posters") is truthful and needed. That descriptive use is not what §709 targets. The risk comes from **implied endorsement**. Places where it could creep in:

| Surface | Current state | Rule |
| --- | --- | --- |
| App name | "OpenWorld". Good. | Never "FBI Scanner", "FBI Match", "OpenWorld FBI", or similar. |
| Icon | Generic face-in-frame on blue (checked `apple/App/Resources/Assets.xcassets`). Good. | No badge, shield, star, seal, eagle, or gold-on-navy "agency" look. |
| Store subtitle, keywords, promo text | Not written yet. | No "FBI" in the subtitle or keywords. Apple 2.3.7 also bars other entities' names in keywords. Mention the FBI only in the description body, next to the non-affiliation line. |
| Screenshots | Not made yet. | Use the demo pack only. Never show a real poster page, the FBI seal, or fbi.gov headers. |
| UI | The start line says "against public FBI missing and wanted posters". Fine. There is no non-affiliation line anywhere (searched the repo for "affiliat" and "endorse": no hits). | Add a non-affiliation line ([§10](#10-copy-review-what-works-and-what-to-change)). |
| Developer or seller name | OneX Engineering. Good. | Never a name implying a public agency. |
| Press, social, README | README is neutral apart from the tagline. | Never say "partnered with", "working with", "approved by", or "for" the FBI. |

**Mitigations.**

- Adopt the table above as a written brand rule.
- Add the non-affiliation line.
- Consider a short courtesy letter to the FBI Office of Public Affairs describing the app, asking whether the FBI objects, and confirming the app does not imply endorsement. Counsel should decide whether to ask for written permission under §709. Permission is probably unnecessary for descriptive use, and the FBI may simply not answer.
- Related: **18 U.S.C. §912** (impersonating a federal officer) applies to people, not apps. It matters only if marketing encouraged users to act "for the FBI". The copy should never do that.

### 2.3 FBI website and API terms; public-domain status of poster images

**Rating: Low-Medium.**

- **17 U.S.C. §105** denies copyright to "any work of the United States Government", meaning works prepared by federal officers or employees as part of their duties. The government may still hold copyrights transferred to it ([statute, govinfo](https://www.govinfo.gov/content/pkg/USCODE-2020-title17/html/USCODE-2020-title17-chap1-sec105.htm)). Work by contractors, or photos taken by private people, are not automatically public domain ([OSU Libraries copyright guide](https://library.osu.edu/site/copyright/?p=511) **[secondary]**).
- **What that means for posters.** FBI poster text and FBI-taken photos are generally public domain. Many missing-person photos, however, come from families. Some are booking photos taken by state or local police, and some are age-progressions made by NCMEC. Those may be copyrighted by someone else.
- **NCMEC.** NCMEC's terms say NCMEC owns the content in its Poster API service. Unless a use is permitted, they bar commercial use, redistribution, derivative works, and data mining ([NCMEC terms](https://www.missingkids.org/termsandconditions); [NCMEC poster API registration](https://www.missingkids.org/gethelpnow/search/poster-api-registration)). [record-classes.md](record-classes.md) already defers NCMEC to v1, after a notice about facial recognition. Good. Computing a face template from an NCMEC age-progression that appears on an FBI poster could still be argued to be a "derivative" or "data mining" use.
- **FBI API terms.** The FBI describes the Wanted API as a public JSON service with no key ([FBI Wanted API page](https://www.fbi.gov/wanted/api)). No separate API license text was found on 2026-10-19; the page itself could not be fetched. A general "public information may be copied" statement applies to FBI-published material ([FBI privacy and website policy page](https://www.fbi.gov/privacy-policy)).

**Analysis.** OpenWorld does not redistribute poster images. Each device fetches them from the FBI, uses them temporarily for matching, and deletes them after 7 days. That transient, non-commercial, informational use, which links back to the source, is a strong fair-use position for third-party photos. It is not certain.

**Mitigations.**

- **Before real posters:** have the poster ingester record each image's credit line, if the API provides one, and skip images credited to NCMEC or another third party until counsel clears them.
- Keep the "no poster images in releases" rule.
- Respect API rate limits.
- Drop resolved or captured posters, which [record-classes.md](record-classes.md) already requires.

### 2.4 Export controls (EAR)

**Rating: Low.**

- **Proposed controls.** In July 2024, BIS proposed adding "facial recognition systems" to ECCN 3A981, with software under 3D980 and technology under 3E980. The control would be under Crime Control reasons and aimed at systems designed for mass surveillance and crowd scanning. Detection, authentication, and personal-device access uses would be excluded ([Federal Register, July 29, 2024](https://www.federalregister.gov/documents/2024/07/29/2024-16498/export-administration-regulations-crime-controls-and-expansionupdate-of-us-persons-controls); [Akin Gump, 2024](https://www.akingump.com/en/insights/alerts/bis-proposes-significant-amendments-to-the-ears-us-person-and-end-user-controls-and-a-control-on-facial-recognition-items) **[secondary]**). **Searches on 2026-10-19 found no final rule.** Re-check the Federal Register before release.
- **Published software.** Under 15 C.F.R. §734.3(b) and §734.7, software that is "published", meaning available to the public without restrictions on further distribution, is "not subject to the EAR". BIS gives free downloadable apps as an example of publicly available items. Exceptions include certain encryption items and firearm files ([eCFR §734.7](https://www.ecfr.gov/current/title-15/subtitle-B/chapter-VII/subchapter-C/part-734/section-734.7); [Linux Foundation, EAR and open source](https://bestpractices.linuxfoundation.org/export/ear-oss.html); [BIS, encryption items not subject to the EAR](https://www.bis.gov/learn-support/encryption-controls/encryption-items-not-subject-to-ear)). Publication is not a cure-all: an item is not published merely because it calls open-source code ([Centre Law Group](https://centrelawgroup.com/blog/publication-is-not-a-panacea/) **[secondary]**).
- **Encryption.** OpenWorld uses only the operating system's HTTPS, so it should be able to answer Apple's export-compliance question as "exempt".

**Mitigations.** Keep source and binaries freely downloadable, without registration. Re-check the 3A981 rulemaking before release. If OneX ever offers paid support or custom builds to foreign police or security agencies, get export counsel first: the 2024 package also proposed new controls on US-person support to such end users.

### 2.5 Section 230

**Rating: No meaningful protection. Plan as if it does not apply.**

Section 230(c)(1) protects a provider only for information "provided by another information content provider". Anyone responsible "in whole or in part" for creating or developing the content is an information content provider ([47 U.S.C. §230, Cornell LII](https://www.law.cornell.edu/uscode/text/47/230)). A candidate card is generated by OpenWorld's own models, cutoff, and wording. It is OneX's content, not a third party's.

- In *Anderson v. TikTok* (3d Cir. Aug 27, 2024), the court held that TikTok's own algorithmic recommendations were its first-party activity, outside §230 ([opinion](https://www2.ca3.uscourts.gov/opinarch/223061p.pdf)).
- Commentators read that reasoning to leave AI output largely unprotected, especially when claims are framed as product-design defects ([JD Supra, "Section 230 and Generative AI"](https://www.jdsupra.com/legalnews/look-who-s-talking-section-230-and-5466365/) **[secondary]**).
- §230 might cover republishing FBI poster text if the poster itself were wrong. It will not cover the match.

---

## 3. US state and local law

### 3.1 Illinois BIPA

**Rating: Medium-Low for OneX if the architecture stays as is; Medium for Illinois users scanning strangers.**

**The law.** BIPA regulates "private entities" that collect, capture, possess, or obtain "biometric identifiers", including a "scan of face geometry", or "biometric information".

- **Photographs are excluded**, but data derived from a photo can still be a face-geometry scan ([740 ILCS 14/10](https://lig.ilga.gov/legislation/ilcs/documents/074000140K10.htm); [Mondaq on photo-derived data](https://webiis10.mondaq.com/unitedstates/privacy-protection/1188272/breaking-federal-judge-in-illinois-affirms-that-bipa-extends-to-information-derived-from-photographs) **[secondary]**).
- **Private entity** includes "any individual". Check the current text of §10.
- **Duties:** written consent, a public retention policy, and limits on disclosure. There is a private right of action, with statutory damages of $1,000 per negligent and $5,000 per intentional or reckless violation.
- **2024 amendment.** SB 2979 (Public Act 103-0769, effective Aug 2, 2024) limits recovery to one violation per person when the same data is collected repeatedly by the same method. It also allows electronic signatures ([ILGA, Public Act 103-0769](https://ilga.gov/legislation/publicacts/103/103-0769.htm)). On Apr 1, 2026, the Seventh Circuit held that the damages limit applies to pending cases (*Clay v. Union Pacific*) ([Paul Hastings](https://paulhastings.com/insights/ph-privacy/7th-circuit-confirms-bipa-amendment-has-retroactive-application) **[secondary]**; [ABA Business Law Today, May 2026](https://www.americanbar.org/groups/business_law/resources/business-law-today/2026-may/7th-circuit-holds-bipa-damages-remedy-applies-retroactively) **[secondary]**).

**On-device processing: the key 2026 cases.**

- ***G.T. v. Samsung Electronics America*, No. 25-1120 (7th Cir. Aug 7, 2026)** — favorable. The court affirmed dismissal of a BIPA suit over the Samsung Gallery app's on-device face grouping. It held that "possess", "collect", "capture" and "obtain" all require the defendant to have some control over the data. It was not enough that Samsung designed the software and sold the device, because the templates stayed on the user's device under the user's control. A privacy policy saying data might be collected did not establish control either ([Justia, opinion page](https://law.justia.com/cases/federal/appellate-courts/ca7/25-1120/25-1120-2026-08-07.html); [Mayer Brown, Aug 2026](https://www.mayerbrown.com/en/insights/publications/2026/08/seventh-circuit-holds-that-bipa-does-not-reach-biometric-data-that-remains-on-a-users-device) **[secondary]**; [WLF, Sept 1, 2026](https://www.wlf.org/2026/09/01/wlf-legal-pulse/possess-or-collect-under-illinois-bipa-seventh-circuit-resolves-trial-court-split/) **[secondary]**). This aligns with *Bhavilai v. Microsoft* (N.D. Ill. 2024).
- ***Hazlitt v. Apple*, No. 3:20-cv-00421 (S.D. Ill.)** — a caution. In 2020–21 the court let BIPA claims over Apple Photos' on-device face recognition proceed, saying on-device processing "does not necessarily absolve" the data ([JD Supra](https://www.jdsupra.com/legalnews/illinois-federal-court-rules-apple-may-1793870/) **[secondary]**). In an order dated Sept 30, 2026 and unsealed Oct 8, 2026, the court sent the case to a jury trial set for April 2027. It reportedly held that Apple can "control" data it syncs to its own servers even when end-to-end encrypted, and that the ability to lock, move, or delete data can be control. Claims based only on cropped face images were dropped ([Law360, Oct 2026](https://www.law360.com/articles/2536320/apple-faces-trimmed-down-bipa-trial-over-faceprint-claims) **[secondary]**; [PPC Land](https://ppc.land/apple-loses-bid-to-end-illinois-suit-over-faceprints-in-iphone-photos/) **[secondary]**; [ID Tech](https://idtechwire.com/apple-asks-judge-to-end-photos-faceprint-class-action-citing-samsung-ruling/) **[secondary]**).
- **Clearview AI**, whose business was scraping a face database, settled the BIPA multidistrict litigation for a 23% equity stake, given final approval on Mar 20, 2025. An appeal by objectors was pending in the Seventh Circuit ([Bloomberg Law](https://news.bloomberglaw.com/class-action/clearview-ai-gets-settlement-approved-in-face-scan-privacy-case) **[secondary]**).

**Analysis.**

- **OneX.** OpenWorld is closer to *Samsung* than to *Hazlitt*:
  - no cloud sync;
  - backups excluded on mobile (`allowBackup=false`, and iOS results excluded from backup per [data-flow.md](data-flow.md));
  - templates never written to disk;
  - no telemetry;
  - no remote ability to lock, move, or delete user data.

  The Seventh Circuit's ruling binds federal courts in Illinois. It does not bind Illinois state courts, which could disagree.
- **Bystanders.** No consent is possible. That is why staying outside BIPA's definitions matters more than any notice text.
- **Saved crops.** Crops are photographs, which BIPA excludes. *Hazlitt* dropped claims based only on cropped images. Keep templates out of storage.
- **Users.** An Illinois user is an "individual", and so possibly a "private entity" that controls templates on their own device. Suits against individuals are rare, but OneX should not imply that the user's use is always lawful.

**Mitigations (make them release requirements).**

1. No cloud sync, no telemetry, and no crash reports containing media-derived data, ever.
2. Templates never persisted. Add a test, as the threat model implies.
3. No remote kill switch or remote delete of user results.
4. A short public biometric data policy: what is processed, that it stays on device, the 7-day retention, and how to delete. It costs nothing and matches what BIPA §15(a) would require if it applied.
5. A Terms of Use line telling users they are responsible for lawful use where they live.

### 3.2 Texas CUBI

**Rating: Low-Medium.**

- **The law.** Texas's Capture or Use of Biometric Identifier Act bars capturing a biometric identifier "for a commercial purpose" without notice and consent, and limits sale and retention. Only the Attorney General can enforce it, with penalties up to $25,000 per violation ([Tex. Bus. & Com. Code §503.001](https://statutes.capitol.texas.gov/Docs/BC/htm/BC.503.htm); [ABA, Winter 2026](https://www.americanbar.org/groups/tort_trial_insurance_practice/resources/brief/2026-winter/everythings-bigger-texas-including-its-data-privacy-initiative) **[secondary]**). "Commercial purpose" is not defined (same ABA source).
- **Enforcement.** The AG obtained $1.4 billion from Meta over facial "tag suggestions" (July 2024). A reported $1.375 billion settlement with Google (May 2025) resolved claims that included CUBI ([Texas Lawbook](https://texaslawbook.net/texas-obtains-1-4b-settlement-from-meta-over-facial-recognition-data/) **[secondary]**; [ITECS guide](https://itecsonline.com/post/texas-biometric-identifier-act-cubi-a-compliance-guide-for-businesses-capturing-fingerprints-faces-or-voices) **[secondary]**).
- **TRAIGA.** Texas's AI law (HB 149) took effect Jan 1, 2026. No primary source confirmed that it changed CUBI.
- **Analysis.** OneX does not capture anything; the user's device does, for the user's non-commercial purpose, and OneX earns nothing. The Texas AG is aggressive and has used CUBI against software makers whose servers processed faces. OpenWorld's facts are much weaker for a claim. The same architecture rules as §3.1 apply.

### 3.3 Washington: RCW 19.375 and the My Health My Data Act

**Rating: Low (RCW 19.375); Low-Medium (MHMDA, unsettled).**

- **RCW 19.375.** This law applies only to "enrolling" a biometric identifier in a database "for a commercial purpose". It is enforced only by the AG ([RCW 19.375 text](https://wa-law.org/rcw/19_business_regulations%E2%80%94miscellaneous/19.375_biometric_identifiers.html) **[secondary mirror]**). OpenWorld enrolls nothing: poster templates are made from FBI images, and user faces are never stored as templates. Low.
- **MHMDA (RCW 19.373).** It treats biometric data as "consumer health data". It reaches a "regulated entity" that "alone or jointly determines the purpose and means" of collecting or processing such data, with "collect" defined broadly to include processing "in any manner". It gives a private right of action through the Consumer Protection Act ([RCW 19.373](https://app.leg.wa.gov/RCW/default.aspx?cite=19.373)). The first class action, *Maxwell v. Amazon* (W.D. Wash., filed Feb 10, 2025), had no reported merits ruling as of the searches ([Frankfurt Kurnit](https://technologylaw.fkks.com/post/102jzzg/amazon-faces-lawsuit-under-washingtons-my-health-my-data-privacy-law) **[secondary]**; [Trellis](https://news.trellis.law/federal-case-tests-washington-states-health-data-privacy-law/) **[secondary]**).
- **Analysis.** A plaintiff could argue that OneX "determines the means" of on-device processing. Whether MHMDA reaches a software designer that never receives data is untested.
- **Mitigation.** Post a "Consumer Health Data Privacy Policy" linked from the homepage, as the Act requires of regulated entities. It should say OpenWorld collects, shares, and sells no consumer health data, and that processing stays on the device. This costs little and blunts a claim that OneX hid anything.

### 3.4 Colorado biometric amendment (HB24-1130)

**Rating: Low-Medium.**

- **The law.** Effective July 1, 2025, it amends the Colorado Privacy Act. Any controller of one or more biometric identifiers must adopt a written policy covering retention, deletion, and incident response, and must give notice and get consent before collection ([Colorado General Assembly, HB24-1130](https://leg.colorado.gov/bills/HB24-1130); [Hunton](https://www.hunton.com/privacy-and-cybersecurity-law-blog/new-colorado-law-imposes-heightened-requirements-for-the-collection-and-processing-of-biometric-data) **[secondary]**).
- **Thresholds.** One source says these duties apply regardless of the CPA's usual size thresholds ([Captain Compliance](https://captaincompliance.com/education/colorados-biometrics-privacy-rules/) **[secondary; verify]**).
- **Enforcement.** The AG and district attorneys; there is no private right of action.
- **Analysis.** The same "controller determines purposes and means" question as MHMDA. The written biometric policy recommended in §3.1 also covers Colorado's policy requirement.

### 3.5 California CCPA/CPRA

**Rating: Low.**

- **Thresholds.** The CCPA applies to a "business" with more than $26,625,000 in annual gross revenue (inflation-adjusted Jan 1, 2025), or that buys, sells, or shares the data of 100,000 or more consumers, or that earns at least half its revenue from selling or sharing personal information ([Jackson Lewis CCPA FAQ](https://www.jacksonlewis.com/insights/navigating-california-consumer-privacy-act-30-essential-faqs-covered-businesses-including-clarifying-regulations-effective-1126) **[secondary]**).
- **Sensitive data.** Biometric data processed to identify a consumer is "sensitive personal information" under the CPRA.
- **Analysis.** OneX almost certainly meets none of the thresholds and collects nothing. Low. Re-check if OneX's revenue grows or if any data collection is ever added.

### 3.6 New York City biometric ordinance

**Rating: Low.**

NYC Admin. Code §22-1201 et seq. (effective July 9, 2021) requires signs at "commercial establishments" (retail, food and drink, entertainment) that collect customers' biometric data, and bans selling it. It has a private right of action ([Morrison Foerster](https://www.mofo.com/resources/insights/210623-biometric-identifier-information-law) **[secondary]**; [Local Law 3 of 2021](https://intro.nyc/local-laws/2021-3)). A 2024 bill, Int. 217, would widen it to all "places of public accommodation"; its status was not confirmed ([Int. 217](https://intro.nyc/0217-2024)).

It does not apply to a person scanning their own media. A NYC store running OpenWorld on customer CCTV footage could trigger it. **Mitigation:** the Terms of Use should prohibit business use on customer or premises footage.

### 3.7 Portland (OR) and other local bans

**Rating: Low for OneX; relevant to acceptable use.**

Portland's ordinance, effective Jan 1, 2021, bars **private entities** from using face recognition in **places of public accommodation** in the city. It exempts a user's own personal devices. It has a private right of action for damages or $1,000 per day ([Davis Wright Tremaine, Jan 2021](https://www.dwt.com/blogs/privacy--security-law-blog/2021/01/portland-facial-recognition-ban) **[secondary]**; [Hunton](https://hunton.com/privacy-and-information-security-law/portland-oregon-becomes-first-jurisdiction-in-u-s-to-ban-the-commercial-use-of-facial-recognition-technology) **[secondary]**). No repeal was found, but its status in 2026 was not confirmed. Most other US municipal bans restrict **government** use only ([IAPP](https://iapp.org/news/a/local-facial-recognition-bans-begin-to-take-hold) **[secondary]**).

Because OpenWorld is import-only with no camera, it is not the live in-store system these ordinances target. A Portland business scanning its premises footage could violate the ordinance. **Mitigation:** the Terms of Use ban on business premises use (§3.6) covers this.

### 3.8 Impersonating law enforcement, vigilantism, and false reports

**Rating: Low for OneX directly; this is the main harm pathway for users.**

- **No impersonation.** OpenWorld does not impersonate police. The risk is users treating a candidate as authority ("the FBI app says you're wanted"), or acting on it. State laws on impersonating officers and on harassment apply to users.
- **Citizen's arrest.** Some states have narrowed citizen's-arrest powers. Georgia repealed most of its 1863 law in 2021 after the killing of Ahmaud Arbery ([CBS News, 2021](https://www.cbsnews.com/amp/news/georgia-house-repeal-citizen-arrest-law-ahmaud-arbery-death); [Yahoo/HuffPost, May 2021](https://news.yahoo.com/georgia-citizens-arrest-law-repeal-ahmaud-arbery-191257257.html)).
- **False reports.** Knowingly false statements to federal agents are a crime under 18 U.S.C. §1001 ([Cornell LII](https://www.law.cornell.edu/uscode/text/18/1001)). Copy should never push a user to "report" based on a candidate alone.
- **A precedent from another app.** In May 2021 the Citizen app offered a bounty for a suspected arsonist who turned out to be the wrong man. Apple's June 2021 guideline 1.7 followed ([Gummicube, June 2021](https://www.gummicube.com/blog/app-store-review-guidelines-june-2021-updates) **[secondary]**). This is the closest real-world failure mode.

**What already helps:** "Do not approach", no report, no sharing card, no "found" wording, and amber rather than red. **Gaps:** see the follow/detain wording and the "do not post or share" line in [§10](#10-copy-review-what-works-and-what-to-change).

### 3.9 State age and app-store laws

**Rating: Low-Medium (compliance, not liability).**

- **Texas SB 2420** (App Store Accountability Act) requires developers to use app-store age categories and parental-consent signals. It was preliminarily enjoined in December 2025. The Fifth Circuit stayed the injunction pending appeal in June 2026 ([Subscription Insider](https://www.subscriptioninsider.com/article-type/news/texas-app-store-age-verification-law-reaches-jan-1-effective-date-but-federal-injunction-keeps-sb-2420-on-ice) **[secondary]**; [ZwillGen](https://alerts.zwillgen.com/alerts/injunction-temporarily-lifted-for-texas-app-store-accountability-law) **[secondary]**).
- **California AB 1043** (Digital Age Assurance Act, signed Oct 13, 2025, effective Jan 1, 2027) requires developers to read an operating-system age signal ([Troutman](https://troutmanprivacy.com/2025/10/analyzing-californias-digital-age-assurance-act/) **[secondary]**).
- Similar laws exist in Utah and Louisiana; they were not separately verified here.

**Mitigation:** target adults. Use the store's highest fitting age rating, and read store age signals where the law requires it. Have counsel confirm SB 2420's status at launch.

**COPPA.** COPPA applies to operators that collect personal information from children under 13 ([FTC COPPA FAQ](https://www.ftc.gov/business-guidance/resources/complying-coppa-frequently-asked-questions)). OpenWorld collects nothing, so COPPA should not apply. An adults-only rating still reduces risk.

---

## 4. Civil liability for false candidates

**Rating: Medium-High. This is the scenario that most needs a litigator's view.**

### 4.1 Defamation and false light

**The theory.** OpenWorld's software tells the user that a specific, recognizable person in the user's file is a "possible candidate" for a named FBI wanted poster. The user is a third party to that person, so the statement is "published". If the user then reports, posts, or confronts, the harm compounds.

**What helps OneX.**

- The wording is a possibility, not an assertion: "Possible candidate. Not an identification." The screen also shows the evidence, disclaims identification, and ties in no score.
- In *Walters v. OpenAI* (Ga. Super. Ct., Gwinnett County, May 2025), the court granted summary judgment to OpenAI. It held that a reasonable reader in the user's position, who had seen warnings that the output could be wrong and did not believe it, would not take the output as fact ([Cleary Gottlieb, May 27, 2025](https://www.clearygottlieb.com/news-and-insights/publication-listing/georgia-court-dismisses-defamation-lawsuit-against-openai-over-chatgpt-output) **[secondary]**; [Loeb & Loeb, May 2025](https://www.loeb.com/en/insights/publications/2025/05/walters-v-openai-llc) **[secondary]**). This is one state trial court ruling and binds no one else.

**What hurts.**

- An implication of criminality can be defamatory even when hedged.
- After the 2013 Boston Marathon bombing, two men pictured on a New York Post cover ("Bag Men") sued for libel. The paper pointed to small print saying there was no evidence linking them to the crime. The case settled ([ABA Journal, 2013](https://www.abajournal.com/news/article/two_men_wrongly_identified_as_possible_suspects_in_boston_marathon_bombing) **[secondary]**; [Mediaite, settlement](https://www.mediaite.com/print/ny-post-settles-defamation-suit-over-bag-men-cover) **[secondary]**).
- A face side by side with a wanted poster is a strong visual implication.
- For a **missing** poster tied to a parental kidnapping, a false candidate can imply that a parent in the user's photo abducted a child.

**False light** (in states that recognize it) and **intentional or negligent infliction of emotional distress** follow the same facts.

### 4.2 Negligence and product liability

- **AI apps as products.** Courts are increasingly willing to treat AI apps as "products" for design-defect and failure-to-warn claims. In *Garcia v. Character Technologies* (M.D. Fla. May 2025), the court let product-liability and failure-to-warn claims against an AI chatbot app proceed. It rejected, at that stage, arguments that the app was not a product and that the First Amendment barred the claims. The case reportedly settled in 2026 ([Tech Justice Law Project, case page](https://techjusticelaw.org/cases/garcia-v-character-technologies-google-and-character-ai-co-founders-daniel-de-frietas-and-noam-shazeer/) **[secondary]**; [Tech Policy Press](https://techpolicy.press/from-dolls-to-downloads-courts-reimagine-product-liability-for-the-digital-age) **[secondary]**). The law is unsettled and varies by state.
- **Negligence.** A plaintiff would argue that foreseeable misuse, such as confrontation, required stronger safeguards. OneX's best evidence is the record it already keeps: a written threat model, a strict and measured cutoff, no near-miss list, no sharing card, do-not-approach copy, and a documented choice to accept more misses to avoid false matches.

### 4.3 State consumer-protection claims

State "little FTC Acts" (for example, California's UCL and the Illinois Consumer Fraud Act) mirror §5. The accuracy and privacy wording risks in [§2.1](#21-ftc-act-5-deceptive-and-unfair-practices) apply here too, and private plaintiffs can bring them.

### 4.4 The base-rate problem in numbers

The [decisions](decisions.md) give the chance line as f·N·M. With f about 8e-7, about 1,000 enabled posters (the API returned 1,253 imaged posters on 28 Sep 2026, less the skipped tags), and M faces compared:

| File | Faces compared (M) | Expected false candidates from chance alone |
| --- | --- | --- |
| Family photo | 5 | ≈ 0.004 |
| 100-photo album | 300 | ≈ 0.24 |
| 10-min crowd video | 2,000 | ≈ 1.6 |
| Hour of event footage | 10,000 | ≈ 8 |

These figures use the LFW rate. Real-world false-match rates differ across demographic groups, often by large factors, in NIST's testing of many algorithms ([NIST IR 8280, Dec 2019](https://nvlpubs.nist.gov/nistpubs/ir/2019/NIST.IR.8280.pdf)). Poster photos are often old, small, or scanned. **The legal exposure grows with long crowd videos.**

### 4.5 Mitigations for civil liability

1. **Terms of Use** (shown once at first launch, with a short summary):
   - acceptable use: lawful use only; not for business, premises, or employment screening; no harassment;
   - no posting or sharing of results;
   - do not act on a candidate;
   - warranty disclaimer and limitation of liability, to the extent law allows.

   Apache-2.0 disclaims warranties for the source code. Store binaries need their own terms. Apple's standard EULA is a floor, not a substitute.
2. **Copy fixes** in [§10](#10-copy-review-what-works-and-what-to-change).
3. **A per-scan guard (owner's call; changes a locked decision).** When f·N·M is at least about 0.5, show the chance line in the headline card rather than in Details, and add "Most candidates in a scan this large are expected to be strangers."
4. **Demographic and poster-like evaluation before real posters,** already a release gate.
5. **Insurance:** technology errors-and-omissions plus media liability.
6. **A takedown and contact channel** for anyone who believes the app mislabeled them, even though OneX holds no data. It shows good faith and gives the press a route.
7. **Keep evidence of design choices:** dated decision records (the repo already does this).

---

## 5. EU and UK

### 5.1 GDPR

**Rating: High for EU distribution.**

- **Special-category data.** "Biometric data" processed "for the purpose of uniquely identifying a natural person" is special-category data under Art. 9(1). It may be processed only under an Art. 9(2) exception: explicit consent, or a law of the Union or a Member State for substantial public interest, among others ([GDPR, EUR-Lex](https://eur-lex.europa.eu/eli/reg/2016/679/oj)).
  - **Bystanders have given no consent.** No EU or national law authorizes private individuals to match strangers against wanted lists.
  - Matching against a **wanted** list also processes data "relating to criminal convictions and offences". Art. 10 allows that only under official authority, or where Union or Member State law authorizes it.
- **The household exemption** (Art. 2(2)(c)). GDPR does not apply to processing "by a natural person in the course of a purely personal or household activity".
  - The CJEU reads the exemption narrowly. In *Ryneš* (C-212/13, Dec 11, 2014), a home camera that partly covered a public street was not purely household ([CJEU, C-212/13](https://curia.europa.eu/juris/liste.jsf?num=C-212/13); [commentary](https://www.thomashelbing.com/en/wissen/dsgvo-hub/rechtsprechung/1.4.20-eugh-rynes) **[secondary]**).
  - Checking strangers in one's media against crime and missing lists has a public, quasi-investigative purpose. It is doubtful that it counts as "purely personal".
  - **Recital 18** says the Regulation still applies to "controllers or processors which provide the means for processing personal data for such personal or household activities" ([GDPR recitals](https://eur-lex.europa.eu/eli/reg/2016/679/oj); [mirror](https://presencis.com/regulations/gdpr/recital-18/) **[secondary]**).
- **Is OneX a controller although it receives nothing?**
  - In *Nacionalinis visuomenės sveikatos centras* (C-683/21, Grand Chamber, Dec 5, 2023), the CJEU held that an entity that took part in determining the purposes and means of processing done through a mobile app may be a controller ([CJEU, C-683/21](https://curia.europa.eu/juris/liste.jsf?num=C-683/21); [Loyens & Loeff](https://www.loyensloeff.com/insights/news--events/news/is-an-app-builder-a-data-controller-and-is-fault-required-for-a-gdpr-fine/) **[secondary]**). Earlier CJEU cases (*Wirtschaftsakademie*, *Jehovan todistajat*) also found joint control without access to the data.
  - OneX decides the purpose (matching against FBI posters) and the essential means (models, cutoff, gallery source). An EU regulator could treat OneX as a controller or joint controller of the users' on-device processing.
- **Territorial reach.** Art. 3(2)(a) covers a non-EU controller "offering goods or services" to people in the EU, "irrespective of whether a payment … is required". Listing the app in EU stores is offering it.
- **Pending changes.** The EU's 2025 "digital omnibus" proposal would add a relative definition of personal data and an Art. 9 exception for AI development. As of early 2026 it was pending, and the EDPB and EDPS opposed key parts ([Inside Privacy, Feb 2026](https://www.insideprivacy.com/eu-data-protection/eu-regulators-issue-opinion-on-revisions-of-gdpr-and-other-data-laws/) **[secondary]**; [Kennedys, 2026](https://www.kennedyslaw.com/en/thought-leadership/article/2026/the-2025-european-commission-eu-digital-omnibus-package-the-gdpr-regulation-eu-2016679) **[secondary]**). Nothing found suggests it would make this use lawful.

### 5.2 EU AI Act

**Rating: High for EU distribution. Prohibitions are probably not triggered; the high-risk classification probably is.**

- **Definitions.**
  - Art. 3(41) defines a "remote biometric identification system" as one "for the purpose of identifying natural persons, without their active involvement, typically at a distance through the comparison of a person's biometric data with the biometric data contained in a reference database".
  - Art. 3(43) defines a "post-remote" system as any remote biometric identification system that is not real-time.
  - Biometric **verification** (confirming someone is who they claim to be) is excluded ([AI Act Art. 3, Commission AI Act Service Desk](https://ai-act-service-desk.ec.europa.eu/en/ai-act/article-3); [Recital 17](https://ai-act-service-desk.ec.europa.eu/en/ai-act/recital-17)).

  OpenWorld identifies people in recorded media, who are not involved, by comparing them with a reference database of posters. That fits "post-remote biometric identification" closely. A narrower argument exists, that a person's own photo of nearby people is not "at a distance". The definition says "typically", so it is weak.
- **Art. 5 prohibitions.**
  - **Untargeted scraping, Art. 5(1)(e).** This bans building or expanding face-recognition databases "through the untargeted scraping of facial images from the internet or CCTV footage". The Commission's Feb 2025 guidelines say scraping that targets specific individuals or a pre-defined group is not prohibited ([Orrick, Apr 2025](https://www.orrick.com/en/insights/2025/04/eu-commission-publishes-guidelines-on-the-prohibited-ai-practices-under-the-ai-act) **[secondary]**; [Fieldfisher](https://www.fieldfisher.com/en/insights/key-takeaways-from-the-european-commission-s-guide) **[secondary]**). OpenWorld fetches named persons from one official API, which is targeted. **Keep it that way: never add crawling, user-added galleries, or CCTV sources.**
  - **Real-time remote biometric identification in public spaces, Art. 5(1)(h).** This prohibition is limited to law-enforcement use ([AI Act Art. 5](https://ai-act-service-desk.ec.europa.eu/en/ai-act/article-5)). OpenWorld is neither real-time nor law enforcement. Not triggered.
- **High-risk classification.** Annex III point 1(a) lists remote biometric identification systems as high-risk, insofar as their use is permitted under Union or national law ([Annex III](https://ai-act-service-desk.ec.europa.eu/en/ai-act/annex-3)). Providers must meet the Chapter III requirements:
  - risk management, data governance, technical documentation, logging, human oversight, accuracy and robustness, and a quality management system;
  - conformity assessment, which for biometrics involves a notified body unless harmonized standards are fully applied ([Art. 43](https://ai-act-service-desk.ec.europa.eu/en/ai-act/article-43));
  - EU database registration, CE marking, post-market monitoring;
  - for a non-EU provider, an authorized representative in the EU.
- **Timeline after the omnibus.** The AI omnibus, Regulation (EU) 2026/1744, was published July 24, 2026 and in force July 27, 2026. It moved Annex III high-risk obligations from Aug 2, 2026 to **Dec 2, 2027**. Prohibitions and Art. 50 transparency were not delayed ([Gibson Dunn](https://www.gibsondunn.com/eu-ai-act-omnibus-agreement-postponed-high-risk-deadlines-and-other-key-changes/) **[secondary]**; [White & Case](https://www.whitecase.com/insight-alert/eu-ai-omnibus-enters-force-amending-ai-act) **[secondary]**; [Cooley](https://cdp.cooley.com/digital-ai-omnibus-delays-key-deadlines-introduces-new-rules/) **[secondary]**). Confirm against the Official Journal.
- **The open-source exemption does not help.** Art. 2(12) exempts AI systems released under free and open-source licenses *unless* they are placed on the market or put into service as high-risk systems, or fall under Art. 5 or Art. 50 ([Linux Foundation Europe explainer](https://linuxfoundation.eu/newsroom/ai-act-explainer) **[secondary]**; [Orrick, May 2024](https://orrick.com/Insights/2024/05/The-EU-AI-Act-Open-Source-Exceptions-and-Considerations-for-Your-AI-Strategy) **[secondary]**). "Placing on the market" covers supply "whether for payment or free of charge" (Art. 3).
- **The personal-use exemption covers users, not OneX.** Art. 2(10) exempts deployers who are natural persons using AI "in the course of a purely personal non-professional activity" ([Art. 2](https://ai-act-service-desk.ec.europa.eu/en/ai-act/article-2)). It does not exempt the provider.

**Conclusion.** Distributing OpenWorld in the EU would make OneX the provider of a probable high-risk system. That requires a notified-body conformity assessment and an EU representative by Dec 2, 2027. GDPR problems apply today. **Do not distribute in the EU/EEA at launch.**

### 5.3 United Kingdom

**Rating: High.**

- **Law and guidance.** UK GDPR and the Data Protection Act 2018 treat biometric data used for identification as special-category data. The ICO's biometric recognition guidance (2024) remains in force but is under review after the Data (Use and Access) Act 2025, which received Royal Assent on June 19, 2025 ([Stephenson Harwood, Aug 2025](https://www.stephensonharwood.com/insights/data-protection-update-august-2025/) **[secondary]**; [BABL AI on ICO guidance](https://babl.ai/ico-unveils-guidance-on-lawful-biometric-recognition/) **[secondary]**).
- **Reach over foreign companies.** On Oct 7, 2025, the Upper Tribunal held that the ICO had jurisdiction over Clearview AI, a US company with no UK establishment. It read "behavioural monitoring" broadly under Art. 3(2)(b) and sent the case back to the First-tier Tribunal ([A&O Shearman](https://www.aoshearman.com/en/insights/ao-shearman-on-data/upper-tribunal-finds-in-favour-of-ico-in-appeal-against-first-tier-tribunal-decision-on-clearview-ai) **[secondary]**; [11KBW](https://www.11kbw.com/knowledge-events/case/upper-tribunal-hands-down-clearview-v-ico-judgment-on-territorial-and-material-scope-of-gdpr/) **[secondary]**). OpenWorld's facts are much milder (no scraping, no server), but the ICO is willing to reach abroad.
- **No AI Act**, but the GDPR analysis in §5.1 applies almost unchanged.

**Do not distribute in the UK at launch.**

---

## 6. Canada, Australia, and elsewhere

### 6.1 Canada

**Rating: Medium-High.**

- **PIPEDA.** It covers personal information collected in the course of "commercial activity". In 2021, the federal commissioner and the Quebec, BC, and Alberta commissioners jointly found that Clearview AI violated PIPEDA and provincial laws by collecting biometric data without consent and for inappropriate purposes ([OPC, PIPEDA Findings #2021-001](https://priv.gc.ca/en/opc-actions-and-decisions/investigations/investigations-into-businesses/2021/pipeda-2021-001)).
- **Quebec.** Quebec requires express consent for biometric verification. Creating a biometric database must be disclosed to the Commission d'accès à l'information in advance ([Miller Thomson](https://www.millerthomson.com/en/insights/cybersecurity/privacy-commissioners-take-stance-against-collection-of-biometric-data/) **[secondary]**).

Whether a free app from a US company is "commercial activity" is arguable. **Exclude Canada at launch**, at minimum Quebec, which in practice means all of Canada in app stores.

### 6.2 Australia

**Rating: Medium.**

- **Privacy Act.** Biometric templates are "sensitive information". Businesses with turnover of A$3 million or less are generally exempt from the Australian Privacy Principles, but a statutory tort for serious invasions of privacy took effect June 10, 2025 with no small-business exemption ([College of Law](https://www.collaw.edu.au/community/news/australias-new-tort-of-privacy-is-in-force-what-you-need-to-know/) **[secondary]**; [Lawyers Weekly, Mar 2026](https://www.lawyersweekly.com.au/sme-law/43941-australia-s-new-privacy-tort-puts-workplace-ai-and-surveillance-on-trial) **[secondary]**).
- **Bunnings.** On Feb 4, 2026, the Administrative Review Tribunal partly overturned the OAIC's determination against Bunnings' in-store face recognition. It kept the transparency findings and reversed the consent finding ([Clayton Utz, Feb 2026](https://www.claytonutz.com/insights/2026/february/bunnings-wins-appeal-on-facial-recognition-technology-key-takeaways-for-businesses-and-privacy-law) **[secondary]**).

**Exclude Australia at launch** until local counsel reviews it.

### 6.3 Everywhere else

Many countries have GDPR-style laws (for example Brazil's LGPD and South Korea's PIPA) that treat biometric identification as sensitive. Some also restrict reusing images of wanted persons. INTERPOL's terms forbid reusing notice pictures ([record-classes.md](record-classes.md), citing [INTERPOL terms](https://www.interpol.int/en/Who-we-are/Terms-of-use)). The FBI poster set is US-centric anyway.

---

## 7. Geo-restriction recommendation

**Recommendation: launch in app stores in the United States only.**

| Region | Store availability at launch | Why |
| --- | --- | --- |
| United States | **Yes** | The data source is US; the main risks are manageable with the steps in this document. |
| EU/EEA, Switzerland | **No** | GDPR Art. 9 and 10 now; AI Act high-risk duties from Dec 2, 2027. |
| United Kingdom | **No** | UK GDPR; ICO reach over foreign companies. |
| Canada | **No** | PIPEDA and *Clearview* findings; Quebec biometric rules. |
| Australia | **No** | Statutory privacy tort; review pending. |
| Rest of world | **No** (initially) | No local review; posters are US-focused. |

**What geo-restriction cannot do.** Store territory settings do not control GitHub release downloads or the public source code.

- Publishing source code is far lower risk than offering a ready-to-run app. The EU AI Act's provider duties attach to placing a system on the market or putting it into service.
- For GitHub binary releases: state on the release page and in the app's About screen that OpenWorld is "intended for use in the United States and not offered in the EU, EEA, UK, Switzerland, Canada, or Australia". Do not localize the app into EU languages yet. Do not market abroad.
- Counsel should say whether that is enough (question 2 in [§12](#12-questions-for-a-licensed-attorney)).
- Within the US, no state needs to be blocked on current law. Revisit Illinois if *Hazlitt* or a state court rejects the *Samsung* reasoning for apps with no cloud component.

---

## 8. App stores

### 8.1 Apple App Store

**Rating: High risk of rejection. Plan for a long review.**

| Guideline | What it says (paraphrased; check the [current guidelines](https://developer.apple.com/app-store/review/guidelines/)) | Why it matters to OpenWorld |
| --- | --- | --- |
| **5.1.1(viii)** | Apps that compile personal information from any source not directly from the user, or without the user's explicit consent, "even public databases", are not permitted. Added in 2019, and used to reject a ballot-lookup app in 2020 ([mjtsai, June 2019](https://mjtsai.com/blog/2019/06/04/updates-to-the-app-store-review-guidelines/) **[secondary]**; [9to5Mac, Oct 2020](https://9to5mac.com/2020/10/30/an-iphone-app-designed-to-verify-pennsylvania-mail-in-ballots-gets-rejected-by-apple) **[secondary]**). | The poster pack is personal information about wanted and missing people, compiled from a public government database. A reviewer could apply this rule. **Position:** the app displays official public safety notices from their publisher, does not build profiles, does not let users search for a person, stores nothing beyond a 7-day cache, and links back to the source. |
| **1.7 Reporting criminal activity** | Apps for reporting alleged criminal activity must involve local law enforcement and may be offered only where that involvement is active. Added June 2021 after the Citizen bounty incident ([Gummicube, June 2021](https://www.gummicube.com/blog/app-store-review-guidelines-june-2021-updates) **[secondary]**; [Apple developer forum, guideline 1.7 rejection](https://developer.apple.com/forums/thread/736371)). | OpenWorld does not send reports, but the leaving sheet shows FBI tip lines. A reviewer may ask for proof of a relationship with law enforcement, which OneX does not have. **Position:** the app is not a reporting app. It opens an official FBI web page, and the user decides what to do. Be ready for this to fail. |
| **5.1.1(ix)** | Apps in highly regulated fields, or that require sensitive user information, should be submitted by a legal entity. Rejections have required the seller name to match the organization ([Apple developer forums](https://developer.apple.com/forums/thread/750657)). | Submit under the **OneX Engineering** organization account, not an individual account. |
| **5.1.1(i), 5.1.2** | Privacy policy link required in the app and in App Store Connect; data use limits. | A privacy policy is required even though nothing is collected. |
| **2.3, 2.3.7** | Accurate metadata; no other brands' names in keywords. | No "FBI" in keywords or subtitle. |
| **2.5.13** | Facial recognition for account sign-in must use LocalAuthentication. | Not applicable (no sign-in). Say so in review notes if asked. |
| **Law-enforcement-adjacent apps** | In Oct 2025 Apple removed ICEBlock, citing safety information from law enforcement ([Al Jazeera, Oct 3, 2025](https://www.aljazeera.com/news/2025/10/3/apple-pulls-iceblock-from-app-store-following-us-government-pressure)). | Apple acts quickly on apps it sees as affecting police and public safety. Expect scrutiny in both directions. |
| **Oct 16, 2026 update** | Added rules on AI intimate imagery and a new 2.5.19 on hidden features; no reported change to the privacy rules ([Apple Developer news](https://developer.apple.com/news/?id=92th7e5i)). | Make sure "demo" versus "real posters" modes are disclosed in review notes (2.5.19: no hidden or dormant features). |

### 8.2 Google Play

**Rating: Medium.**

- **Misleading claims.** Google's misleading-claims and government-information rules bar apps that falsely suggest a tie to a government body or provide government services without authorization. They also bar "Official" in a title without the rights to use it ([Google Play: requirements for apps that communicate government information](https://support.google.com/googleplay/android-developer/answer/9514050?hl=en)). Google also shows a badge on verified government apps ([TechCrunch, May 2024](https://techcrunch.com/2024/05/02/google-will-now-show-labels-in-play-store-to-denote-official-government-apps)). The non-affiliation line and naming rules address this.
- **Data safety form.** Data that is only processed on the device and never sent off it does not need to be disclosed as "collected" ([Google Play Help, Data safety](https://support.google.com/googleplay/answer/11416267?hl=en)). OpenWorld can truthfully declare "No data collected" and "No data shared". When Update posters ships and the manifest gains INTERNET, re-check whether the poster download (which reveals only the device's IP address to the FBI) must be declared.
- **Privacy policy.** Required for the listing.

### 8.3 Other stores

- **Microsoft Store and Flathub:** apply the same naming rules. Flathub requires accurate license metadata, so fix the FFmpeg licensing first ([§9](#9-open-source-licensing-and-model-provenance)).
- **F-Droid:** if used, its build server will see the "Accurate" manifest that names non-free weights. Keep it clearly unshipped.

### 8.4 What the listing must and must not say

**Must say** (description body):

- "OpenWorld is not part of, or endorsed by, the FBI or any government agency."
- "It compares faces in a photo or video you already have with public FBI missing-person and wanted-person posters, on your device. Nothing is uploaded."
- "A possible candidate is not an identification. No candidate is not a clearance. Do not approach anyone."
- "For adults in the United States."
- A link to the privacy policy, the biometric data policy, and the Terms of Use.

**Must not say:**

- "FBI" in the name, subtitle, icon, or keywords; "official"; "approved", "partnered", or "with the FBI".
- "Identify", "find", "catch", "track down", "match found", "solve", "bounty", or "reward".
- "Accurate", "unbiased", "99.x%", or any accuracy number other than a qualified one with its test set named.
- "Help catch fugitives" or "be a hero". Avoid calls to action that suggest pursuit.
- Screenshots of real wanted or missing people, the FBI seal, or fbi.gov page headers. Use demo posters only.

---

## 9. Open-source licensing and model provenance

**Rating: Medium overall (fixable before release).**

| Component | Where | License | Obligations and findings |
| --- | --- | --- | --- |
| **FFmpeg (Linux Flatpak)** | `desktop/packaging/app.openworld.OpenWorld.json` builds tag n7.1.1 from source with `--disable-autodetect`, `--disable-network`, and **no** `--enable-gpl` or `--enable-nonfree` | LGPL-2.1-or-later | **Good choice.** Ship the license text, the exact source, and the configure flags, ideally hosted next to the binary, and credit it in About. This follows FFmpeg's own checklist ([FFmpeg legal page](https://www.ffmpeg.org/legal.html)). The core runs `ffmpeg` as a separate program, which keeps OpenWorld's code clearly separate. |
| **FFmpeg (Windows)** | `desktop/packaging/windows.md` installs `mingw-w64-ucrt-x86_64-ffmpeg` from MSYS2 and `windows-bundle.sh` copies `ffmpeg.exe`, `ffprobe.exe`, and **every DLL they load** | MSYS2 labels its FFmpeg package **GPL-3.0-or-later** ([MSYS2 package page](https://packages.msys2.org/base/mingw-w64-ffmpeg)) | **Fix before release.** Distributing a GPL-3.0 `ffmpeg.exe` means offering complete corresponding source for it and every GPL library bundled (for example x264 and x265, if linked), shipping the license, and adding no further restrictions. Running it as a separate process likely keeps OpenWorld's Apache-2.0 code unaffected, but the bundle itself must comply. **Recommendation:** build a decode-only LGPL FFmpeg for Windows with the same flags as the Flatpak, plus `--disable-encoders` and only the decoders needed. Then inventory every copied DLL with an SBOM. |
| **Codec patents** | Desktop FFmpeg includes H.264, HEVC, and AAC decoders | Not a copyright license issue | FFmpeg's legal page warns that patents are separate from its license. Decoding patented formats in a free desktop app is a low-probability risk, but it is real. Phones use the OS decoders (AVFoundation and MediaCodec), whose licensing the platform covers. That is good. Counsel can say whether to limit desktop decoders or use OS decoders (Media Foundation on Windows). |
| **GTK 4, libadwaita, GLib, Pango, and others (Linux, Windows)** | Linked dynamically; DLLs on Windows | LGPL-2.1-or-later (GTK, libadwaita); others vary | Ship license texts and a source offer or link to the exact versions. Dynamic linking satisfies the relinking requirement. Do not add EULA terms forbidding reverse engineering for debugging LGPL modifications. The copied Adwaita icon theme carries its own license (CC-BY-SA / LGPL); include attribution. |
| **ONNX Runtime** | Fetched by `scripts/fetch_ort.py`, pinned by SHA-256 | MIT, plus `ThirdPartyNotices.txt` for bundled components | Ship the MIT notice and ORT's ThirdPartyNotices in every app. |
| **YuNet 2023mar** | `bundles/fast/manifest.toml` | MIT, Copyright (c) 2020 Shiqi Yu ([OpenCV Zoo](https://huggingface.co/opencv/opencv_zoo/blob/d33294bfaec5f2386dab1783e1a4031330645f57/models/face_detection_yunet/README.md)) | Include the MIT notice. **Training data:** WIDER FACE, per the upstream training repo ([mirror model card](https://huggingface.co/pollen-robotics/face_detection_yunet_2023mar) **[secondary]**). WIDER FACE is commonly distributed for non-commercial research, and [handoff.md](handoff.md) lists it as CC BY-NC-ND. Whether dataset terms reach trained weights is unsettled. Document it. |
| **SFace 2021dec** | `bundles/fast/manifest.toml` | Apache-2.0 ([OpenCV Zoo directory](https://github.com/opencv/opencv_zoo/blob/main/models/face_recognition_sface/README.md)) | Include the Apache-2.0 license and any NOTICE. **Training data:** the SFace paper reports models trained on CASIA-WebFace, VGGFace2, and MS-Celeb-1M ([arXiv 2205.12010](https://arxiv.org/pdf/2205.12010)). Which one produced the OpenCV Zoo weights was not confirmed. MS-Celeb-1M was withdrawn by Microsoft in 2019, and its license status is disputed ([Microsoft Research page](https://www.microsoft.com/en-us/research/?p=270507); [Google Dataset Search listing](https://datasetsearch.research.google.com/search?query=ms1m)). **Ask OpenCV or the authors which dataset was used, and record the answer.** An Apache-2.0 license on weights does not guarantee rights in the training data. |
| **Accurate bundle (SCRFD-10GF + ArcFace-R100@Glint360K)** | `bundles/accurate/manifest.toml` already says "non-commercial research only. Not bundled." | InsightFace weights: non-commercial research only ([decisions §2](decisions.md)) | **Do not ship these weights.** Glint360K was cleaned from MS-Celeb-1M and Celeb-500k ([InsightFace challenge report](https://arxiv.org/pdf/2108.08191)). WebFace260M is research-only. Any future "Accurate" embedder needs weights whose **training data** allows commercial redistribution, documented in the manifest. Keep the "not measured yet" slot honest. |
| **Evaluation datasets (LFW, CALFW, CPLFW, WIDER FACE)** | `eval/`, downloaded and not committed | Research terms; WIDER FACE CC BY-NC-ND | Use is limited to evaluation, and nothing is redistributed. Publishing aggregate metrics is generally fine. OneX is a company, so have counsel confirm that "non-commercial" evaluation terms allow a free product's measurement. Low. |
| **Rust crates** | `core/`, `desktop/` | Mostly MIT or Apache-2.0 (cargo-deny is configured) | MIT requires the notice in all copies, including binaries. Generate a notices file (for example with `cargo about`) and show it in-app. |
| **OpenWorld itself** | `LICENSE` | Apache-2.0 | Add a `NOTICE` file. Apache-2.0 §4(d) requires downstream redistributors to carry it. |

**Finding:** the repo has no third-party notices file and no in-app Licenses screen. Searching the repo found only `LICENSE` files. **Must fix before release** on all five platforms:

- Android: an OSS licenses screen.
- iOS and Mac: an Acknowledgements screen in Settings or About.
- Desktop: an About dialog plus a `licenses/` folder.

---

## 10. Copy review: what works and what to change

### 10.1 What already handles risk well

The docs and copy already do a lot that a careful lawyer would ask for:

| Design or copy | Risk it reduces |
| --- | --- |
| "Possible candidate. Not an identification." (`possible_candidate`) and "A candidate is not an identification." (`not_identification`) | Defamation, FTC deception |
| "No candidate is not a clearance." (`no_clearance`), "The rest are not cleared.", the TOO_BRIEF wording | Negligence from users relying on a clean result |
| No raw score, no near-miss list, no slider, no sharing card, no "found" or "identified" ([decisions §7](decisions.md), [handoff](handoff.md)) | Defamation, threshold shopping, viral spread of false matches |
| Amber, not red; no sirens; "calm, not alarming" ([ux.md](ux.md)) | Escalation and confrontation |
| Leaving sheet: "OpenWorld is not sending a report.", "Do not approach.", class-specific numbers, "This file is not authenticated." | Vigilantism, false reports |
| Missing class: "It is not a criminal accusation." | Defamation of missing people and of the people with them |
| Only the poster's own fbi.gov URL opens; no prefill or POST to tips.fbi.gov | Impersonation, auto-report harms |
| Skips ECAP, unidentified remains, and unnamed "Seeking Information" ([record-classes.md](record-classes.md)) | Matching against unknown suspects (highest false-accusation risk); AI Act "untargeted" concerns |
| Import only, no camera, no background scan | Real-time remote biometric identification bans; Portland and NYC-type premises use |
| No upload, telemetry, accounts, or cloud backup; templates never persisted | BIPA "possession" and "control" (*Samsung*), FTC, GDPR transfer risk |
| Real posters off until a curve exists; legal review is a release gate ([release-gates.md](release-gates.md)) | Everything; shows deliberate care |
| Chance line on every result ([decisions §1](decisions.md)) | FTC substantiation; honest base-rate disclosure (needs a qualifier, below) |
| File warnings: old file, timestamps disagree, not authenticated | Users relying on stale or fake media |

### 10.2 Copy that should change

Quoted from `core/openworld-core/src/copy.rs` unless noted. Each change keeps the existing code-based design: add new codes rather than changing meanings that shells read.

1. **Add a non-affiliation line (must).** New code, for example `not_affiliated`:
   > "OpenWorld is not part of, or endorsed by, the FBI or any government agency."

   Show it on the Start screen footer, in the disclosure list (`DISCLOSURE`), on the leaving sheet (`leave_notes`), in About, and in the store listing. *Why:* 18 U.S.C. §709; Google's misleading-claims policy.

2. **Fix "does not contact an agency" (must).** Current: `NO_AGENCY` = "OpenWorld does not contact an agency."
   - Once Update posters fetches from api.fbi.gov and Open FBI page visits fbi.gov, this is not literally true.
   - The same issue appears in the [trust.md](trust.md) disclosure ("…contact an agency, or make a report").
   - Suggested:
     > "OpenWorld does not report anything to an agency."
   - Add a network line, shown when real posters are on:
     > "Updating posters downloads them from api.fbi.gov. Opening a poster visits fbi.gov. Those sites see your device's internet address, as any website does."

   *Why:* FTC deception (literal truth of express claims).

3. **Align the missing-person leaving line with the spec (must).** Current: `LEAVE_MISSING` = "This opens an FBI missing-person poster. It is not a criminal accusation. Do not approach. Missing-child posters typically list NCMEC at 1-800-THE-LOST (1-800-843-5678)."
   - [record-classes.md](record-classes.md) requires "**do not approach, follow, or detain — especially a child**", and that "parental kidnapping is often family conflict". The copy has only "Do not approach."
   - Suggested:
     > "This opens an FBI missing-person poster. It is not a criminal accusation. Do not approach, follow, or try to hold anyone, especially a child. Missing-child posters typically list NCMEC at 1-800-THE-LOST (1-800-843-5678)."

   *Why:* negligence and foreseeable confrontation.

4. **Strengthen the wanted leaving line (should).** Current: `LEAVE_WANTED` = "This opens an FBI wanted poster. Do not approach. Wanted posters typically list 1-800-CALL-FBI (1-800-225-5324) or tips.fbi.gov."
   - Suggested:
     > "This opens an FBI wanted poster. Do not approach, follow, or confront anyone. If someone is in immediate danger, call 911. Wanted posters typically list 1-800-CALL-FBI (1-800-225-5324) or tips.fbi.gov."

   *Why:* vigilantism; a clear emergency path.

5. **Add a "do not share" line (should).** New code, shown on candidate cards and the leaving sheet:
   > "Do not post or share this result. A possible candidate can be a stranger who looks alike."

   *Why:* defamation and false light flow mainly from republication by users.

6. **Qualify the LFW claims (must, before real posters).**
   - Current chance line (`chance_line`): "With {faces} faces compared to {posters} posters, chance alone gives about X false candidates (measured on {source})."
   - Current bundle row ([ux.md](ux.md)): "Measured on LFW".
   - Suggested chance-line tail:
     > "…(measured on {source}: photos of celebrities, not posters. The real rate on your file can be higher, and can differ between groups of people)."
   - Suggested bundle row:
     > "Measured on celebrity photos (LFW), not on posters."

   *Why:* FTC substantiation. LFW does not represent posters, as the README itself says.

7. **Pair the strongest band with a reminder (should).** Current: `BAND_WELL` = "Far past the locked cutoff."
   - Users may read this as "certain".
   - Keep it, but always render it with `not_identification`, and add:
     > "Look-alikes, relatives, and old photos can pass the cutoff."

   *Why:* deception by implication; defamation.

8. **Add a lawful-use line before Analyze (should).** New code in the disclosure:
   > "Only scan files you have the right to use. Some places limit scanning other people's faces."

   *Why:* BIPA and GDPR exposure for users; good-faith notice.

9. **Add a demographic-unknown line until measured (must, before real posters).** New code:
   > "How often this misidentifies people of different ages, sexes, and skin tones is not measured yet."

   *Why:* FTC biometric policy statement (known risks); honesty about an open item in [decisions.md](decisions.md).

10. **README tagline (should).** "OneX Engineering will successfully privatize open source world peace." It is not unlawful, and it would likely be read as puffery. But in a product touching law enforcement and missing children it reads as unserious, and opposing counsel could quote it. Consider removing it before public release.

11. **Start-screen line (no change needed).** "Check a photo or video you already have against public FBI missing and wanted posters, on this device." It is accurate and descriptive. Add the non-affiliation line under it.

---

## 11. Product-design mitigations

These reduce legal risk without changing the locked core. Items marked *(owner's call)* would change a locked decision in [decisions.md](decisions.md) and are suggestions only.

| Area | Mitigation | Legal risk addressed |
| --- | --- | --- |
| **Wording** | The [§10](#10-copy-review-what-works-and-what-to-change) changes. | FTC, §709, defamation, negligence |
| **First-run screen** | One screen at first launch: what the app is and is not, not affiliated with the FBI, do not approach or share, adults only, links to the Terms and privacy policy, and one "Continue" tap. It records acceptance locally only. It is not a per-scan checkbox, consistent with [trust.md](trust.md). | Contract formation for the Terms; notice |
| **Age gating** | Use the highest fitting store age rating. Read OS or store age signals where the law requires (Texas SB 2420, California AB 1043). No self-declared age field is needed. | State age laws; harm to minors |
| **Geofencing** | Store territories US-only. A notice on GitHub releases. No in-app location check, which would mean collecting location. | GDPR, AI Act, PIPEDA |
| **No-approach messaging** | Already strong. Add "follow or try to hold", the 911 line, and "do not share". | Negligence, vigilantism |
| **False-candidate rate** | Keep the 1e-6 cutoff. Finish the demographic and poster-like evaluation before real posters. *(Owner's call)* Move the chance line into the headline when f·N·M ≥ 0.5, and consider requiring more gated frames for a video face in very large scans. | Defamation, FTC unfairness |
| **Record retention** | 7 days is reasonable and disclosed. Consider deleting candidate **evidence frames**, which show bystanders, sooner (for example 24 hours), and keeping only the crop pair. Encrypt result folders on desktop, as mobile platform file protection already does. Never persist templates. | BIPA/Colorado retention duties if they apply; data minimization under GDPR-style laws |
| **What to log** | Nothing media-derived, ever. No crash reporter, or one that is opt-in and strips media, paths, and results. No analytics. | BIPA "control" (*Samsung* versus *Hazlitt*); FTC; MHMDA |
| **Cloud and backup** | Keep `allowBackup=false` and the iOS backup exclusion. On macOS and Windows, store results outside synced folders (`%LOCALAPPDATA%` is not roamed; check macOS `Application Support` against iCloud Desktop & Documents sync). | *Hazlitt* cloud-sync theory |
| **Gallery scope** | Keep it limited to named FBI posters. Never allow user-added photos, crawling, or other sources without a new legal review. | AI Act Art. 5(1)(e); 5.1.1(viii); watchlist misuse |
| **Takedown and contact** | A public email and web form for "I think OpenWorld mislabeled me" and for poster-image rights holders. OneX holds no user data, so the answer is mostly explanation, but the channel matters. | Defamation mitigation, copyright notice handling |
| **Accessibility** | Keep the WCAG AA contrast, non-color status cues, and screen-reader labels already in [ux.md](ux.md). Warnings must be read aloud as well as shown, so that disclaimers reach every user. | Disclaimers effective for all users; consumer-protection fairness |
| **Insurance and entity** | Publish only under OneX Engineering (not an individual). Get technology E&O and media liability cover. | Personal liability; defense costs |
| **Documentation** | Keep the dated decision records, threat model, and evaluation files. Add a short "harm assessment" page modeled on the FTC policy statement's factors. | FTC unfairness ("assess foreseeable harms") |

---

## 12. Questions for a licensed attorney

In priority order, with the kind of lawyer needed.

1. **False-candidate liability** (US litigation and media counsel). Given the copy in [§10](#10-copy-review-what-works-and-what-to-change), what is OneX's exposure to defamation, false light, IIED, negligence, and product-liability claims if a user acts on a false candidate? What should the Terms of Use say, and is a clickwrap at first run enforceable for a free app? What insurance limits are reasonable?
2. **EU, UK, and international** (EU/UK data-protection and AI Act counsel). Is OneX a GDPR controller of on-device processing it designed but never receives (C-683/21)? Is OpenWorld a high-risk "remote biometric identification system" under Annex III 1(a)? Does a GitHub binary release count as placing on the EU market, and does a US-only notice suffice?
3. **FBI name and seal** (US federal regulatory counsel). Review the final name, icon, listing, and screenshots under 18 U.S.C. §709 and §701. Should OneX ask the FBI Director for written permission, or send a courtesy notice?
4. **State biometric laws** (US privacy counsel). After *G.T. v. Samsung* (7th Cir. 2026) and with *Hazlitt v. Apple* set for trial in 2027: is OneX outside BIPA, CUBI, MHMDA, and Colorado's HB24-1130? Should Illinois users get any specific notice? Should OneX post the Washington consumer-health-data and Colorado biometric policies even if it takes the position that they do not apply?
5. **Poster images and terms** (IP counsel). Is transient on-device matching against third-party and NCMEC-credited photos on FBI posters fair use? Do the FBI API and website terms allow automated retrieval and on-device embedding? Do NCMEC's terms reach NCMEC images republished on fbi.gov?
6. **App-store strategy** (counsel with store experience). How to respond to Apple 1.7 and 5.1.1(viii) objections, and whether to request a pre-submission consultation.
7. **Open-source compliance** (OSS licensing counsel). Confirm the LGPL-only FFmpeg plan, codec patent exposure on desktop, notice requirements, and whether model training-data terms (WIDER FACE, MS-Celeb-1M, CASIA-WebFace) affect redistributing YuNet and SFace weights.
8. **Age laws** (US consumer-protection counsel). The status of Texas SB 2420 at launch, and California AB 1043 duties from Jan 1, 2027.
9. **Export** (export-control counsel, short check). Confirm "published" status, and check whether the 3A981 facial-recognition rule has been finalized.

---

## 13. Launch checklist

### Must before public release (any store or binary)

- [ ] **Store territories set to the US only.** A notice on GitHub releases and in About: not offered in the EU/EEA, UK, Switzerland, Canada, or Australia.
- [ ] **No "FBI" in the app name, subtitle, icon, keywords, or seller name. No seal or badge anywhere.** Listing written to [§8.4](#84-what-the-listing-must-and-must-not-say).
- [ ] **Non-affiliation line** added to copy, shown on Start, disclosure, leaving sheet, About, and the listing.
- [ ] **`NO_AGENCY` reworded,** plus a network disclosure line for Update posters and Open FBI page.
- [ ] **`LEAVE_MISSING` aligned with record-classes.md** ("approach, follow, or try to hold … especially a child").
- [ ] **LFW qualifiers** on the chance line and the bundle row.
- [ ] **Demographic-unknown line** (until a demographic evaluation exists).
- [ ] **Terms of Use** (acceptable use, no business premises use, no sharing, no acting on candidates, disclaimers) and a first-run screen.
- [ ] **Privacy policy** (in-app and store), saying nothing is collected and what fbi.gov sees on fetch or visit.
- [ ] **Biometric data policy** (what is processed, on device, 7-day retention, deletion) and a **Washington consumer health data policy** linked from the website homepage.
- [ ] **Third-party notices:** a NOTICE file plus an in-app Licenses screen on all platforms, covering ONNX Runtime and its ThirdPartyNotices, YuNet (MIT), SFace (Apache-2.0), FFmpeg (LGPL), GTK and libadwaita (LGPL), Adwaita icons, and Rust crates.
- [ ] **Windows FFmpeg rebuilt LGPL-only and decode-only,** with source, configure flags, and license shipped. **SBOM** of every bundled DLL.
- [ ] **No telemetry, cloud sync, or crash reports with media-derived data,** verified by the planned packet-capture test, and **templates never persisted** (test).
- [ ] **Already gated in the repo:** real posters stay off until the curve exists. Before turning them on, the poster-like and **demographic evaluation**, plus counsel's sign-off on poster image rights (skip NCMEC- or third-party-credited images until cleared).
- [ ] **Organization developer accounts** (Apple and Google) under OneX Engineering.
- [ ] **Licensed attorney review** of at least questions 1–5 in [§12](#12-questions-for-a-licensed-attorney).
- [ ] **Re-check time-sensitive items on release day:** Texas SB 2420 status, the BIS 3A981 rule, the FTC biometric policy statement, *Hazlitt v. Apple*, the AI Act omnibus text, and the Apple and Google guideline text.

### Should

- [ ] Strengthen `LEAVE_WANTED` (follow or confront; the 911 line).
- [ ] Add a "do not post or share this result" line.
- [ ] Pair `BAND_WELL` with the look-alike reminder.
- [ ] Add the lawful-use line before Analyze.
- [ ] Remove the README tagline.
- [ ] *(Owner's call)* Move the chance line into the headline when f·N·M ≥ 0.5.
- [ ] Shorten retention of evidence frames (for example to 24 hours); encrypt desktop result folders.
- [ ] A public contact and takedown channel.
- [ ] A courtesy letter to the FBI Office of Public Affairs.
- [ ] Technology E&O and media-liability insurance.
- [ ] Ask OpenCV and the SFace authors which training set produced the shipped weights; record the training data for YuNet and SFace in the manifests.
- [ ] A short harm-assessment page following the FTC biometric policy statement factors.
- [ ] Apple pre-submission guidance on 1.7 and 5.1.1(viii); prepare review notes explaining that it is not a reporting app and how demo and real-poster modes work.
- [ ] Plan a desktop-first launch (GitHub, Flathub, Microsoft Store) in case Apple rejects the app.

---

## 14. Sources

All sources were checked by web search on **2026-10-19**. Dates in brackets are the event or publication date. **[secondary]** means a law-firm, news, or mirror source rather than the primary text.

**US federal**

- 18 U.S.C. §709: [Cornell LII](https://www.law.cornell.edu/uscode/text/18/709) (the text could not be fetched; verify). 1954 amendment record: [UNT](https://mocat.library.unt.edu/catalog/717-16803) [secondary]
- 18 U.S.C. §701: [Cornell LII](https://www.law.cornell.edu/uscode/text/18/701); *Goeltz* discussion: [Wikisource letter, 2010](https://en.wikisource.org/wiki/Page:20100803-wiki-LetterToLarson.pdf/1) [secondary]
- 18 U.S.C. §1001: [Cornell LII](https://www.law.cornell.edu/uscode/text/18/1001)
- FBI website policy (seal and name): [fbi.gov/privacy-policy](https://www.fbi.gov/privacy-policy); FBI Wanted API: [fbi.gov/wanted/api](https://www.fbi.gov/wanted/api)
- FBI Anti-Piracy Warning Seal Program [July 13, 2012]: [Federal Register](https://www.federalregister.gov/documents/2012/07/13/2012-16506/federal-bureau-of-investigation-anti-piracy-warning-seal-program)
- 17 U.S.C. §105: [govinfo](https://www.govinfo.gov/content/pkg/USCODE-2020-title17/html/USCODE-2020-title17-chap1-sec105.htm); [OSU Libraries](https://library.osu.edu/site/copyright/?p=511) [secondary]
- FTC Rite Aid [Dec 19, 2023]: [FTC](https://www.ftc.gov/news-events/news/press-releases/2023/12/rite-aid-banned-using-ai-facial-recognition-after-ftc-says-retailer-deployed-technology-without-reasonable-safeguards); [Fierce Healthcare](https://fiercehealthcare.com/regulatory/rite-aids-reckless-use-ai-facial-recognition-tech-earns-5-year-ban-ftc) [secondary]
- FTC IntelliVision [Dec 2024; final Jan 2025]: [FTC](https://search.ftc.gov/news-events/news/press-releases/2025/01/ftc-finalizes-order-prohibiting-intellivision-making-deceptive-claims-about-its-facial-recognition); [Global Policy Watch](https://www.globalpolicywatch.com/2024/12/intellivision-settles-ftc-allegations-regarding-its-facial-recognition-technology/) [secondary]
- FTC biometric policy statement [May 18, 2023]: [Wiley](https://www.wiley.law/wiley-connect/ftc-issues-policy-statement-on-biometric-information-signaling-a-new-enforcement-priority) [secondary]; [Steptoe](https://www.steptoe.com/en/news-publications/ftc-policy-statement-on-use-of-biometric-information-technology-lays-the-groundwork-for-future-enforcement-activity.html) [secondary]
- FTC COPPA FAQ: [FTC](https://www.ftc.gov/business-guidance/resources/complying-coppa-frequently-asked-questions)
- BIS facial recognition proposal [July 29, 2024]: [Federal Register](https://www.federalregister.gov/documents/2024/07/29/2024-16498/export-administration-regulations-crime-controls-and-expansionupdate-of-us-persons-controls); [Akin Gump](https://www.akingump.com/en/insights/alerts/bis-proposes-significant-amendments-to-the-ears-us-person-and-end-user-controls-and-a-control-on-facial-recognition-items) [secondary]
- EAR published software: [eCFR §734.7](https://www.ecfr.gov/current/title-15/subtitle-B/chapter-VII/subchapter-C/part-734/section-734.7); [Linux Foundation](https://bestpractices.linuxfoundation.org/export/ear-oss.html); [BIS](https://www.bis.gov/learn-support/encryption-controls/encryption-items-not-subject-to-ear); [Centre Law Group](https://centrelawgroup.com/blog/publication-is-not-a-panacea/) [secondary]
- 47 U.S.C. §230: [Cornell LII](https://www.law.cornell.edu/uscode/text/47/230); *Anderson v. TikTok* [3d Cir. Aug 27, 2024]: [opinion](https://www2.ca3.uscourts.gov/opinarch/223061p.pdf); [JD Supra](https://www.jdsupra.com/legalnews/look-who-s-talking-section-230-and-5466365/) [secondary]
- NIST IR 8280, demographic effects [Dec 2019]: [NIST](https://nvlpubs.nist.gov/nistpubs/ir/2019/NIST.IR.8280.pdf)

**US state and local**

- BIPA definitions: [740 ILCS 14/10](https://lig.ilga.gov/legislation/ilcs/documents/074000140K10.htm); Public Act 103-0769 [Aug 2, 2024]: [ILGA](https://ilga.gov/legislation/publicacts/103/103-0769.htm)
- *Clay v. Union Pacific* [7th Cir. Apr 1, 2026]: [Paul Hastings](https://paulhastings.com/insights/ph-privacy/7th-circuit-confirms-bipa-amendment-has-retroactive-application) [secondary]; [ABA](https://www.americanbar.org/groups/business_law/resources/business-law-today/2026-may/7th-circuit-holds-bipa-damages-remedy-applies-retroactively) [secondary]
- *G.T. v. Samsung* [7th Cir. Aug 7, 2026]: [Justia](https://law.justia.com/cases/federal/appellate-courts/ca7/25-1120/25-1120-2026-08-07.html); [Mayer Brown](https://www.mayerbrown.com/en/insights/publications/2026/08/seventh-circuit-holds-that-bipa-does-not-reach-biometric-data-that-remains-on-a-users-device) [secondary]; [WLF, Sept 1, 2026](https://www.wlf.org/2026/09/01/wlf-legal-pulse/possess-or-collect-under-illinois-bipa-seventh-circuit-resolves-trial-court-split/) [secondary]
- *Hazlitt v. Apple* [2020–21 rulings; order Sept 30, 2026, unsealed Oct 8, 2026]: [JD Supra](https://www.jdsupra.com/legalnews/illinois-federal-court-rules-apple-may-1793870/) [secondary]; [Law360](https://www.law360.com/articles/2536320/apple-faces-trimmed-down-bipa-trial-over-faceprint-claims) [secondary]; [PPC Land](https://ppc.land/apple-loses-bid-to-end-illinois-suit-over-faceprints-in-iphone-photos/) [secondary]; [ID Tech](https://idtechwire.com/apple-asks-judge-to-end-photos-faceprint-class-action-citing-samsung-ruling/) [secondary]
- Clearview BIPA settlement [Mar 20, 2025]: [Bloomberg Law](https://news.bloomberglaw.com/class-action/clearview-ai-gets-settlement-approved-in-face-scan-privacy-case) [secondary]
- Texas CUBI: [Tex. Bus. & Com. Code ch. 503](https://statutes.capitol.texas.gov/Docs/BC/htm/BC.503.htm); [ABA, Winter 2026](https://www.americanbar.org/groups/tort_trial_insurance_practice/resources/brief/2026-winter/everythings-bigger-texas-including-its-data-privacy-initiative) [secondary]; [Texas Lawbook, Meta, July 2024](https://texaslawbook.net/texas-obtains-1-4b-settlement-from-meta-over-facial-recognition-data/) [secondary]
- Washington RCW 19.375: [mirror](https://wa-law.org/rcw/19_business_regulations%E2%80%94miscellaneous/19.375_biometric_identifiers.html) [secondary]; MHMDA RCW 19.373: [WA Legislature](https://app.leg.wa.gov/RCW/default.aspx?cite=19.373); *Maxwell v. Amazon* [filed Feb 10, 2025]: [FKKS](https://technologylaw.fkks.com/post/102jzzg/amazon-faces-lawsuit-under-washingtons-my-health-my-data-privacy-law) [secondary]
- Colorado HB24-1130 [effective July 1, 2025]: [Colorado General Assembly](https://leg.colorado.gov/bills/HB24-1130); [Hunton](https://www.hunton.com/privacy-and-cybersecurity-law-blog/new-colorado-law-imposes-heightened-requirements-for-the-collection-and-processing-of-biometric-data) [secondary]
- CCPA thresholds [Jan 1, 2025 adjustment]: [Jackson Lewis](https://www.jacksonlewis.com/insights/navigating-california-consumer-privacy-act-30-essential-faqs-covered-businesses-including-clarifying-regulations-effective-1126) [secondary]
- NYC biometric law [effective July 9, 2021]: [intro.nyc](https://intro.nyc/local-laws/2021-3); [MoFo](https://www.mofo.com/resources/insights/210623-biometric-identifier-information-law) [secondary]
- Portland ordinance [effective Jan 1, 2021]: [DWT](https://www.dwt.com/blogs/privacy--security-law-blog/2021/01/portland-facial-recognition-ban) [secondary]; [IAPP](https://iapp.org/news/a/local-facial-recognition-bans-begin-to-take-hold) [secondary]
- Georgia citizen's arrest repeal [May 2021]: [Yahoo/HuffPost](https://news.yahoo.com/georgia-citizens-arrest-law-repeal-ahmaud-arbery-191257257.html) [secondary]
- Texas SB 2420 [enjoined Dec 2025; stay June 2026]: [Subscription Insider](https://www.subscriptioninsider.com/article-type/news/texas-app-store-age-verification-law-reaches-jan-1-effective-date-but-federal-injunction-keeps-sb-2420-on-ice) [secondary]; [ZwillGen](https://alerts.zwillgen.com/alerts/injunction-temporarily-lifted-for-texas-app-store-accountability-law) [secondary]
- California AB 1043 [signed Oct 13, 2025]: [Troutman](https://troutmanprivacy.com/2025/10/analyzing-californias-digital-age-assurance-act/) [secondary]

**Civil liability**

- *Walters v. OpenAI* [Ga. Super. Ct. May 2025]: [Cleary Gottlieb](https://www.clearygottlieb.com/news-and-insights/publication-listing/georgia-court-dismisses-defamation-lawsuit-against-openai-over-chatgpt-output) [secondary]; [Loeb & Loeb](https://www.loeb.com/en/insights/publications/2025/05/walters-v-openai-llc) [secondary]
- *Garcia v. Character Technologies* [M.D. Fla. May 2025]: [Tech Justice Law Project](https://techjusticelaw.org/cases/garcia-v-character-technologies-google-and-character-ai-co-founders-daniel-de-frietas-and-noam-shazeer/) [secondary]; [Tech Policy Press](https://techpolicy.press/from-dolls-to-downloads-courts-reimagine-product-liability-for-the-digital-age) [secondary]
- "Bag Men" libel suit [2013; settled]: [ABA Journal](https://www.abajournal.com/news/article/two_men_wrongly_identified_as_possible_suspects_in_boston_marathon_bombing) [secondary]; [Mediaite](https://www.mediaite.com/print/ny-post-settles-defamation-suit-over-bag-men-cover) [secondary]

**EU, UK, and elsewhere**

- GDPR: [EUR-Lex](https://eur-lex.europa.eu/eli/reg/2016/679/oj); Recital 18 mirror: [presencis](https://presencis.com/regulations/gdpr/recital-18/) [secondary]
- *Ryneš* C-212/13 [Dec 11, 2014]: [CJEU](https://curia.europa.eu/juris/liste.jsf?num=C-212/13); [commentary](https://www.thomashelbing.com/en/wissen/dsgvo-hub/rechtsprechung/1.4.20-eugh-rynes) [secondary]
- C-683/21 [Dec 5, 2023]: [CJEU](https://curia.europa.eu/juris/liste.jsf?num=C-683/21); [Loyens & Loeff](https://www.loyensloeff.com/insights/news--events/news/is-an-app-builder-a-data-controller-and-is-fault-required-for-a-gdpr-fine/) [secondary]
- GDPR digital omnibus (pending) [Feb 2026]: [Inside Privacy](https://www.insideprivacy.com/eu-data-protection/eu-regulators-issue-opinion-on-revisions-of-gdpr-and-other-data-laws/) [secondary]; [Kennedys](https://www.kennedyslaw.com/en/thought-leadership/article/2026/the-2025-european-commission-eu-digital-omnibus-package-the-gdpr-regulation-eu-2016679) [secondary]
- AI Act text, AI Act Service Desk: [Art. 2](https://ai-act-service-desk.ec.europa.eu/en/ai-act/article-2), [Art. 3](https://ai-act-service-desk.ec.europa.eu/en/ai-act/article-3), [Art. 5](https://ai-act-service-desk.ec.europa.eu/en/ai-act/article-5), [Art. 43](https://ai-act-service-desk.ec.europa.eu/en/ai-act/article-43), [Annex III](https://ai-act-service-desk.ec.europa.eu/en/ai-act/annex-3), [Recital 17](https://ai-act-service-desk.ec.europa.eu/en/ai-act/recital-17)
- Commission prohibited-practices guidelines [Feb 2025]: [Orrick](https://www.orrick.com/en/insights/2025/04/eu-commission-publishes-guidelines-on-the-prohibited-ai-practices-under-the-ai-act) [secondary]; [Fieldfisher](https://www.fieldfisher.com/en/insights/key-takeaways-from-the-european-commission-s-guide) [secondary]
- AI omnibus, Regulation (EU) 2026/1744 [OJ July 24, 2026; in force July 27, 2026]: [Gibson Dunn](https://www.gibsondunn.com/eu-ai-act-omnibus-agreement-postponed-high-risk-deadlines-and-other-key-changes/) [secondary]; [White & Case](https://www.whitecase.com/insight-alert/eu-ai-omnibus-enters-force-amending-ai-act) [secondary]; [Cooley](https://cdp.cooley.com/digital-ai-omnibus-delays-key-deadlines-introduces-new-rules/) [secondary]
- AI Act open-source exemption: [Linux Foundation Europe](https://linuxfoundation.eu/newsroom/ai-act-explainer) [secondary]; [Orrick, May 2024](https://orrick.com/Insights/2024/05/The-EU-AI-Act-Open-Source-Exceptions-and-Considerations-for-Your-AI-Strategy) [secondary]
- UK ICO guidance and the Data (Use and Access) Act [June 19, 2025]: [Stephenson Harwood](https://www.stephensonharwood.com/insights/data-protection-update-august-2025/) [secondary]; [BABL AI](https://babl.ai/ico-unveils-guidance-on-lawful-biometric-recognition/) [secondary]
- *ICO v Clearview* [UT, Oct 7, 2025]: [A&O Shearman](https://www.aoshearman.com/en/insights/ao-shearman-on-data/upper-tribunal-finds-in-favour-of-ico-in-appeal-against-first-tier-tribunal-decision-on-clearview-ai) [secondary]; [11KBW](https://www.11kbw.com/knowledge-events/case/upper-tribunal-hands-down-clearview-v-ico-judgment-on-territorial-and-material-scope-of-gdpr/) [secondary]
- Canada, *Clearview* findings [Feb 2021]: [OPC](https://priv.gc.ca/en/opc-actions-and-decisions/investigations/investigations-into-businesses/2021/pipeda-2021-001); Quebec: [Miller Thomson](https://www.millerthomson.com/en/insights/cybersecurity/privacy-commissioners-take-stance-against-collection-of-biometric-data/) [secondary]
- Australia statutory tort [June 10, 2025]: [College of Law](https://www.collaw.edu.au/community/news/australias-new-tort-of-privacy-is-in-force-what-you-need-to-know/) [secondary]; Bunnings [ART Feb 4, 2026]: [Clayton Utz](https://www.claytonutz.com/insights/2026/february/bunnings-wins-appeal-on-facial-recognition-technology-key-takeaways-for-businesses-and-privacy-law) [secondary]

**App stores**

- Apple App Review Guidelines: [developer.apple.com](https://developer.apple.com/app-store/review/guidelines/); Oct 16, 2026 update: [Apple Developer news](https://developer.apple.com/news/?id=92th7e5i)
- 5.1.1 "even public databases" [June 2019]: [mjtsai](https://mjtsai.com/blog/2019/06/04/updates-to-the-app-store-review-guidelines/) [secondary]; [9to5Mac, Oct 2020](https://9to5mac.com/2020/10/30/an-iphone-app-designed-to-verify-pennsylvania-mail-in-ballots-gets-rejected-by-apple) [secondary]
- 1.7 crime reporting [June 2021]: [Gummicube](https://www.gummicube.com/blog/app-store-review-guidelines-june-2021-updates) [secondary]; [Apple developer forum](https://developer.apple.com/forums/thread/736371)
- 5.1.1(ix) legal entity: [Apple developer forum](https://developer.apple.com/forums/thread/750657)
- ICEBlock removal [Oct 3, 2025]: [Al Jazeera](https://www.aljazeera.com/news/2025/10/3/apple-pulls-iceblock-from-app-store-following-us-government-pressure)
- Google Play government information: [Play Console Help](https://support.google.com/googleplay/android-developer/answer/9514050?hl=en); government badge [May 2024]: [TechCrunch](https://techcrunch.com/2024/05/02/google-will-now-show-labels-in-play-store-to-denote-official-government-apps); Data safety: [Play Help](https://support.google.com/googleplay/answer/11416267?hl=en)

**Open source and models**

- FFmpeg license and legal: [ffmpeg.org/legal.html](https://www.ffmpeg.org/legal.html); MSYS2 FFmpeg (GPL-3.0-or-later): [packages.msys2.org](https://packages.msys2.org/base/mingw-w64-ffmpeg)
- YuNet (MIT): [OpenCV Zoo README](https://huggingface.co/opencv/opencv_zoo/blob/d33294bfaec5f2386dab1783e1a4031330645f57/models/face_detection_yunet/README.md); training note: [mirror model card](https://huggingface.co/pollen-robotics/face_detection_yunet_2023mar) [secondary]
- SFace (Apache-2.0): [OpenCV Zoo](https://github.com/opencv/opencv_zoo/blob/main/models/face_recognition_sface/README.md); paper and training sets: [arXiv 2205.12010](https://arxiv.org/pdf/2205.12010)
- MS-Celeb-1M: [Microsoft Research](https://www.microsoft.com/en-us/research/?p=270507); Glint360K provenance: [InsightFace challenge report](https://arxiv.org/pdf/2108.08191)
- NCMEC terms: [missingkids.org/termsandconditions](https://www.missingkids.org/termsandconditions); Poster API: [registration](https://www.missingkids.org/gethelpnow/search/poster-api-registration)
