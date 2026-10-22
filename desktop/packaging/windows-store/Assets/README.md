# Store logos

The MSIX manifest (`../AppxManifest.xml`) references these PNG files. They are **not**
committed as binaries. Drop real icons here before a real Store submission, or let CI
fill them with clearly-marked placeholders (`../make-placeholder-assets.ps1`) when it
only needs to pack and validate the package.

| File                   | Size (px) | Used for                                   |
| ---------------------- | --------- | ------------------------------------------ |
| `Square44x44Logo.png`  | 44 x 44   | App-list icon, taskbar, title bar          |
| `Square150x150Logo.png`| 150 x 150 | Medium Start tile                          |
| `Wide310x150Logo.png`  | 310 x 150 | Wide Start tile (`DefaultTile`)            |
| `StoreLogo.png`        | 50 x 50   | Store listing / Properties `<Logo>`        |

Notes:

- The source art is `../../app.openworld.OpenWorld.svg` (a picture frame with a lens,
  accent blue `#2F5D8C` on white). Export it to the sizes above, trimmed and padded to
  square where the manifest expects a square, with a transparent or `#2F5D8C` background.
- For a polished Store listing you may also add scaled variants
  (`Square44x44Logo.scale-200.png`, etc.) and a `Square310x310Logo.png` /
  `Square71x71Logo.png`; they are optional and not referenced by the current manifest.
- The Microsoft Store also wants listing screenshots and a 300x300 (or larger) listing
  icon uploaded in Partner Center directly — those are not part of the package.
