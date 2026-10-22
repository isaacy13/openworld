// SPDX-License-Identifier: Apache-2.0
package app.openworld

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import app.openworld.core.JobStart
import app.openworld.core.Report
import app.openworld.core.ScanJob
import app.openworld.core.Status
import app.openworld.core.Stop
import app.openworld.core.request
import app.openworld.media.MediaInfo
import app.openworld.media.Probe
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.io.RandomAccessFile
import java.nio.ByteBuffer
import java.util.UUID

/**
 * Videos made on this device with MediaCodec and MediaMuxer, scanned through the app's own
 * path: AndroidMediaReader probes and decodes with MediaExtractor and MediaCodec, and pushes
 * each picture into the linked core. The demo marker is on screen for 1.2 seconds.
 */
@RunWith(AndroidJUnit4::class)
class VideoScanTest {
    private val app = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext as OpenWorldApplication
    private val env = app.env
    private lateinit var work: File
    private lateinit var posters: File

    @Before
    fun setUp() {
        env.install()
        work = File(app.cacheDir, "video-tests-${UUID.randomUUID()}").apply { mkdirs() }
        posters = File(work, "posters")
        val pack = env.core.call(request("posters_demo") { put("out", posters.path) })
        assertTrue(pack.toString(), pack.optBoolean("ok"))
    }

    @After
    fun tearDown() {
        work.deleteRecursively()
    }

    @Test
    fun aVideoFromThePlatformEncoderFindsTheMarker() {
        val file = clip("upright.mp4", rotation = 0)
        val info = probe(file)
        assertTrue(info.video)
        assertEquals(FRAMES.toLong(), info.frames)
        assertEquals(0, info.rotation)
        assertEquals(DemoScene.WIDTH to DemoScene.HEIGHT, info.width to info.height)
        val report = scan(file, info)
        assertCandidate(report)
    }

    @Test
    fun aVideoWithARotationHintIsScannedAsShown() {
        val file = clip("turned.mp4", rotation = 90)
        val info = probe(file)
        // The stored pictures are portrait; the hint turns them upright for the core.
        assertEquals(90, info.rotation)
        assertEquals(DemoScene.HEIGHT to DemoScene.WIDTH, info.width to info.height)
        assertCandidate(scan(file, info))
    }

    @Test
    fun aStopMidScanIsIncomplete() {
        val file = clip("stopped.mp4", rotation = 0)
        val info = probe(file)
        val report = scan(file, info, stopAfter = 3)
        assertEquals(describe(report), Status.INCOMPLETE, report.status)
        assertEquals("incomplete", report.headline.code)
        assertEquals("cancelled", report.reason?.code)
        assertNotACleanPass(report)
        assertTrue(describe(report), report.framesDecoded < FRAMES)
    }

    @Test
    fun aTruncatedVideoIsIncompleteNeverAClearance() {
        val whole = clip("whole.mp4", rotation = 0)
        val boxes = TestVideo.boxes(whole)
        val mdat = boxes.first { it.first == "mdat" }
        val moovFirst = boxes.indexOfFirst { it.first == "moov" } < boxes.indexOf(mdat)
        // Cut inside the pictures, so the sample table promises more than the file holds.
        val cut = File(work, "cut.mp4")
        whole.copyTo(cut)
        RandomAccessFile(cut, "rw").use { it.setLength(mdat.second + mdat.third * 6 / 10) }

        when (val probe = env.reader.probe(cut, "video/mp4")) {
            is Probe.Refused -> {
                // A muxer that writes the index last leaves nothing to read: refused, not scanned.
                assertFalse("a cut file with its index first must be read, then found short", moovFirst)
                assertNotEquals("no_clearance", probe.code)
            }
            is Probe.Ready -> {
                assertEquals(FRAMES.toLong(), probe.info.frames)
                val report = scan(cut, probe.info)
                assertEquals(describe(report), Status.INCOMPLETE, report.status)
                assertNotACleanPass(report)
                assertTrue(describe(report), report.framesDecoded < FRAMES)
            }
        }
    }

    // Helpers.

