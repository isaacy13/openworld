# Measurements

`measure_fast.py` runs the `openworld measure` command and writes the Fast curve. The script only counts. The scan is the Rust library.

The curve reports, at the locked cutoff:

- True positive: same fixture identity, score at or above the cutoff.
- False negative: same fixture identity, score below the cutoff.
- False positive: different fixture identity, score at or above the cutoff.
- True negative: different fixture identity, score below the cutoff.

Each rate has a count, a trial count, and a 95% Wilson interval. Genuine trials are 16 fixture identities at 10 placements. Impostor trials are 160 pairs, and each probe identity differs from its poster. A trial that is not compared is refused instead of being counted as a true negative. Score min, median, and max are published for both sets.

The same run still reports false match rate, false non-match rate, detection recall at the 64 px rule, faces seen but not compared, and the miss rate for a face visible about one second. Plate text is counted apart from faces: a published plate that matches, a different plate, and a plate with nothing published.

The published Fast curve is a fixture-marker measurement. `real_posters_allowed` is false.

## Open

`real_faces.py` measures the Open bundle and writes `bundles/open/curve.json`. It downloads LFW and WIDER FACE validation into `--data` and refuses an archive whose SHA-256 is not the pin. No photograph is committed. It needs `eval/real` built and the Open weights fetched with `scripts/fetch_weights.py open`.

- LFW: the ten standard folds, then every same-person and different-person pair, so a false match rate near one in a million has trials behind it. The cutoff is the lowest score whose all-pairs false match rate is at or under one in a hundred thousand. The miss rate is reported over all pairs and over one pair per person; the second is what one poster against one photo sees.
- Scans: 100 LFW people enrolled from one photo each, a different photo of each, and 100 strangers, through `openworld scan`. Then 1280x720 H.264 clips with the enrolled face on screen for one second, under both coverages.
- Held out: CALFW and CPLFW, 6,000 pairs each, read from a Hugging Face mirror at a pinned commit and checked against one digest over the pairs file and every photo. The LFW cutoff is applied unchanged. A same-person pair with a photo that has no compared face counts as a miss.
- WIDER FACE: detection recall for labeled faces 72 px and up on the detection image, and detections that land on no labeled face.

LFW is press photos. It sets Open's cutoff on that photo type only. It is not a mugshot or poster set, and NIST numbers are not this curve. SCface needs that project's research agreement. The Open curve sets `real_posters_allowed` to false.

`--people`, `--wider-images`, `--scan-people`, and `--video-people` take a fixed sample; the held-out sets use their first fold. With `--check`, a sample passes when each published rate lies inside the sample's 95% interval. The full run instead has to clear fixed floors, and the manifest cutoff has to sit at or within 0.01 above the measured one.
