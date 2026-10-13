# Measurements

`measure_fast.py` runs the `openworld measure` command and writes the Fast curve. The script only counts. The scan is the Rust library.

The curve reports, at the locked cutoff:

- True positive: same fixture identity, score at or above the cutoff.
- False negative: same fixture identity, score below the cutoff.
- False positive: different fixture identity, score at or above the cutoff.
- True negative: different fixture identity, score below the cutoff.

Each rate has a count, a trial count, and a 95% Wilson interval. Genuine trials are 16 fixture identities at 10 placements. Impostor trials are 160 pairs, and each probe identity differs from its poster. A trial that is not compared is refused instead of being counted as a true negative. Score min, median, and max are published for both sets.

The same run still reports false match rate, false non-match rate, detection recall at the 64 px rule, faces seen but not compared, and the miss rate for a face visible about one second. Plate text is counted apart from faces: a published plate that matches, a different plate, and a plate with nothing published.

LFW can show that a scorer runs. It is the wrong photo type to set the cutoff, and it is not used here. WIDER FACE can score detection when a detector weight is pinned. SCface needs that project's research agreement. NIST numbers are not this curve. No face photographs are committed.

The published Fast curve is a fixture-marker measurement. `real_posters_allowed` is false.