    /** 2.5 s at 10 fps: white, the marker on frames 10 to 21, white again. */
    private fun clip(name: String, rotation: Int): File {
        val spot = DemoScene.videoSpot(SHOWN.first.toLong()..SHOWN.last.toLong())
        val marked = DemoScene.luma(spot)
        val blank = DemoScene.luma(null)
        val turned = rotation == 90
        val pictures = listOf(blank, marked).map { if (turned) DemoScene.storedForQuarterTurn(it) else it }
        val (width, height) = if (turned) DemoScene.HEIGHT to DemoScene.WIDTH else DemoScene.WIDTH to DemoScene.HEIGHT
        val file = File(work, name)
        val written = TestVideo.write(file, width, height, FRAMES, rotation) { i -> if (i in SHOWN) pictures[1] else pictures[0] }
        assertEquals("the encoder dropped or added pictures", FRAMES, written)
        assertEquals("the muxer stored a different count", FRAMES, TestVideo.samples(file))
        return file
    }

    private fun probe(file: File): MediaInfo = when (val probe = env.reader.probe(file, "video/mp4")) {
        is Probe.Ready -> probe.info
        is Probe.Refused -> throw AssertionError("probe refused: ${probe.code}")
    }

    /**
     * The scan as ScanViewModel runs it: the same request, the job, the platform decode, finish.
     * With `stopAfter`, the person taps Stop once that many pictures went in.
     */
    private fun scan(file: File, info: MediaInfo, stopAfter: Int? = null): Report {
        val out = File(work, "result-${UUID.randomUUID()}")
        val job = request(null) {
            put("input", file.path)
            put("bundles", env.paths.bundles.path)
            put("bundle", "fast")
            put("posters", posters.path)
            put("out", out.path)
            put("coverage", "complete")
            put("missing", true)
            put("wanted", true)
            put("accelerate", true)
            put("device", JSONObject().put("kind", "phone").put("thermal", "nominal").put("battery", 1.0).put("charging", true))
            put("media", info.toJson())
        }
        val inner = when (val started = env.core.start(job) { }) {
            is JobStart.Started -> started.job
            is JobStart.Refused -> throw AssertionError("job refused: ${started.answer}")
        }
        val stopping = StoppingJob(inner, stopAfter)
        val clean = try {
            env.reader.decode(file, info, stopping) { stopping.stopped }
        } catch (t: Throwable) {
            false
        }
        return Report.from(stopping.finish(clean), out)
    }

    private fun assertCandidate(report: Report) {
        assertEquals(describe(report), Status.COMPLETE, report.status)
        assertEquals(describe(report), "possible_candidate", report.headline.code)
        assertEquals(describe(report), FRAMES.toLong(), report.framesDecoded)
        assertEquals(describe(report), 1, report.candidates.size)
        val card = report.candidates[0]
        assertEquals("fixture-missing-a", card.poster.id)
        assertTrue(describe(report), card.frameIndex in SHOWN.first.toLong()..SHOWN.last.toLong())
    }

    /** Not complete, so the clearance line is never shown. */
    private fun assertNotACleanPass(report: Report) {
        assertNotEquals("no_clearance", report.headline.code)
        assertFalse(report.disclosure.any { it.code == "no_clearance" })
    }

    private fun describe(report: Report) =
        "${report.status} ${report.headline.code} reason=${report.reason?.code} decoded=${report.framesDecoded} " +
            "candidates=${report.candidates.size} tracks=${report.tracks.map { "${it.status}" }}"

    /** The real job, with a Stop tapped after `stopAfter` pictures went in. */
    private class StoppingJob(private val job: ScanJob, private val stopAfter: Int?) : ScanJob by job {
        @Volatile var stopped = false
            private set
        private var pushed = 0

        private fun counted(kept: Boolean): Boolean {
            pushed++
            if (!stopped && stopAfter != null && pushed >= stopAfter) {
                stopped = true
                job.stop(Stop.CANCEL)
            }
            return kept
        }

        override fun pushYuv(
            index: Long,
            y: ByteBuffer,
            yStride: Int,
            u: ByteBuffer,
            v: ByteBuffer,
            uvStride: Int,
            uvPixelStride: Int,
            width: Int,
            height: Int,
            fullRange: Boolean,
            bt709: Boolean,
            rotation: Int,
        ): Boolean = counted(job.pushYuv(index, y, yStride, u, v, uvStride, uvPixelStride, width, height, fullRange, bt709, rotation))
    }

    private companion object {
        const val FRAMES = 25
        /** 1.2 s. Frames from a multiple of 8 never all pass together; see DemoScene.videoSpot. */
        val SHOWN = 10..21
    }
}
