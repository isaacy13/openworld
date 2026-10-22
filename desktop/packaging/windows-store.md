# Microsoft Store (MSIX)

The `windows-store` workflow packages the GTK desktop app as an **MSIX** for the Microsoft
Store. The package is a full-trust (Win32 / desktop-bridge) MSIX: the same
`openworld-desktop.exe`, its DLLs, `ffmpeg.exe`/`ffprobe.exe`, the pinned
`onnxruntime.dll`, the bundle catalog, and GTK's run-time data that
`windows-bundle.sh` collects (see [windows.md](windows.md)), copied under `app\` with the
layout intact. The app still finds its DLLs and its `share\`/`lib\` data next to the
executable, so nothing is remapped through a VFS folder.

**The Store signs it, we do not.** When you submit the package, the Store re-signs it to
your Store-assigned identity. There is no code-signing certificate in this repo and the
workflow adds no signing step. A checksum shipped beside a binary is not the check
(`docs/decisions.md`); for the Store, the signature the Store applies is.

## Files

- `windows-store/AppxManifest.xml` — the package manifest (placeholders marked inline).
- `windows-store/msix-layout.sh` — stages a `makeappx`-ready root from the bundle folder.
- `windows-store/Assets/README.md` — the four logos the manifest needs, and their sizes.
- `windows-store/make-placeholder-assets.ps1` — writes clearly-marked placeholder logos so
  the package can be packed and validated before the real icons exist.
- `.github/workflows/windows-store.yml` — builds, bundles, lays out, packs, validates.

## What the owner must supply

### 1. Store identity (from Partner Center)

Reserve the app name in Partner Center, then open **Product → Product identity**. It shows
three values; copy each into `windows-store/AppxManifest.xml`, replacing the `PLACEHOLDER`
text:

| Partner Center                          | AppxManifest.xml                         |
| --------------------------------------- | ---------------------------------------- |
| Package/Identity/Name                   | `<Identity Name="...">`                  |
| Package/Identity/Publisher              | `<Identity Publisher="CN=...">`          |
| Package/Properties/PublisherDisplayName | `<Properties><PublisherDisplayName>`     |

- The `Publisher` is a `CN=...` string containing your Store publisher hash. **Do not
  invent it** — the uploaded package's Identity must match exactly what Partner Center
  assigned, or the Store rejects it.
- `Version` stays `N.N.N.0`: the Store reserves the fourth part (revision), so leave it `0`
  and bump the first three for each submission.
- `ProcessorArchitecture` is `x64` (the only build target).

### 2. Icons

Export `desktop/packaging/app.openworld.OpenWorld.svg` to the four PNGs listed in
`windows-store/Assets/README.md` (`StoreLogo.png` 50×50, `Square44x44Logo.png` 44×44,
`Square150x150Logo.png` 150×150, `Wide310x150Logo.png` 310×150) and commit them under
`windows-store/Assets/`. Until then the workflow fills them with placeholder squares so the
pack and validation still run — those placeholders are **not** shippable. Partner Center
also asks for listing screenshots and a store listing icon uploaded directly; those are not
part of the package.

## What CI does (`windows-store.yml`, `windows-latest`)

1. Builds the desktop app with MSYS2 UCRT64 GTK and rustup's pinned
   `1.97.0-x86_64-pc-windows-gnu`, the same toolchain as `ci.yml`'s `desktop-windows` job
   (plus `ntldd`, which the bundle script needs).
2. Fetches the pinned ONNX Runtime (`scripts/fetch_ort.py windows-x86_64`).
3. Runs `desktop/packaging/windows-bundle.sh dist` to assemble the runnable folder.
4. Writes placeholder logos if real ones are absent, then runs
   `windows-store/msix-layout.sh dist msix-root`.
5. Checks the manifest is well-formed and every file it references is present.
6. Runs `makeappx pack` (which validates `AppxManifest.xml` against the package schema and
   fails on any missing referenced file) and round-trips it with `makeappx unpack`.
7. Runs the Windows App Certification Kit if it is installed on the runner (best effort,
   non-gating — WACK's full checks need an elevated, installed package).
8. Uploads the unsigned `.msix` as the `openworld-msix-unsigned` artifact. It is **not**
   submittable as-is while the identity and icons are placeholders.

## Submitting

1. Fill in the real identity and icons (above). Re-run the workflow to produce an `.msix`
   with your identity and real icons, or build it locally:

   ```sh
   # in an MSYS2 UCRT64 shell, from the repo root
   desktop/packaging/windows-bundle.sh dist
   desktop/packaging/windows-store/msix-layout.sh dist msix-root
   # then, from a normal Windows shell with the Windows SDK:
   makeappx pack /o /d msix-root /p openworld.msix
   ```

2. Upload `openworld.msix` in the Partner Center submission (**Packages**). Do not sign it
   first; the Store signs it on acceptance.
3. Complete the store listing (description, screenshots, age rating, privacy — the scan is
   on-device and makes no network request; "Update posters" is the only networked action).

## Not yet verified

`makeappx` and WACK run only on Windows; this was authored on Linux, so the `makeappx pack`
step and WACK have not been executed here. The first CI run on `windows-latest` is the first
real test of the pack and validation. The manifest's placeholder identity must be replaced
before a package the Store will accept can be built.
