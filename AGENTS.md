# Agents

Read [docs/handoff.md](docs/handoff.md) first, then [docs/decisions.md](docs/decisions.md), which corrects it. Treat the specs in [docs/](docs/) as locked decisions. Do not relitigate them. If two specs disagree, [docs/decisions.md](docs/decisions.md) wins, then [docs/handoff.md](docs/handoff.md).

Phone `complete` scans are allowed after the user sees the estimate. The detection size is automatic: a face 112 px across in the original reaches the detector. Fast (YuNet + SFace) is the pre-selected bundle. Accurate is selectable and not measured yet. Compare only when the original-frame face is at least 112 px and passes the quality gates. A video face needs three gated frames or one second to be a candidate. Plates and vehicles are out of v0. A tap opens that poster's FBI URL. There is no poster-signing server. There is no release yet, so do not invent a SHA-256. When a release exists, the checksum is filled from that GitHub release. A checksum shipped beside the binary is not the check. Coverage policies and fail-closed rules stay as written in the specs.

Shells call the core through [docs/api.md](docs/api.md) and follow [docs/ux.md](docs/ux.md). They never compare English text to decide anything; they read codes.
