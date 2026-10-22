// SPDX-License-Identifier: Apache-2.0
package app.openworld

/**
 * A still with one demo marker (poster "fixture-missing-a", marker 7) on white, as RGBA bytes.
 * Synthetic: no photograph of a person. It mirrors the core's fixture so a demo scan of it
 * finds a candidate: the marker's demo score depends only on where it sits, and this picks a
 * spot whose placement hash puts the probe on the poster's own vector.
 */
object DemoScene {
    const val WIDTH = 640
    const val HEIGHT = 480
    private const val MARKER = 7
    private const val MODULE = 16
    /** The marker is 8 modules, 128 px across: over the 112 px compare size. */
    const val SIDE = 8 * MODULE

    /**
     * A sighting passes the demo cutoff (0.55) when its placement hash is at most 70 mod 100:
     * the probe mixes the poster vector with a share of 0.85 * hash / 99 of orthogonal noise.
     * 66 keeps a margin (cosine 0.61). A template, the mean of passing sightings, passes too.
     */
    private const val PASS_MAX = 66L
    private val DATA_CELLS = listOf(0 to 1, 0 to 2, 0 to 3, 1 to 0, 1 to 1, 1 to 2, 1 to 3, 2 to 0, 2 to 1, 2 to 2, 2 to 3, 3 to 0, 3 to 1, 3 to 2)

    fun rgba(): ByteArray {
        val pixels = ByteArray(WIDTH * HEIGHT * 4) { -1 }
        val side = 8 * MODULE
        val (x0, y0) = origin(side)
        val black = cells()
        for (r in 0 until 8) for (c in 0 until 8) {
            if (!black[r * 8 + c]) continue
            for (y in y0 + r * MODULE until y0 + (r + 1) * MODULE) for (x in x0 + c * MODULE until x0 + (c + 1) * MODULE) {
                val i = (y * WIDTH + x) * 4
                pixels[i] = 0
                pixels[i + 1] = 0
                pixels[i + 2] = 0
            }
        }
        return pixels
    }

    /** The 8x8 module grid of marker 7: border, white ring, then 4x4 data bits. */
    private fun cells(): BooleanArray {
        val id = MARKER and 0x3FF
        val packed = (id shl 4) or ((id xor (id shr 4) xor (id shr 8)) and 0x0F)
        val bits = BooleanArray(16)
        bits[0] = true
        DATA_CELLS.forEachIndexed { i, (r, c) -> bits[r * 4 + c] = (packed shr (13 - i)) and 1 == 1 }
        return BooleanArray(64) { k ->
            val r = k / 8
            val c = k % 8
            val border = r == 0 || r == 7 || c == 0 || c == 7
            val ring = !border && (r == 1 || r == 6 || c == 1 || c == 6)
            border || (!ring && bits[(r - 2) * 4 + (c - 2)])
        }
    }

    /** A place whose placement hash is 0 mod 100, where the demo probe equals the poster vector. */
    private fun origin(side: Int): Pair<Int, Int> {
        for (y in 12 until HEIGHT - side) for (x in 12 until WIDTH - side) {
            if (salt(x, y, side) % 100L == 0L) return x to y
        }
        error("no placement")
    }

    /**
     * A corner on the 16 px grid (so H.264 blocks line up with the modules) where the marker
     * passes on every frame in `frames` of a [WIDTH] x [HEIGHT] picture. The hash has structure:
     * eight frames starting on a multiple of 8 never all pass, so pick the frames with care.
     */
    fun videoSpot(frames: LongRange): Pair<Int, Int> {
        for (step in intArrayOf(16, 8, 2)) {
            for (y in 16 until HEIGHT - SIDE - 16 step step) for (x in 16 until WIDTH - SIDE - 16 step step) {
                if (frames.all { salt(x, y, SIDE, it) % 100L <= PASS_MAX }) return x to y
            }
        }
        error("no spot passes on frames $frames")
    }

    /**
     * An upright [WIDTH] x [HEIGHT] luma picture (BT.601 video range: 16 black, 235 white),
     * white, with the marker's corner at `spot` when there is one.
     */
    fun luma(spot: Pair<Int, Int>?): ByteArray {
        val luma = ByteArray(WIDTH * HEIGHT) { WHITE }
        if (spot == null) return luma
        val (x0, y0) = spot
        val black = cells()
        for (r in 0 until 8) for (c in 0 until 8) {
            if (!black[r * 8 + c]) continue
            for (y in y0 + r * MODULE until y0 + (r + 1) * MODULE) {
                luma.fill(BLACK, y * WIDTH + x0 + c * MODULE, y * WIDTH + x0 + (c + 1) * MODULE)
            }
        }
        return luma
    }

    /**
     * The picture to store so a player that turns it 90 degrees clockwise shows `upright`:
     * [HEIGHT] wide and [WIDTH] tall. Stored (x, y) is upright (WIDTH - 1 - y, x).
     */
    fun storedForQuarterTurn(upright: ByteArray): ByteArray {
        val width = HEIGHT
        val height = WIDTH
        val stored = ByteArray(width * height)
        for (y in 0 until height) for (x in 0 until width) {
            stored[y * width + x] = upright[x * WIDTH + (WIDTH - 1 - y)]
        }
        return stored
    }

    private const val BLACK: Byte = 16
    /** 235 as a signed byte. */
    private const val WHITE: Byte = -21

    /** The core's `fixture::placement_salt` for a box at (x, y), `side` across, on `frame`. */
    private fun salt(x: Int, y: Int, side: Int, frame: Long = 0): Long {
        var hash = 0x811c9dc5.toInt()
        for (v in intArrayOf(x, y, side, side, frame.toInt(), (frame ushr 32).toInt())) {
            hash = hash xor v
            hash *= 0x01000193
        }
        return hash.toLong() and 0xFFFFFFFFL
    }
}
