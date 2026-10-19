# OpenWorld UX

One flow, three screens, on every platform. Each shell uses its platform's controls, type, and motion (SwiftUI on iPhone and Mac, Material 3 on Android, libadwaita on Linux, Fluent spacing on Windows) and the same words, by code, from the core (`docs/api.md`).

## Principles

1. **Calm, not alarming.** A candidate is a possible match to review, not an alert. No red, no sirens, no "match found". Amber marks "look at this". Red is only for errors.
2. **No result looks like an all-clear.** "No candidate" is neutral grey, never green, and always says it is not a clearance. Incomplete is orange and says why.
3. **Show what happens before it happens.** Before Analyze: what stays on the device, what is compared, how long it takes, what it costs the battery. The full disclosure is one tap away on every screen.
4. **Evidence over scores.** A candidate shows the crop from the file beside the poster photo, the frame it came from, and a band ("Clearly past the locked cutoff"). There is no number to argue with and no slider.
5. **Few decisions.** The file, the classes (missing, wanted), and Analyze. The detection size is automatic. Bundle and coverage live under "Options", with Fast and "Every frame" chosen.
6. **Leaving is deliberate.** Opening an FBI page goes through one confirmation sheet with the class-specific do-not-approach line.

## Visual language

| Token | Light | Dark | Use |
| --- | --- | --- | --- |
| `accent` | #2F5D8C | #8DB4E2 | Primary buttons, links, selection |
| `candidate` | #8F5600 on #FFF4E0 | #FFC266 on #3A2A10 | Candidate headline and card edge |
| `incomplete` | #B34700 on #FFEDE0 | #FFA066 on #3A2210 | Incomplete headline, stop reasons |
| `neutral` | #4A5560 on #EEF1F4 | #C3CCD5 on #232A31 | No candidate, not compared |
| `error` | #B3261E on #FCE8E6 | #F2B8B5 on #3B1D1B | Refusals |
| `demo` | #6B4FA0 on #F1ECFA | #C9B6F0 on #2B2338 | The demo-posters badge |

- Contrast: text on every tint is at least 4.5:1 (WCAG AA). Status is never shown by color alone: each state has an icon and words.
- Corners: 12 px cards, 20 px sheets (platform defaults where they exist).
- Spacing: 8 px grid; 16 px side gutters on phones, 24 px on desktop.
- Type: the platform's own (SF Pro, Roboto Flex via Material 3, Cantarell/Adwaita Sans, Segoe UI Variable). Large title on the first screen only.
- Icons: SF Symbols, Material Symbols, Adwaita icons. `lock.shield` for on-device, `photo`/`film` for the file, `person.crop.square` for faces, `exclamationmark.triangle` for incomplete, `arrow.up.forward.square` for leaving.
- Motion: crops fade in as they are found; respect Reduce Motion.

## Screens

### 1. Start

- Title **OpenWorld**. One line under it: "Check a photo or video you already have against public FBI missing and wanted posters, on this device."
- A large drop card with the primary button **Choose a photo or video** (phone: Photos picker; a second button, **Files**). Desktop: "or drop it here". A shared file arrives here too.
- Three trust chips: **Stays on this device** · **Nothing uploaded** · **No camera**.
- Footer: the poster pack, e.g. "Demo posters · 2 posters · fetched today" with the demo badge, or "No posters installed. Real FBI photos stay off."

### 2. Review (before Analyze)

- **File card**: thumbnail, name (wrapped, never truncated mid-word), kind, duration or size, recorded date. File warnings (old, timestamps disagree) as an inline caution row.
- **Estimate card**: the time, large ("About 2 minutes"), then "1,800 frames · faces looked for at 960 px". Device notes as rows with icons (CPU, heat, battery, "a computer will finish this sooner").
- **Compare with**: two toggle chips, **Missing** and **Wanted**, each with its poster count. Both off disables Analyze and says "Choose missing, wanted, or both."
- **Options** (collapsed): Model bundle (rows: name, best-for, "Measured on LFW" or "Not measured yet", Fast first and selected) and Coverage (**Every frame**, or **5 frames a second** with "A brief face can be missed.").
- **What happens to this file** card: the first three disclosure lines and **Read all** (a sheet with every line).
- Primary **Analyze on this device**; secondary **Choose another file**.

### 3. Scan → Result (one screen)

While scanning:
- A progress bar with "Frame 412 of 1,800 · about 1 minute left" and "Scanning on this device" with the lock icon.
- A grid of faces as they are found, each with a chip that updates: Looking → Compared / Not compared (why) / Possible candidate.
- **Stop**. A stop is Incomplete, never a clearance.

When done, the headline card, by state:
- **Possible candidate. Not an identification.** (amber) with how many.
- **No candidate is not a clearance.** (neutral) with the faces line ("3 faces found. 2 could be compared. The rest are not cleared.").
- **Incomplete.** (orange) with the reason.
- A refusal (error) with its sentence and what to do.

Then:
- **Candidate cards**: the crop from the file and the poster photo side by side; poster title and a class badge (Missing, Wanted); the band; the frame label with **Show frame**; **Open FBI page** (disabled when the poster has no page).
- **Details** (collapsed): frames analyzed, coverage, detection size, chance line, classes, bundle and poster pack with their SHA-256, the device that ran it.
- **Other faces**: the grid with each face's label and reason.
- The disclosure lines.
- **Delete result** (destructive, confirmed) and **Scan another file**.

### Leaving sheet

Title **You are leaving OpenWorld.** Then: "OpenWorld is not sending a report.", the class line (wanted: do not approach, 1-800-CALL-FBI or tips.fbi.gov; missing: not a criminal accusation, do not approach, NCMEC 1-800-THE-LOST), the file's age and timestamp warnings, "This file is not authenticated." Buttons **Open FBI page** and **Stay**. Only the poster's own fbi.gov URL opens.

## Behavior

- Back steps to the previous screen; during a scan it asks before stopping.
- Turning the phone, changing text size, language, or color mode keeps the screen and the scan.
- The phone keeps the screen on during a scan, stops on serious heat or a low battery that is not charging, and on leaving the foreground. Each stop is Incomplete with its reason. A scan the system killed shows as Incomplete on the next launch.
- Results are deleted after 7 days (or when the posters expire); the start screen says so. Every launch sweeps expired results and leftover temporary copies.
- Every crop has an accessible label: "Face from frame 12. Possible candidate." Headlines are announced when they appear.
