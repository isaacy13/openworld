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

    private fun salt(x: Int, y: Int, side: Int): Long {
        var hash = 0x811c9dc5.toInt()
        for (v in intArrayOf(x, y, side, side, 0, 0)) {
            hash = hash xor v
            hash *= 0x01000193
        }
        return hash.toLong() and 0xFFFFFFFFL
    }
}
