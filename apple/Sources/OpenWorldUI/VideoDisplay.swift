// SPDX-License-Identifier: Apache-2.0
import Foundation

/// Display size for a video frame. A quarter turn runs first and inverts the
/// pixel aspect, matching the desktop player. The frame count uses the video
/// track. Audio that continues after the pictures is not part of that count.
public enum VideoDisplay {
    public static func shownSize(
        codedWidth: Int,
        codedHeight: Int,
        sarNum: Int,
        sarDen: Int,
        quarterTurn: Bool
    ) -> (Int, Int) {
        let square = sarNum > 0 && sarDen > 0 && sarNum != sarDen
        if quarterTurn && square {
            return (squareWidth(codedHeight, num: sarDen, den: sarNum), codedWidth)
        }
        if quarterTurn {
            return (codedHeight, codedWidth)
        }
        if square {
            return (squareWidth(codedWidth, num: sarNum, den: sarDen), codedHeight)
        }
        return (codedWidth, codedHeight)
    }

    public static func squareWidth(_ width: Int, num: Int, den: Int) -> Int {
        if width <= 0 || num <= 0 || den <= 0 || num == den {
            return max(width, 1)
        }
        let shown = (Int64(width) * Int64(num) + Int64(den) / 2) / Int64(den)
        if shown < 1 {
            return 1
        }
        if shown > Int64(Int.max) {
            return width
        }
        return Int(shown)
    }

    /// `frame_000002.png`. A grouped locale writes `frame_000,002.png`, and past a million
    /// frames that name sorts ahead of an earlier frame. The scan reads names in sorted order.
    public static func frameFileName(_ index: Int, locale _: Locale = .current) -> String {
        String(format: "frame_%06d.png", locale: Locale(identifier: "en_US_POSIX"), index)
    }

    /// How many pictures the estimate counts. `pictureSeconds` is the video track.
    /// A count that does not fit in `Int` is 0, so the estimate refuses instead of trapping.
    public static func frameCount(pictureSeconds: Double, rate: Double) -> Int {
        guard pictureSeconds.isFinite, pictureSeconds > 0, rate.isFinite, rate > 0 else {
            return 1
        }
        let pictures = (pictureSeconds * rate).rounded()
        guard pictures.isFinite, let count = Int(exactly: pictures), count >= 1 else {
            return 0
        }
        return count
    }
}
