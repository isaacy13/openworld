// SPDX-License-Identifier: Apache-2.0
package app.openworld

import android.content.Intent
import android.graphics.Bitmap
import org.robolectric.fakes.RoboCursor
import android.net.Uri
import android.provider.OpenableColumns
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.test.core.app.ApplicationProvider
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import java.io.File

/**
 * Phone screens on a Robolectric device, calling the same openworld program the app starts.
 * Fixture markers only. No photograph of a person.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], qualifiers = "w360dp-h800dp")
class PhoneFlowTest {
    @Test
    fun inProcessLibraryAgreesWithTheSceneScan() {
        val required = System.getenv("OPENWORLD_REQUIRE_LINKED") == "1"
        if (!Core.linkedLibrary()) {
            if (required) {
                throw AssertionError("libopenworld_jni did not load")
            }
            return
        }
        val model = drive("scene")
        assertEquals("Possible candidate. Not an identification.", model.summary)
        assertTrue(model.candidateRows.size >= 2)
        assertTrue(model.candidateRows.all { it.url.startsWith("https://www.fbi.gov") })
        model.prepareLeave("https://www.fbi.gov.evil.com/wanted")
        assertNull(model.leavingUrl)
        assertTrue(model.leaveNotice?.contains("FBI page") == true)
        model.prepareLeave(model.candidateRows[0].url)
        assertEquals(model.candidateRows[0].url, model.leavingUrl)
        assertNull(model.leaveNotice)
    }

    @Test
    fun deleteRemovesTheResultAndLeavesAForeignDirectory() {
        val model = drive("scene")
        val result = model.resultDir
        assertNotNull(result)
        val root = result!!.parentFile
        assertTrue(File(result, "result.json").isFile)
        assertTrue(File(root, "openworld-frames").isDirectory)
        model.deleteResult()
        assertEquals("Deleted.", model.summary)
        assertTrue(model.strip.isEmpty())
        assertTrue(model.candidateRows.isEmpty())
        assertNull(model.resultDir)
        assertFalse(result.exists())
        assertFalse(root!!.exists())

        val foreign = File.createTempFile("ow-foreign", "").parentFile!!
        val dir = File(foreign, "ow-foreign-" + System.nanoTime())
        assertTrue(dir.mkdirs())
        File(dir, "notes.txt").writeText("keep")
        try {
            val json = Core.json(listOf("--json", "delete", "--out", dir.absolutePath))
            assertFalse(json.optBoolean("deleted"))
            assertTrue(json.optString("message").contains("not an OpenWorld result"))
        } catch (err: java.io.IOException) {
            assertTrue(err.message?.contains("not an OpenWorld result") == true)
        }
        assertTrue(File(dir, "notes.txt").isFile)
        dir.deleteRecursively()
    }

    @Test
    fun aStoppedScanKeepsAReasonBesidesIncomplete() {
        val file = still("blank")
        val root = File(file.parentFile, "ow-stop-" + System.nanoTime())
        assertTrue(root.mkdirs())
        val posters = File(root, "posters")
        val out = File(root, "result")
        Core.json(listOf("--json", "posters", "write-fixture", "--out", posters.absolutePath))
        val frames = File(root, "frames")
        val reel = PlatformDecode.writeFrames(file, frames)
        val json = Core.json(
            listOf(
                "--json", "--bundles", Core.bundlesDir(), "scan",
                "--input", file.absolutePath,
                "--bundle", "fast",
                "--long-side", "640",
                "--coverage", "complete",
                "--posters", posters.absolutePath,
                "--out", out.absolutePath,
                "--form-factor", "phone",
                "--provider", "cpu",
                "--abort-after-frames", "0",
            ) + reel.arguments(reel.directory)
        )
        assertEquals("incomplete", json.getString("status"))
        assertEquals("Incomplete.", json.getString("summary"))
        val reason = json.getString("message")
        assertEquals("The scan stopped before every selected frame was analyzed.", reason)
        val model = FlowModel()
        model.status = json.getString("status")
        model.summary = json.getString("summary")
        model.incompleteReason = if (reason == model.summary) "" else reason
        assertEquals(reason, model.incompleteReason)
        root.deleteRecursively()
    }

    @Test
    fun sceneShowsACandidateAndAFaceThatWasNotCompared() {
        val model = drive("scene")
        assertEquals("Possible candidate. Not an identification.", model.summary)
        assertTrue(model.detail.contains("Nothing is uploaded."))
        assertTrue(model.detail.contains("Nobody is enrolled."))
        assertTrue(model.detail.contains("OpenWorld does not train on this file."))
        assertTrue(model.detail.contains("OpenWorld does not contact an agency."))
        assertTrue(model.detail.contains("A candidate is not an identification."))
        assertTrue(model.detail.contains("No candidate is not a clearance."))
        assertTrue(model.detail.contains("This file is not authenticated."))
        assertTrue(model.detail.contains("On-device does not mean the file is real."))
        assertTrue(model.detail.contains("Fixture markers were read."))
        assertFalse(model.detail.lineSequence().any { it == "null" })
        val labels = model.strip.map { it.first }
        assertTrue(labels.contains("Possible candidate. Not an identification."))
        assertTrue(labels.contains("Not compared."))
        assertTrue(labels.contains("A vehicle is not a person."))
        assertTrue(model.fbiUrl?.startsWith("https://www.fbi.gov") == true)
        assertNull(model.leavingUrl)
        model.strip.forEach { (_, path) ->
            val crop = File(path)
            assertTrue(crop.isFile)
            assertEquals(0x89.toByte(), crop.inputStream().use { it.read().toByte() })
        }
        val report = scanReport(still("scene"))
        assertTrue(report.getJSONArray("candidates").length() >= 1)
        assertTrue(report.getInt("faces_seen_not_compared") >= 1)
    }

    @Test
    fun blankStillIsAClearance() {
        val model = drive("blank")
        assertEquals("No candidate is not a clearance.", model.summary)
        assertFalse(model.detail.lineSequence().any { it == "null" })
        assertTrue(model.strip.isEmpty())
        assertNull(model.fbiUrl)
        val report = scanReport(still("blank"))
        assertEquals(0, report.getJSONArray("candidates").length())
        assertEquals(0, report.getJSONArray("inventory").length())
    }

    @Test
    fun genuineFaceBelowTheCutoffIsNotACandidate() {
        val model = drive("below")
        assertEquals("No candidate is not a clearance.", model.summary)
        assertNull(model.fbiUrl)
        assertTrue(model.strip.map { it.first }.contains("Below the locked cutoff. Not a candidate."))
        val report = scanReport(still("below"))
        assertEquals(0, report.getJSONArray("candidates").length())
        val comparisons = report.getJSONArray("comparisons")
        assertTrue(comparisons.length() > 0)
        var matchedPoster = false
        for (i in 0 until comparisons.length()) {
            val row = comparisons.getJSONObject(i)
            if (row.isNull("poster_fiducial_id")) continue
            if (row.getInt("fiducial_id") == row.getInt("poster_fiducial_id")) {
                matchedPoster = true
                assertFalse(row.getBoolean("passed"))
            }
        }
        assertTrue(matchedPoster)
    }

    @Test
    fun impostorFaceIsNotACandidate() {
        val model = drive("impostor")
        assertEquals("No candidate is not a clearance.", model.summary)
        assertNull(model.fbiUrl)
        assertFalse(model.strip.map { it.first }.contains("Possible candidate. Not an identification."))
        val report = scanReport(still("impostor"))
        assertEquals(0, report.getJSONArray("candidates").length())
        val comparisons = report.getJSONArray("comparisons")
        assertTrue(comparisons.length() > 0)
        for (i in 0 until comparisons.length()) {
            val row = comparisons.getJSONObject(i)
            assertFalse(row.getBoolean("passed"))
            if (!row.isNull("poster_fiducial_id")) {
                assertFalse(row.getInt("fiducial_id") == row.getInt("poster_fiducial_id"))
            }
        }
    }

    @Test
    fun faceUnder64PixelsIsLeftOut() {
        val model = drive("tiny")
        assertEquals("No candidate is not a clearance.", model.summary)
        assertTrue(model.strip.isEmpty())
        val report = scanReport(still("tiny"))
        assertEquals(0, report.getInt("faces_seen_not_compared"))
        assertEquals(0, report.getJSONArray("inventory").length())
        assertEquals(0, report.getJSONArray("candidates").length())
    }

    @Test
    fun faceAt64PixelsIsSeenAndNotCompared() {
        val model = drive("uncompared")
        assertEquals("No candidate is not a clearance.", model.summary)
        assertNull(model.fbiUrl)
        assertEquals(listOf("Not compared."), model.strip.map { it.first })
        val report = scanReport(still("uncompared"))
        assertTrue(report.getInt("faces_seen_not_compared") >= 1)
        assertEquals(0, report.getJSONArray("candidates").length())
    }

    @Test
    fun phoneEstimateNamesCpuHeatAndBattery() {
        val model = FlowModel()
        choose(model, still("blank"))
        model.continueFromDevice()
        val ids = model.bundleRows.map { it.first }
        assertTrue(ids.contains("fast"))
        assertTrue(ids.contains("accurate"))
        assertEquals("fast", model.bundleId)
        assertTrue(model.bundleRows.any { it.third.contains("Not measured yet.") })
        model.longSide = "640"
        model.coverage = "complete"
        model.continueFromSize()
        assertTrue(model.canAnalyze)
        assertTrue(model.estimateText.contains("second"))
        assertTrue(model.estimateText.contains("This is a planning estimate, not a thermal measurement."))
        assertTrue(model.estimateText.contains("This scan runs on the CPU. It will be slower, warmer, and use more battery."))
        assertTrue(model.estimateText.contains("This phone may get hot."))
        assertTrue(model.estimateText.contains("A long scan uses a lot of battery."))
        assertFalse(model.estimateText.contains("A computer will finish this sooner."))
        assertFalse(model.estimateText.lineSequence().any { it == "null" })
    }

    @Test
    fun measuredCoverageWarnsBeforeAnalyze() {
        val model = FlowModel()
        choose(model, still("blank"))
        model.coverage = "measured"
        model.continueFromSize()
        assertTrue(model.estimateText.contains("A brief face can be missed."))
    }

    @Test
    fun unreadableFileRefusesBeforeAnalyze() {
        val junk = File.createTempFile("openworld-junk", null)
        junk.writeText("this is not a photo")
        val model = FlowModel()
        choose(model, junk)
        model.continueFromSize()
        assertFalse(model.canAnalyze)
        assertTrue(model.estimateText.contains("Refusing"))
    }

    @Test
    fun oldProviderDateWarnsWithoutBlocking() {
        val model = FlowModel()
        val uri = Uri.parse("content://app.openworld/old.png")
        val file = still("blank")
        val resolver = ApplicationProvider.getApplicationContext<android.content.Context>().contentResolver
        val cursor = object : RoboCursor() {
            override fun close() {
                moveToPosition(-1)
            }
        }
        cursor.setColumnNames(listOf(OpenableColumns.DISPLAY_NAME, "last_modified"))
        cursor.setResults(
            arrayOf(arrayOf<Any>("old.png", System.currentTimeMillis() - 40L * 24 * 60 * 60 * 1000))
        )
        shadowOf(resolver).setCursor(uri, cursor)
        shadowOf(resolver).registerInputStream(uri, file.inputStream())
        model.choose(uri, resolver)
        assertEquals("old.png", model.fileName)
        assertTrue(model.oldFile)
        assertEquals(Step.Device, model.step)
    }

    @Test
    fun oldFileWarningIsOnTheResult() {
        val model = FlowModel()
        val uri = Uri.parse("content://app.openworld/old-blank.png")
        val file = still("blank")
        val resolver = ApplicationProvider.getApplicationContext<android.content.Context>().contentResolver
        val cursor = object : RoboCursor() {
            override fun close() {
                moveToPosition(-1)
            }
        }
        cursor.setColumnNames(listOf(OpenableColumns.DISPLAY_NAME, "last_modified"))
        cursor.setResults(
            arrayOf(arrayOf<Any>("old-blank.png", System.currentTimeMillis() - 40L * 24 * 60 * 60 * 1000))
        )
        shadowOf(resolver).setCursor(uri, cursor)
        shadowOf(resolver).registerInputStream(uri, file.inputStream())
        model.choose(uri, resolver)
        assertTrue(model.oldFile)
        model.continueFromDevice()
        model.continueFromSize()
        model.analyze()
        assertEquals("No candidate is not a clearance.", model.summary)
        assertTrue(model.detail.contains("This file is older than about 30 days."))
        assertFalse(model.detail.contains("Open FBI page"))
    }

    @Test
    fun aPngStillIsCopiedByteForByte() {
        val file = still("blank")
        val frames = File(file.parentFile, "ow-frames-" + System.nanoTime())
        val facts = PlatformDecode.writeFrames(file, frames)
        val copied = File(frames, "frame_000000.png").readBytes()
        assertTrue(copied.contentEquals(file.readBytes()))
        assertEquals(1L, facts.frames)
        assertFalse(facts.video)
    }

    @Test
    fun anAnimatedGifKeepsAFaceThatIsNotOnTheFirstFrame() {
        val gif = laterFrameGif()
        val facts = PlatformDecode.facts(gif)
        assertTrue(facts.video)
        assertEquals(3L, facts.frames)
        assertEquals(5.0, facts.fps, 0.05)
        val report = scanReport(gif)
        assertEquals("complete", report.getString("status"))
        assertEquals("Possible candidate. Not an identification.", report.getString("summary"))
        assertEquals(3, report.getInt("frames_decoded"))
        val candidates = report.getJSONArray("candidates")
        assertTrue(candidates.length() > 0)
        for (i in 0 until candidates.length()) {
            assertEquals(1, candidates.getJSONObject(i).getInt("frame_index"))
        }
        val frames = File(gif.parentFile, "ow-gif-frames-" + System.nanoTime())
        val reel = PlatformDecode.writeFrames(gif, frames)
        val only = File(gif.parentFile, "ow-gif-first-" + System.nanoTime())
        assertTrue(only.mkdirs())
        File(frames, "frame_000000.png").copyTo(File(only, "frame_000000.png"))
        val first = scanPrepared(
            gif,
            reel.copy(frames = 1, fps = 0.0, durationSec = 0.0, video = false, directory = only),
        )
        assertEquals("complete", first.getString("status"))
        assertEquals("No candidate is not a clearance.", first.getString("summary"))
        assertEquals(1, first.getInt("frames_decoded"))

        val model = FlowModel()
        val uri = Uri.parse("content://app.openworld/clip.gif")
        val resolver = ApplicationProvider.getApplicationContext<android.content.Context>().contentResolver
        shadowOf(resolver).registerInputStream(uri, gif.inputStream())
        model.choose(uri, resolver)
        model.continueFromDevice()
        model.continueFromBundle()
        model.continueFromSize()
        assertTrue(model.canAnalyze)
        assertFalse(model.estimateText.contains("Refusing"))
        model.analyze()
        assertEquals("Possible candidate. Not an identification.", model.summary)
        assertTrue(model.candidateRows.isNotEmpty())
    }

    @Test
    fun aOneFrameGifStaysOneFrame() {
        val gif = File.createTempFile("ow-one", ".gif")
        ffmpeg("-f", "lavfi", "-i", "color=c=blue:s=64x64", "-frames:v", "1", gif.absolutePath)
        val facts = PlatformDecode.facts(gif)
        assertFalse(facts.video)
        assertEquals(1L, facts.frames)
        assertEquals(0.0, facts.fps, 0.0)
        val report = scanReport(gif)
        assertEquals("No candidate is not a clearance.", report.getString("summary"))
        assertEquals(1, report.getInt("frames_decoded"))
        assertEquals(0, report.getJSONArray("candidates").length())
    }

    @Test
    fun aBrokenGifIsRefused() {
        val gif = File.createTempFile("ow-bad", ".gif")
        gif.writeBytes("GIF89a".encodeToByteArray() + ByteArray(24))
        val model = FlowModel()
        val uri = Uri.parse("content://app.openworld/broken.gif")
        val resolver = ApplicationProvider.getApplicationContext<android.content.Context>().contentResolver
        shadowOf(resolver).registerInputStream(uri, gif.inputStream())
        model.choose(uri, resolver)
        model.continueFromDevice()
        model.continueFromBundle()
        model.continueFromSize()
        assertFalse(model.canAnalyze)
        assertTrue(model.estimateText.contains("Refusing"))
        assertEquals(Step.Estimate, model.step)
    }

    @Test
    fun anAnimatedPngIsRefusedInsteadOfClearingTheFirstFrame() {
        val apng = movingPicture("apng")
        assertTrue(StillMotion.animatedPng(apng))
        assertFalse(StillMotion.animatedPng(still("blank")))
        val model = FlowModel()
        val uri = Uri.parse("content://app.openworld/clip.apng")
        val resolver = ApplicationProvider.getApplicationContext<android.content.Context>().contentResolver
        shadowOf(resolver).registerInputStream(uri, apng.inputStream())
        model.choose(uri, resolver)
        model.continueFromDevice()
        model.continueFromBundle()
        model.continueFromSize()
        assertFalse(model.canAnalyze)
        assertTrue(model.estimateText.contains("not fully decoded"))
        assertTrue(model.estimateText.contains("Refusing"))
        val frames = File(apng.parentFile, "ow-apng-frames-" + System.nanoTime())
        var refused = false
        try {
            PlatformDecode.writeFrames(apng, frames)
        } catch (err: java.io.IOException) {
            refused = true
            assertTrue(err.message?.contains("Refusing") == true)
        }
        assertTrue(refused)
        assertFalse(File(frames, "frame_000000.png").isFile)
    }

    @Test
    fun anAnimatedWebpIsRefusedInsteadOfClearingTheFirstFrame() {
        val webp = movingPicture("webp")
        val stillWebp = File.createTempFile("ow-still", ".webp")
        ffmpeg("-f", "lavfi", "-i", "color=c=blue:s=16x16", "-frames:v", "1", "-c:v", "libwebp", stillWebp.absolutePath)
        assertTrue(StillMotion.animatedWebp(webp))
        assertFalse(StillMotion.animatedWebp(stillWebp))
        val model = FlowModel()
        val uri = Uri.parse("content://app.openworld/clip.webp")
        val resolver = ApplicationProvider.getApplicationContext<android.content.Context>().contentResolver
        shadowOf(resolver).registerInputStream(uri, webp.inputStream())
        model.choose(uri, resolver)
        model.continueFromDevice()
        model.continueFromBundle()
        model.continueFromSize()
        assertFalse(model.canAnalyze)
        assertTrue(model.estimateText.contains("not fully decoded"))
        assertTrue(model.estimateText.contains("Refusing"))
    }

    @Test
    fun orientationSixTurnsTheStoredPixelsUpright() {
        val colors = intArrayOf(
            0xFF010000.toInt(), 0xFF020000.toInt(), 0xFF030000.toInt(),
            0xFF040000.toInt(), 0xFF050000.toInt(), 0xFF060000.toInt(),
        )
        val stored = Bitmap.createBitmap(colors, 3, 2, Bitmap.Config.ARGB_8888)
        val shown = JpegOrientation.apply(stored, 6)
        assertEquals(2, shown.width)
        assertEquals(3, shown.height)
        assertEquals(0xFF040000.toInt(), shown.getPixel(0, 0))
        assertEquals(0xFF010000.toInt(), shown.getPixel(1, 0))
        assertEquals(0xFF050000.toInt(), shown.getPixel(0, 1))
        assertEquals(0xFF020000.toInt(), shown.getPixel(1, 1))
        assertEquals(0xFF060000.toInt(), shown.getPixel(0, 2))
        assertEquals(0xFF030000.toInt(), shown.getPixel(1, 2))
        val same = JpegOrientation.apply(stored, 1)
        assertTrue(same === stored)
        val mirrored = JpegOrientation.apply(stored, 2)
        assertEquals(0xFF030000.toInt(), mirrored.getPixel(0, 0))
        assertEquals(0xFF010000.toInt(), mirrored.getPixel(2, 0))
        val fromTrack = JpegOrientation.apply(stored, JpegOrientation.tagForClockwise(90))
        assertEquals(shown.getPixel(0, 0), fromTrack.getPixel(0, 0))
        assertEquals(shown.getPixel(1, 2), fromTrack.getPixel(1, 2))
        assertEquals(1, JpegOrientation.tagForClockwise(0))
        assertEquals(8, JpegOrientation.tagForClockwise(270))
        assertEquals(6, JpegOrientation.tagForClockwise(-270))
        assertEquals(640 to 480, PlatformDecode.squarePixelSize(320, 480, 2, 1))
        assertEquals(320 to 480, PlatformDecode.squarePixelSize(320, 480, 1, 1))
        assertEquals(640 to 480, PlatformDecode.displayedVideoSize(480, 320, 1, 2, 90))
        assertEquals(240 to 320, PlatformDecode.displayedVideoSize(320, 480, 2, 1, 90))
        assertEquals(640 to 480, PlatformDecode.displayedVideoSize(320, 480, 2, 1, 0))
        assertEquals(640 to 480, PlatformDecode.displayedVideoSize(480, 640, 1, 1, 90))
    }

    @Test
    fun aJpegOrientationTagIsReadWithoutAnExtension() {
        val file = File.createTempFile("ow-orient", "")
        file.writeBytes(jpegWithOrientation(6))
        assertEquals(6, JpegOrientation.tag(file))
        assertEquals(640 to 480, JpegOrientation.displaySize(480, 640, 6))
        assertEquals(1, JpegOrientation.tag(byteArrayOf(0x89.toByte(), 0x50, 0x4E, 0x47)))
    }

    @Test
    fun aWebpOrientationTagIsReadWithoutAnExtension() {
        val direct = File.createTempFile("ow-webp", "")
        direct.writeBytes(webpWithOrientation(6, prefix = false))
        assertEquals(6, JpegOrientation.tag(direct))
        assertEquals(640 to 480, JpegOrientation.displaySize(480, 640, JpegOrientation.tag(direct)))
        val marked = webpWithOrientation(6, prefix = true)
        assertEquals(6, JpegOrientation.tag(marked))
        assertEquals(1, JpegOrientation.tag(webpWithOrientation(1, prefix = false)))
    }

    @Test
    fun aJpegWithCameraOrientationIsScannedAsShown() {
        val scene = still("scene")
        val root = File(scene.parentFile, "ow-orient-" + System.nanoTime())
        check(root.mkdirs())
        val turned = File(root, "turned.jpg")
        ffmpeg("-i", scene.absolutePath, "-vf", "transpose=2", "-q:v", "2", turned.absolutePath)
        val side = File(root, "side")
        side.writeBytes(jpegWithOrientation(6, turned.readBytes()))
        assertEquals(6, JpegOrientation.tag(side))
        val bounds = android.graphics.BitmapFactory.Options().apply { inJustDecodeBounds = true }
        android.graphics.BitmapFactory.decodeFile(turned.absolutePath, bounds)
        val facts = PlatformDecode.facts(side)
        assertEquals(bounds.outHeight, facts.width)
        assertEquals(bounds.outWidth, facts.height)
        val report = scanReport(side)
        assertEquals("complete", report.getString("status"))
        assertEquals("Possible candidate. Not an identification.", report.getString("summary"))
        assertTrue(report.getJSONArray("candidates").length() > 0)
        val upright = File(root, "upright.jpg")
        ffmpeg("-i", scene.absolutePath, "-q:v", "2", upright.absolutePath)
        val plain = scanReport(upright)
        assertEquals("Possible candidate. Not an identification.", plain.getString("summary"))
        val raw = scanReport(turned)
        assertEquals("No candidate is not a clearance.", raw.getString("summary"))
        assertEquals(0, raw.getJSONArray("candidates").length())
    }

    @Test
    fun anInterlacedGifKeepsThePixelsOfThatFrame() {
        val gif = File.createTempFile("ow-inter", ".gif")
        gif.writeBytes(INTERLACED_GIF)
        val reel = GifFrames.read(gif)
        assertEquals(16, reel.width)
        assertEquals(16, reel.height)
        assertEquals(1, reel.frames.size)
        val rgb = reel.frames[0]
        for (y in 0 until 16) {
            for (x in 0 until 16) {
                val pixel = (y * 16 + x) * 3
                val expected = if (x < 8 && y < 8) 0 else 255
                assertEquals(expected, rgb[pixel].toInt() and 0xFF)
                assertEquals(expected, rgb[pixel + 1].toInt() and 0xFF)
                assertEquals(expected, rgb[pixel + 2].toInt() and 0xFF)
            }
        }
    }

    private fun drive(kind: String): FlowModel {
        val model = FlowModel()
        choose(model, still(kind))
        model.continueFromDevice()
        model.bundleId = "fast"
        model.longSide = "640"
        model.coverage = "complete"
        model.continueFromBundle()
        model.continueFromSize()
        assertTrue(model.canAnalyze)
        assertEquals(Step.Estimate, model.step)
        model.analyze()
        assertEquals(Step.Results, model.step)
        return model
    }

    private fun choose(model: FlowModel, file: File) {
        val uri = Uri.parse("content://app.openworld/${file.name}")
        val resolver = ApplicationProvider.getApplicationContext<android.content.Context>().contentResolver
        shadowOf(resolver).registerInputStream(uri, file.inputStream())
        model.choose(uri, resolver)
        assertEquals(Step.Device, model.step)
        assertTrue(model.fileName.isNotBlank())
    }
}

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], qualifiers = "w360dp-h800dp")
class PhoneScreenTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun resultsScreenShowsTheStripAndTheLeavePrompt() {
        val model = FlowModel()
        val uri = Uri.parse("content://app.openworld/scene.png")
        val resolver = ApplicationProvider.getApplicationContext<android.content.Context>().contentResolver
        shadowOf(resolver).registerInputStream(uri, still("scene").inputStream())
        model.choose(uri, resolver)
        compose.setContent { OpenWorldApp(model = model, onChoose = {}, onOpen = {}) }
        compose.onNodeWithText("This file stays on this device.").assertExists()
        compose.onNodeWithText("Nothing is uploaded.").assertExists()
        compose.onNodeWithText("Continue").performClick()
        compose.onNodeWithText("Phones and long video.", substring = true).assertExists()
        compose.onNodeWithText("Not measured yet.").assertExists()
        compose.onNodeWithText("Continue").performClick()
        compose.onNodeWithText("Not compared.", substring = true).assertExists()
        compose.onNodeWithText("Continue").performClick()
        compose.onNodeWithText("This scan runs on the CPU. It will be slower, warmer, and use more battery.", substring = true).assertExists()
        compose.onNodeWithText("This phone may get hot.", substring = true).assertExists()
        compose.onNodeWithText("A long scan uses a lot of battery.", substring = true).assertExists()
        compose.onNodeWithText("Analyze").assertIsEnabled().performClick()
        assertTrue(compose.onAllNodesWithText("Possible candidate. Not an identification.").fetchSemanticsNodes().isNotEmpty())
        compose.onNodeWithText("Not compared.").assertExists()
        compose.onNodeWithText("A vehicle is not a person.").assertExists()
        assertTrue(compose.onAllNodesWithText("No candidate is not a clearance.", substring = true).fetchSemanticsNodes().isNotEmpty())
        compose.onAllNodesWithText("You are leaving OpenWorld.").assertCountEquals(0)
        model.prepareLeave("https://www.fbi.gov.evil.com/wanted")
        compose.onNodeWithText("OpenWorld only opens an FBI page.").assertExists()
        compose.onAllNodesWithText("You are leaving OpenWorld.").assertCountEquals(0)
        compose.onAllNodesWithText("Open").assertCountEquals(0)
        assertTrue(model.candidateRows.size >= 2)
        compose.onAllNodesWithText("Open FBI page").assertCountEquals(model.candidateRows.size)
        compose.onNodeWithText("Fixture subject A", substring = true).assertExists()
        compose.onNodeWithText("Fixture vehicle C", substring = true).assertExists()
        compose.onAllNodesWithText("Open FBI page")[0].performClick()
        compose.onNodeWithText("You are leaving OpenWorld.").assertExists()
        compose.onNodeWithText("Choose another file").assertExists()
        compose.onNodeWithText("Stay").performClick()
        compose.onAllNodesWithText("You are leaving OpenWorld.").assertCountEquals(0)
        compose.onAllNodesWithText("Open FBI page")[1].performScrollTo().performClick()
        compose.onNodeWithText("You are leaving OpenWorld.").assertExists()
        assertEquals(model.candidateRows[1].url, model.leavingUrl)
        assertNotNull(model.fbiUrl)
        assertTrue(model.fbiUrl!!.startsWith("https://www.fbi.gov"))
    }

    @Test
    fun backAndChooseAnotherReturnToTheStart() {
        val model = FlowModel()
        val uri = Uri.parse("content://app.openworld/back.png")
        val resolver = ApplicationProvider.getApplicationContext<android.content.Context>().contentResolver
        shadowOf(resolver).registerInputStream(uri, still("blank").inputStream())
        model.choose(uri, resolver)
        compose.setContent { OpenWorldApp(model = model, onChoose = {}, onOpen = {}) }
        compose.onNodeWithText("Back").performClick()
        compose.onNodeWithText("Choose a photo or video").assertExists()
        shadowOf(resolver).registerInputStream(uri, still("blank").inputStream())
        model.choose(uri, resolver)
        model.continueFromDevice()
        model.continueFromSize()
        model.analyze()
        val result = model.resultDir
        compose.onNodeWithText("Choose another file").performClick()
        assertNotNull(result)
        assertFalse(result!!.exists())
        compose.onNodeWithText("Choose a photo or video").assertExists()
        compose.onAllNodesWithText("Open FBI page").assertCountEquals(0)
        assertEquals(Step.Choose, model.step)
        assertEquals("", model.summary)
    }

    @Test
    fun aFinishedScanWithNoCandidateDoesNotSayIncomplete() {
        val model = FlowModel()
        model.step = Step.Results
        model.status = "complete"
        model.summary = ""
        model.detail = "No candidate is not a clearance."
        compose.setContent { OpenWorldApp(model = model, onChoose = {}, onOpen = {}) }
        assertTrue(compose.onAllNodesWithText("No candidate is not a clearance.", substring = true).fetchSemanticsNodes().isNotEmpty())
        compose.onAllNodesWithText("Incomplete.").assertCountEquals(0)
        compose.onAllNodesWithText("Possible candidate. Not an identification.").assertCountEquals(0)
        compose.onNodeWithText("Choose another file").assertExists()
    }

    @Test
    fun clearanceScreenHasNoFbiButton() {
        val model = FlowModel()
        val uri = Uri.parse("content://app.openworld/blank.png")
        val resolver = ApplicationProvider.getApplicationContext<android.content.Context>().contentResolver
        shadowOf(resolver).registerInputStream(uri, still("blank").inputStream())
        model.choose(uri, resolver)
        model.continueFromDevice()
        model.continueFromSize()
        model.analyze()
        compose.setContent { OpenWorldApp(model = model, onChoose = {}, onOpen = {}) }
        assertTrue(compose.onAllNodesWithText("No candidate is not a clearance.", substring = true).fetchSemanticsNodes().isNotEmpty())
        compose.onAllNodesWithText("Open FBI page").assertCountEquals(0)
        compose.onAllNodesWithText("Possible candidate. Not an identification.").assertCountEquals(0)
    }

    @Test
    fun anIncompleteScanShowsWhyItStopped() {
        val model = FlowModel()
        model.step = Step.Results
        model.status = "incomplete"
        model.summary = "Incomplete."
        model.incompleteReason = "The file was not fully decoded."
        compose.setContent { OpenWorldApp(model = model, onChoose = {}, onOpen = {}) }
        compose.onNodeWithText("Incomplete.").assertExists()
        compose.onNodeWithText("The file was not fully decoded.").assertExists()
        compose.onAllNodesWithText("No candidate is not a clearance.").assertCountEquals(0)
    }

    @Test
    fun deleteRemovesTheResultFromTheScreen() {
        val model = FlowModel()
        val uri = Uri.parse("content://app.openworld/delete.png")
        val resolver = ApplicationProvider.getApplicationContext<android.content.Context>().contentResolver
        shadowOf(resolver).registerInputStream(uri, still("scene").inputStream())
        model.choose(uri, resolver)
        model.continueFromDevice()
        model.continueFromSize()
        model.analyze()
        val result = model.resultDir
        compose.setContent { OpenWorldApp(model = model, onChoose = {}, onOpen = {}) }
        compose.onNodeWithText("Delete").performScrollTo().performClick()
        compose.onNodeWithText("Deleted.").assertExists()
        compose.onAllNodesWithText("Open FBI page").assertCountEquals(0)
        compose.onAllNodesWithText("Delete").assertCountEquals(0)
        assertNotNull(result)
        assertFalse(result!!.exists())
    }
}

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], qualifiers = "w360dp-h800dp")
class PhoneLaunchTest {
    @get:Rule
    val compose = createAndroidComposeRule<MainActivity>()

    @Test
    fun coldStartAsksForAFileTheUserAlreadyHas() {
        compose.onNodeWithText("Choose a photo or video").assertExists()
        compose.onNodeWithText("There is no camera.", substring = true).assertExists()
        compose.onNodeWithText("Choose File").assertExists()
    }

    @Test
    fun shareSheetLandsOnTheDeviceDisclosure() {
        val file = still("blank")
        val uri = Uri.parse("content://app.openworld/shared.png")
        val resolver = ApplicationProvider.getApplicationContext<android.content.Context>().contentResolver
        shadowOf(resolver).registerInputStream(uri, file.inputStream())
        val intent = Intent(Intent.ACTION_SEND).apply {
            type = "image/png"
            putExtra(Intent.EXTRA_STREAM, uri)
        }
        compose.activityRule.scenario.onActivity { activity ->
            val method = MainActivity::class.java.getDeclaredMethod("onNewIntent", Intent::class.java)
            method.isAccessible = true
            method.invoke(activity, intent)
        }
        compose.waitForIdle()
        compose.onNodeWithText("This file stays on this device.").assertExists()
        compose.onNodeWithText("Nothing is uploaded.").assertExists()
        compose.onNodeWithText("shared.png").assertExists()
    }
}

private fun still(kind: String): File {
    val out = File.createTempFile("ow-$kind", ".png")
    val args = mutableListOf(
        Core.binary(), "--json", "--bundles", Core.bundlesDir(),
        "fixture-still", "--out", out.absolutePath,
    )
    when (kind) {
        "scene" -> args.add("--scene")
        "blank" -> args.add("--blank")
        "impostor" -> args.addAll(listOf("--id", "99", "--module", "16", "--x", "16", "--y", "16"))
        "below" -> args.addAll(listOf("--id", "7", "--module", "16", "--below-cutoff"))
        "uncompared" -> args.addAll(listOf("--id", "11", "--module", "8", "--x", "16", "--y", "16"))
        "tiny" -> args.addAll(listOf("--id", "7", "--module", "4", "--x", "16", "--y", "16"))
        else -> error(kind)
    }
    val process = ProcessBuilder(args).redirectErrorStream(true).start()
    val text = process.inputStream.bufferedReader().readText()
    val code = process.waitFor()
    if (code != 0) error(text)
    return out
}

private val INTERLACED_GIF = (
    "47494638376110001000810000ffffff0000000000000000002c00000000100010004008" +
        "2f0003081c281080c18308132a3c4890e0c287101b0e7c28b120c48b182356a4b87161c5" +
        "001c25661c49b2a449830101003b"
    ).chunked(2).map { it.toInt(16).toByte() }.toByteArray()

private fun laterFrameGif(): File {
    val root = File(File.createTempFile("ow-later", "").parentFile, "ow-later-" + System.nanoTime())
    check(root.mkdirs())
    val scene = still("scene")
    val blank = still("blank")
    val header = ByteArray(24)
    scene.inputStream().use { check(it.read(header) == 24) }
    val width = ((header[16].toInt() and 0xFF) shl 24) or
        ((header[17].toInt() and 0xFF) shl 16) or
        ((header[18].toInt() and 0xFF) shl 8) or
        (header[19].toInt() and 0xFF)
    val height = ((header[20].toInt() and 0xFF) shl 24) or
        ((header[21].toInt() and 0xFF) shl 16) or
        ((header[22].toInt() and 0xFF) shl 8) or
        (header[23].toInt() and 0xFF)
    val wide = File(root, "wide.png")
    ffmpeg("-i", blank.absolutePath, "-vf", "scale=$width:$height:flags=neighbor", "-frames:v", "1", wide.absolutePath)
    ffmpeg("-i", wide.absolutePath, "-vf", "drawbox=x=0:y=0:w=1:h=1:color=black:t=fill", File(root, "f0.png").absolutePath)
    scene.copyTo(File(root, "f1.png"), overwrite = true)
    ffmpeg("-i", wide.absolutePath, "-vf", "drawbox=x=20:y=20:w=1:h=1:color=black:t=fill", File(root, "f2.png").absolutePath)
    val palette = File(root, "pal.png")
    ffmpeg(
        "-framerate", "5", "-start_number", "0", "-i", File(root, "f%d.png").absolutePath,
        "-frames:v", "3", "-vf", "palettegen=stats_mode=full:max_colors=8", palette.absolutePath,
    )
    val gif = File(root, "later.gif")
    ffmpeg(
        "-framerate", "5", "-start_number", "0", "-i", File(root, "f%d.png").absolutePath,
        "-i", palette.absolutePath, "-frames:v", "3", "-lavfi", "paletteuse=dither=none",
        "-loop", "0", gif.absolutePath,
    )
    return gif
}

private fun movingPicture(kind: String): File {
    val root = File(File.createTempFile("ow-move", "").parentFile, "ow-move-" + System.nanoTime())
    check(root.mkdirs())
    ffmpeg(
        "-f", "lavfi", "-i", "testsrc2=size=32x32:rate=5:duration=0.6",
        "-start_number", "0", File(root, "f%d.png").absolutePath,
    )
    val out = File(root, "move.$kind")
    if (kind == "apng") {
        ffmpeg(
            "-framerate", "5", "-start_number", "0", "-i", File(root, "f%d.png").absolutePath,
            "-frames:v", "3", "-plays", "1", "-f", "apng", out.absolutePath,
        )
    } else {
        ffmpeg(
            "-framerate", "5", "-start_number", "0", "-i", File(root, "f%d.png").absolutePath,
            "-frames:v", "3", "-loop", "0", "-c:v", "libwebp", out.absolutePath,
        )
    }
    return out
}

private fun webpWithOrientation(tag: Int, prefix: Boolean): ByteArray {
    val tiff = byteArrayOf(
        0x4D, 0x4D, 0x00, 0x2A,
        0x00, 0x00, 0x00, 0x08,
        0x00, 0x01,
        0x01, 0x12,
        0x00, 0x03,
        0x00, 0x00, 0x00, 0x01,
        0x00, tag.toByte(),
        0x00, 0x00,
    )
    val exif = if (prefix) byteArrayOf(0x45, 0x78, 0x69, 0x66, 0x00, 0x00) + tiff else tiff
    val vp8x = byteArrayOf(0x08, 0, 0, 0, 0xDF.toByte(), 0x01, 0x00, 0x7F, 0x02, 0x00)
    val body = "WEBP".encodeToByteArray() + riffChunk("VP8X", vp8x) + riffChunk("EXIF", exif)
    return "RIFF".encodeToByteArray() + le32(body.size) + body
}

private fun riffChunk(tag: String, payload: ByteArray): ByteArray {
    val out = tag.encodeToByteArray() + le32(payload.size) + payload
    return if (payload.size % 2 == 1) out + byteArrayOf(0) else out
}

private fun le32(value: Int): ByteArray {
    return byteArrayOf(
        value.toByte(),
        (value shr 8).toByte(),
        (value shr 16).toByte(),
        (value shr 24).toByte(),
    )
}

private fun jpegWithOrientation(tag: Int, jpeg: ByteArray = byteArrayOf(0xFF.toByte(), 0xD8.toByte(), 0xFF.toByte(), 0xD9.toByte())): ByteArray {
    val tiff = byteArrayOf(
        0x49, 0x49, 0x2A, 0x00,
        0x08, 0x00, 0x00, 0x00,
        0x01, 0x00,
        0x12, 0x01,
        0x03, 0x00,
        0x01, 0x00, 0x00, 0x00,
        tag.toByte(), 0x00,
        0x00, 0x00,
        0x00, 0x00, 0x00, 0x00,
    )
    val payload = byteArrayOf(0x45, 0x78, 0x69, 0x66, 0x00, 0x00) + tiff
    val length = payload.size + 2
    val head = byteArrayOf(
        0xFF.toByte(), 0xD8.toByte(), 0xFF.toByte(), 0xE1.toByte(),
        (length shr 8).toByte(), length.toByte(),
    ) + payload
    return head + jpeg.copyOfRange(2, jpeg.size)
}

private fun ffmpeg(vararg args: String) {
    val process = ProcessBuilder(listOf("ffmpeg", "-y", "-v", "error") + args).redirectErrorStream(true).start()
    val text = process.inputStream.bufferedReader().readText()
    if (process.waitFor() != 0) error(text.ifBlank { "ffmpeg failed" })
}

private fun scanPrepared(file: File, reel: PlatformDecode.Facts): JSONObject {
    val root = File(file.parentFile, "ow-prep-" + System.nanoTime())
    check(root.mkdirs())
    val posters = File(root, "posters")
    val out = File(root, "result")
    Core.json(listOf("--json", "posters", "write-fixture", "--out", posters.absolutePath))
    return Core.json(
        listOf(
            "--json", "--bundles", Core.bundlesDir(), "scan",
            "--input", file.absolutePath,
            "--bundle", "fast",
            "--long-side", "640",
            "--coverage", "complete",
            "--posters", posters.absolutePath,
            "--out", out.absolutePath,
            "--form-factor", "phone",
            "--provider", "cpu",
        ) + reel.arguments(reel.directory)
    )
}

private fun scanReport(file: File): JSONObject {
    val root = File.createTempFile("ow-scan", null).parentFile!!
    val dir = File(root, "openworld-scan-" + System.nanoTime())
    dir.mkdirs()
    val posters = File(dir, "posters")
    val frames = File(dir, "frames")
    val out = File(dir, "result")
    Core.json(listOf("--json", "posters", "write-fixture", "--out", posters.absolutePath))
    val reel = PlatformDecode.writeFrames(file, frames)
    return Core.json(
        listOf(
            "--json", "--bundles", Core.bundlesDir(), "scan",
            "--input", file.absolutePath,
            "--bundle", "fast",
            "--long-side", "640",
            "--coverage", "complete",
            "--posters", posters.absolutePath,
            "--out", out.absolutePath,
            "--form-factor", "phone",
            "--provider", "cpu",
        ) + reel.arguments(reel.directory)
    )
}
