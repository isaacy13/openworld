# OpenWorld design

Start at [handoff.md](handoff.md). That file is the agent entry brief. The specs below are locked decisions.

| Spec | One line |
| --- | --- |
| [handoff.md](handoff.md) | v0 goal, what not to build, the local cascade, bundles, screens, and build order. |
| [feasibility.md](feasibility.md) | Why a compact on-device cascade is the scanner, and why Jev, Laya, and Clef are not. |
| [project-context.md](project-context.md) | Purpose, must / must not, coverage, FBI sources, platforms, and v0 defaults. |
| [design-gaps.md](design-gaps.md) | Threats, the fail-closed table, the tightened v0, and what stays parked. |
| [frictionless-ux.md](frictionless-ux.md) | Intake: picker and share sheet, the downscale prompt, the estimate, pause, and delete. |
| [trust.md](trust.md) | Disclosure before Analyze, what a release may claim, and the README checksum rule. |
| [record-classes.md](record-classes.md) | `missing` and `wanted` labels, what to skip, and sources that are not v0. |
| [mvp-questions.md](mvp-questions.md) | Closed locks, and the four defaults that unblock the fixture CLI. |
| [release-gates.md](release-gates.md) | The measured curve v0 needs, and what still blocks a public app. |
| [validation.md](validation.md) | What Fast must measure: FMR/FNMR, 64 px recall, and the ~1 s miss rate. |
| [platforms.md](platforms.md) | One Rust core, native shells, Fast pre-selected, Accurate selectable and not measured yet. |

If two specs disagree, [handoff.md](handoff.md) and the later corrections win. Phone `complete` scans are allowed after the estimate. Downscale is a prompt. Accurate is selectable and not measured yet.

Related:

- Pipeline, 64 px detect / ~112 px compare, Fast default, coverage: [handoff.md](handoff.md), [design-gaps.md](design-gaps.md), [mvp-questions.md](mvp-questions.md), [platforms.md](platforms.md).
- Jev, Laya, and Clef stay out of the pixel loop: [feasibility.md](feasibility.md), [project-context.md](project-context.md).
- Estimates, the downscale prompt, and phone `complete`: [frictionless-ux.md](frictionless-ux.md), [design-gaps.md](design-gaps.md).
- Disclosure, FBI URL tap only, no poster-signing server, no invented checksum: [trust.md](trust.md), [design-gaps.md](design-gaps.md), [release-gates.md](release-gates.md).
- Classes and copy: [record-classes.md](record-classes.md), [trust.md](trust.md).
- What to measure before real posters: [validation.md](validation.md), [release-gates.md](release-gates.md).
