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

    /// Seconds each picture stays up. The last picture runs to the end of the track.
    /// A missing or backwards clock has no list.
    public static func presentationDelays(times: [Double], duration: Double) -> [Double]? {
        if times.count < 2 || times.contains(where: { !$0.isFinite || $0 < 0 }) { return nil }
        var delays: [Double] = []
        delays.reserveCapacity(times.count)
        for index in 0..<(times.count - 1) {
            let gap = times[index + 1] - times[index]
            if !gap.isFinite || gap < 0 { return nil }
            delays.append(gap)
        }
        let lastStart = times[times.count - 1]
        let last = duration > lastStart ? duration - lastStart : delays[delays.count - 1]
        if !last.isFinite || last < 0 { return nil }
        delays.append(last)
        if delays.allSatisfy({ $0 == 0 }) { return nil }
        return delays
    }

    /// A held picture is not the average rate. A steady camera rate stays on that path.
    public static func presentationVaries(_ delays: [Double], fps: Double) -> Bool {
        guard fps.isFinite, fps > 0 else { return false }
        let nominal = 1.0 / fps
        let slack = nominal * 0.5
        return delays.contains { abs($0 - nominal) > slack }
    }

    /// The estimate counts pictures on screen, one each, at 1 fps.
    /// A steady rate keeps the track's own frame count.
    public static func measuredPlan(rateFrames: Int, fps: Double, delays: [Double]?) -> (frames: Int, fps: Double) {
        guard let delays, presentationVaries(delays, fps: fps) else {
            return (rateFrames, fps)
        }
        let keep = framesOnScreen(delays)
        if keep.isEmpty { return (rateFrames, fps) }
        return (keep.count, 1.0)
    }

    /// One delay per line, with a period. Nil when the frame rate already names the pictures.
    public static func presentationText(times: [Double], duration: Double, fps: Double) -> String? {
        guard let delays = presentationDelays(times: times, duration: duration),
              presentationVaries(delays, fps: fps) else {
            return nil
        }
        let lines = delays.map { delay in
            String(format: "%.4f", locale: Locale(identifier: "en_US_POSIX"), delay)
        }
        return lines.joined(separator: "\n") + "\n"
    }

    /// Frames on screen at each 5 Hz tick. The same rule as the scan.
    public static func framesOnScreen(_ delays: [Double]) -> [Int] {
        if delays.isEmpty { return [] }
        var starts: [Double] = []
        starts.reserveCapacity(delays.count)
        var end = 0.0
        for delay in delays {
            if !delay.isFinite || delay < 0 { return [0] }
            starts.append(end)
            end += delay
        }
        if !(end > 0) || !end.isFinite { return [0] }
        var out: [Int] = []
        var tick = 0
        while tick <= 1_000_000 {
            let when = Double(tick) / 5.0
            if !when.isFinite || when + 1e-9 >= end { break }
            var low = 0
            var high = starts.count
            while low < high {
                let mid = (low + high) / 2
                if starts[mid] <= when {
                    low = mid + 1
                } else {
                    high = mid
                }
            }
            let idx = max(low - 1, 0)
            if out.last != idx { out.append(idx) }
            tick += 1
        }
        if out.isEmpty { return [0] }
        return out
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
