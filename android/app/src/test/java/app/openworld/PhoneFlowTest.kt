// SPDX-License-Identifier: Apache-2.0
package app.openworld

import android.content.ContentProvider
import android.content.ContentValues
import android.content.Intent
import android.database.MatrixCursor
import android.graphics.Bitmap
import android.os.ParcelFileDescriptor
import org.robolectric.fakes.RoboCursor
import android.net.Uri
import android.provider.OpenableColumns
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
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
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.robolectric.shadows.ShadowContentResolver
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
        val disclosure = json.getJSONArray("disclosure")
        val lines = (0 until disclosure.length()).map { disclosure.getString(it) }
        assertTrue(lines.contains("Nothing is uploaded."))
        assertFalse(lines.contains("No candidate is not a clearance."))
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
        assertTrue(model.context.contains("Fixture markers were read."))
        assertTrue(model.context.contains("Missing and wanted."))
        assertFalse(model.detail.contains("Missing and wanted."))
        assertFalse(model.detail.lineSequence().any { it == "null" })
        val labels = model.strip.map { it.label }
        assertTrue(model.strip.all { it.frameLabel == "Frame 1." })
        assertTrue(labels.contains("Possible candidate. Not an identification."))
        assertTrue(labels.contains("Not compared."))
        assertTrue(labels.contains("A vehicle is not a person."))
        assertTrue(model.fbiUrl?.startsWith("https://www.fbi.gov") == true)
        assertNull(model.leavingUrl)
        model.strip.forEach { crop ->
            val cropFile = File(crop.path)
            assertTrue(cropFile.isFile)
            assertEquals(0x89.toByte(), cropFile.inputStream().use { it.read().toByte() })
        }
        val report = scanReport(still("scene"))
        assertTrue(report.getJSONArray("candidates").length() >= 1)
        assertTrue(report.getInt("faces_seen_not_compared") >= 1)
    }

    @Test
    fun blankStillIsAClearance() {
        val model = drive("blank")
        assertEquals("No candidate is not a clearance.", model.summary)
        assertTrue(model.context.contains("Missing and wanted."))
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
        assertTrue(model.strip.map { it.label }.contains("Below the locked cutoff. Not a candidate."))
        assertTrue(model.strip.all { it.frameLabel == "Frame 1." })
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
        assertFalse(model.strip.map { it.label }.contains("Possible candidate. Not an identification."))
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
        assertEquals(listOf("Not compared."), model.strip.map { it.label })
        assertEquals(listOf("Frame 1."), model.strip.map { it.frameLabel })
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
        assertEquals("fast", ids.first())
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
        assertTrue(model.estimateText.contains("Fast. 640 px on the long side. Every decoded frame."))
        assertTrue(model.estimateText.contains("Missing and wanted."))
        assertFalse(model.estimateText.contains("Bundle fast"))
        assertFalse(model.estimateText.contains("Coverage complete"))
        model.includeWanted = false
        model.refreshClassLine()
        assertTrue(model.canAnalyze)
        assertEquals(listOf("--no-wanted"), model.classArgs())
        assertTrue(model.estimateText.contains("Missing."))
        model.includeMissing = false
        model.refreshClassLine()
        assertFalse(model.canAnalyze)
        assertTrue(model.estimateText.contains("Choose missing, wanted, or both."))
        model.includeMissing = true
        model.includeWanted = true
        model.refreshClassLine()
        assertTrue(model.canAnalyze)
        assertTrue(model.classArgs().isEmpty())
    }

    @Test
    fun returningFromTheFilePageKeepsTheChosenBundle() {
        val model = FlowModel()
        choose(model, still("blank"))
        model.continueFromDevice()
        assertEquals("fast", model.bundleId)
        model.bundleId = "accurate"
        model.back()
        assertEquals(Step.Device, model.step)
        model.continueFromDevice()
        assertEquals("accurate", model.bundleId)
        assertEquals(Step.Bundle, model.step)
        model.bundleId = "not-in-the-catalog"
        model.continueFromDevice()
        assertEquals("fast", model.bundleId)
        model.bundleId = "accurate"
        model.chooseAnother()
        assertEquals("fast", model.bundleId)
        assertEquals(Step.Choose, model.step)
        model.continueFromDevice()
        assertEquals("fast", model.bundleId)
    }

    @Test
    fun measuredCoverageWarnsBeforeAnalyze() {
        val model = FlowModel()
        choose(model, still("blank"))
        model.coverage = "measured"
        model.continueFromDevice()
        model.continueFromSize()
        val choice = model.estimateText.indexOf("Fast. 640 px on the long side. 5 frames a second, plus the tracker.")
        val brief = model.estimateText.indexOf("A brief face can be missed.")
        val classes = model.estimateText.indexOf("Missing and wanted.")
        assertTrue(choice >= 0 && brief > choice && classes > brief)
    }

    @Test
    fun unreadableFileRefusesBeforeAnalyze() {
        val junk = File.createTempFile("openworld-junk", null)
        junk.writeText("this is not a photo")
        val model = FlowModel()
        choose(model, junk)
        model.continueFromSize()
        assertFalse(model.estimateOk)
        assertFalse(model.canAnalyze)
        assertTrue(model.estimateText.contains("Refusing"))
        model.includeMissing = false
        model.includeWanted = false
        model.refreshClassLine()
        assertFalse(model.canAnalyze)
        assertFalse(model.estimateText.contains("Choose missing, wanted, or both."))
    }

    @Test
    fun aLaterRefusalDoesNotBringAnalyzeBack() {
        val model = FlowModel()
        choose(model, still("blank"))
        model.continueFromDevice()
        model.continueFromSize()
        assertTrue(model.estimateOk)
        assertTrue(model.canAnalyze)
        val junk = File.createTempFile("openworld-junk", null)
        junk.writeText("this is not a photo")
        choose(model, junk)
        model.continueFromSize()
        assertFalse(model.estimateOk)
        assertFalse(model.canAnalyze)
        model.includeMissing = true
        model.includeWanted = true
        model.refreshClassLine()
        assertFalse(model.canAnalyze)
        assertFalse(model.estimateText.contains("Missing and wanted."))
        assertTrue(model.estimateText.contains("Refusing"))
    }

    @Test
    @Config(sdk = [34], qualifiers = "w360dp-h800dp", shadows = [SharedFileOsShadow::class])
    fun aSharedFileWithoutADateColumnKeepsTheFileTime() {
        val fresh = chooseSharedFile("fresh.png", System.currentTimeMillis())
        assertFalse(fresh.oldFile)
        assertEquals("fresh.png", fresh.fileName)
        assertEquals(Step.Device, fresh.step)
        val old = chooseSharedFile("old-share.png", System.currentTimeMillis() - 40L * 24 * 60 * 60 * 1000)
        assertTrue(old.oldFile)
        assertEquals("old-share.png", old.fileName)
        assertEquals(Step.Device, old.step)
        val dated = chooseSharedFile(
            "dated.png",
            System.currentTimeMillis() - 40L * 24 * 60 * 60 * 1000,
            System.currentTimeMillis(),
        )
        assertFalse(dated.oldFile)
        assertEquals(Step.Device, dated.step)
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
        assertTrue(model.warnings.contains("This file is older than about 30 days."))
        assertFalse(model.context.contains("This file is older than about 30 days."))
        assertTrue(model.context.contains("Missing and wanted."))
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
            assertEquals("Frame 2.", candidates.getJSONObject(i).getString("frame_label"))
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
        assertTrue(model.candidateRows.all { it.frameLabel == "Frame 2." })
        assertTrue(model.strip.isNotEmpty())
        assertTrue(model.strip.all { it.frameLabel == "Frame 2." })
        assertTrue(model.context.startsWith("3 frames analyzed."))
    }

    @Test
    fun aPhoneScanReadsTheContainerClock() {
        val movie = File.createTempFile("ow-clock", ".mp4")
        ffmpeg(
            "-f", "lavfi", "-i", "color=c=blue:s=64x64",
            "-frames:v", "1", "-r", "1", "-an", "-c:v", "libx264",
            "-metadata", "creation_time=2020-01-01T00:00:00Z",
            movie.absolutePath,
        )
        val plain = File.createTempFile("ow-plain", ".mp4")
        ffmpeg(
            "-f", "lavfi", "-i", "color=c=blue:s=64x64",
            "-frames:v", "1", "-r", "1", "-an", "-c:v", "libx264",
            plain.absolutePath,
        )
        val frames = File(movie.parentFile, "ow-clock-frames-" + System.nanoTime())
        assertTrue(frames.mkdirs())
        still("blank").copyTo(File(frames, "frame_000000.png"))
        val reel = PlatformDecode.Facts(64, 64, 1.0, 1, 1.0, true, frames)
        assertFalse(reel.arguments(frames).contains("--container-unix"))
        val warned = scanPrepared(movie, reel)
        assertEquals("complete", warned.getString("status"))
        val warnings = warned.getJSONArray("warnings")
        var found = false
        for (i in 0 until warnings.length()) {
            if (warnings.getString(i) == "The file timestamps disagree.") found = true
        }
        assertTrue(found)
        val quiet = scanPrepared(plain, reel)
        val quietWarnings = quiet.getJSONArray("warnings")
        for (i in 0 until quietWarnings.length()) {
            assertNotEquals("The file timestamps disagree.", quietWarnings.getString(i))
        }
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
    fun aPngOrientationTagIsReadWithoutAnExtension() {
        val file = File.createTempFile("ow-png", "")
        file.writeBytes(pngWithOrientation(6))
        assertEquals(6, JpegOrientation.tag(file))
        assertEquals(640 to 480, JpegOrientation.displaySize(480, 640, JpegOrientation.tag(file)))
        assertEquals(1, JpegOrientation.tag(pngWithOrientation(1)))
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
    fun aTiffOrientationTagIsReadWithoutAnExtension() {
        val file = File.createTempFile("ow-tif", "")
        file.writeBytes(
            rgbTiff(
                listOf(
                    TiffPage(1, 1, byteArrayOf(1, 2, 3), 6),
                    TiffPage(1, 1, byteArrayOf(4, 5, 6), 8),
                ),
            ),
        )
        assertEquals(6, JpegOrientation.tag(file))
        assertEquals(listOf(6, 8), JpegOrientation.tiffPageTags(file.readBytes()))
        assertEquals(640 to 480, JpegOrientation.displaySize(480, 640, 6))
        assertEquals(1, JpegOrientation.tag(rgbTiff(listOf(TiffPage(1, 1, byteArrayOf(9, 9, 9), 1)))))
        val big = byteArrayOf(
            0x4D, 0x4D, 0x00, 0x2A,
            0x00, 0x00, 0x00, 0x08,
            0x00, 0x01,
            0x01, 0x12,
            0x00, 0x03,
            0x00, 0x00, 0x00, 0x01,
            0x00, 0x06,
            0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
        )
        assertEquals(listOf(6), JpegOrientation.tiffPageTags(big))
        assertNull(JpegOrientation.tiffPageTags(byteArrayOf(0x89.toByte(), 0x50, 0x4E, 0x47)))
    }

    @Test
    fun aMultiPageTiffIsRefusedInsteadOfScanningTheFirstPage() {
        val tiff = File.createTempFile("ow-pages", "")
        tiff.writeBytes(
            rgbTiff(
                listOf(
                    TiffPage(1, 1, byteArrayOf(0, 0, 0), 1),
                    TiffPage(1, 1, byteArrayOf(1, 2, 3), 6),
                ),
            ),
        )
        var refused = false
        try {
            PlatformDecode.facts(tiff)
        } catch (err: java.io.IOException) {
            refused = true
            assertTrue(err.message?.contains("not fully decoded") == true)
            assertTrue(err.message?.contains("Refusing") == true)
        }
        assertTrue(refused)
        val frames = File(tiff.parentFile, "ow-tif-frames-" + System.nanoTime())
        refused = false
        try {
            PlatformDecode.writeFrames(tiff, frames)
        } catch (err: java.io.IOException) {
            refused = true
            assertTrue(err.message?.contains("Refusing") == true)
        }
        assertTrue(refused)
        assertFalse(File(frames, "frame_000000.png").isFile)
        val model = FlowModel()
        val uri = Uri.parse("content://app.openworld/pages.tif")
        val resolver = ApplicationProvider.getApplicationContext<android.content.Context>().contentResolver
        shadowOf(resolver).registerInputStream(uri, tiff.inputStream())
        model.choose(uri, resolver)
        model.continueFromDevice()
        model.continueFromBundle()
        model.continueFromSize()
        assertFalse(model.canAnalyze)
        assertTrue(model.estimateText.contains("not fully decoded"))
        assertTrue(model.estimateText.contains("Refusing"))
    }

    @Test
    fun aSinglePageTiffIsRefused() {
        val tiff = File.createTempFile("ow-one", "")
        tiff.writeBytes(rgbTiff(listOf(TiffPage(1, 1, byteArrayOf(1, 2, 3), 6))))
        var refused = false
        try {
            PlatformDecode.facts(tiff)
        } catch (err: java.io.IOException) {
            refused = true
            assertTrue(err.message?.contains("Bad codec") == true)
            assertTrue(err.message?.contains("Refusing") == true)
        }
        assertTrue(refused)
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
    fun aJpegNamedPngIsDecodedAsAJpeg() {
        val scene = still("scene")
        val root = File(scene.parentFile, "ow-misname-" + System.nanoTime())
        check(root.mkdirs())
        val jpeg = File(root, "photo.jpg")
        ffmpeg("-i", scene.absolutePath, "-q:v", "2", jpeg.absolutePath)
        val named = File(root, "photo.png")
        jpeg.copyTo(named)
        val source = named.readBytes()
        assertEquals(0xFF.toByte(), source[0])
        assertEquals(0xD8.toByte(), source[1])
        val frames = File(root, "frames")
        PlatformDecode.writeFrames(named, frames)
        val written = File(frames, "frame_000000.png").readBytes()
        assertEquals(0x89.toByte(), written[0])
        assertEquals(0x50.toByte(), written[1])
        assertFalse(written.contentEquals(source))
        val report = scanReport(named)
        assertEquals("complete", report.getString("status"))
        assertEquals("Possible candidate. Not an identification.", report.getString("summary"))
        assertTrue(report.getJSONArray("candidates").length() > 0)
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

    @Test
    fun anUnreadableImportStaysOnChoose() {
        val model = FlowModel()
        val uri = Uri.parse("content://app.openworld/missing.png")
        val resolver = ApplicationProvider.getApplicationContext<android.content.Context>().contentResolver
        model.choose(uri, resolver)
        assertEquals(Step.Choose, model.step)
        assertEquals("", model.fileName)
        assertEquals("The file could not be read. Refusing.", model.pickNotice)
        choose(model, still("blank"))
        assertNull(model.pickNotice)
        assertEquals(Step.Device, model.step)
        val kept = model.fileName
        model.choose(Uri.parse("content://app.openworld/also-missing.png"), resolver)
        assertEquals(Step.Device, model.step)
        assertEquals(kept, model.fileName)
        assertEquals("The file could not be read. Refusing.", model.pickNotice)
    }

    @Test
    fun chooseAnotherRemovesTheImportedCopy() {
        val before = openworldTempFiles()
        val model = FlowModel()
        choose(model, still("blank"))
        val first = openworldTempFiles() - before
        assertEquals(1, first.size)
        val copy = first.first()
        assertTrue(copy.length() > 0)
        val resolver = ApplicationProvider.getApplicationContext<android.content.Context>().contentResolver
        model.choose(Uri.parse("content://app.openworld/also-missing.png"), resolver)
        assertTrue(copy.exists())
        choose(model, still("blank"))
        assertFalse(copy.exists())
        val replacement = (openworldTempFiles() - before).single()
        model.continueFromDevice()
        model.bundleId = "fast"
        model.longSide = "640"
        model.coverage = "complete"
        model.continueFromBundle()
        model.continueFromSize()
        model.analyze()
        assertEquals(Step.Results, model.step)
        assertEquals("No candidate is not a clearance.", model.summary)
        assertEquals(setOf(replacement), openworldTempFiles() - before)
        model.deleteResult()
        assertEquals("Deleted.", model.summary)
        assertTrue(replacement.exists())
        model.analyze()
        assertEquals(Step.Results, model.step)
        assertEquals("No candidate is not a clearance.", model.summary)
        model.chooseAnother()
        assertFalse(replacement.exists())
        assertTrue((openworldTempFiles() - before).isEmpty())
    }

    private fun openworldTempFiles(): Set<File> {
        val dir = System.getProperty("java.io.tmpdir") ?: return emptySet()
        return File(dir).listFiles()?.filter { it.isFile && it.name.startsWith("openworld") }?.toSet() ?: emptySet()
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

    private fun chooseSharedFile(name: String, modified: Long, columnMillis: Long? = null): FlowModel {
        val source = still("blank")
        assertTrue(source.setLastModified(modified))
        val uri = Uri.parse("content://app.openworld.files/$name")
        val provider = object : ContentProvider() {
            override fun onCreate() = true
            override fun query(
                uri: Uri,
                projection: Array<out String>?,
                selection: String?,
                selectionArgs: Array<out String>?,
                sortOrder: String?,
            ): android.database.Cursor {
                val names = if (columnMillis == null) {
                    arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE)
                } else {
                    arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE, "last_modified")
                }
                val cursor = MatrixCursor(names)
                if (columnMillis == null) {
                    cursor.addRow(arrayOf(name, source.length()))
                } else {
                    cursor.addRow(arrayOf(name, source.length(), columnMillis))
                }
                return cursor
            }
            override fun getType(uri: Uri) = "image/png"
            override fun insert(uri: Uri, values: ContentValues?) = null
            override fun delete(uri: Uri, selection: String?, selectionArgs: Array<out String>?) = 0
            override fun update(uri: Uri, values: ContentValues?, selection: String?, selectionArgs: Array<out String>?) = 0
            override fun openFile(uri: Uri, mode: String): ParcelFileDescriptor {
                SharedFileOsShadow.opened = source
                return ParcelFileDescriptor.open(source, ParcelFileDescriptor.MODE_READ_ONLY)
            }
        }
        val info = android.content.pm.ProviderInfo()
        info.authority = "app.openworld.files"
        info.exported = true
        val context = ApplicationProvider.getApplicationContext<android.content.Context>()
        provider.attachInfo(context, info)
        ShadowContentResolver.registerProviderInternal("app.openworld.files", provider)
        val resolver = context.contentResolver
        shadowOf(resolver).registerInputStream(uri, source.inputStream())
        val model = FlowModel()
        model.choose(uri, resolver)
        return model
    }

    @Test
    fun aCatalogWithoutFastStaysTheCatalog() {
        val empty = File(System.getProperty("java.io.tmpdir"), "openworld-empty-" + System.nanoTime())
        assertTrue(empty.mkdirs())
        try {
            assertEquals(empty.absolutePath, Core.bundlesDir(empty.absolutePath))
            assertFalse(File(empty, "fast/manifest.toml").isFile)
        } finally {
            empty.delete()
        }
        assertNotEquals("   ", Core.bundlesDir("   "))
        val named = System.getenv("OPENWORLD_BUNDLES")
        if (!named.isNullOrBlank()) {
            assertEquals(named, Core.bundlesDir())
        }
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
    fun aCropAppearsOnTheEstimateWhileScanning() {
        val model = FlowModel()
        model.progressSeen = java.util.concurrent.CountDownLatch(1)
        model.progressGate = java.util.concurrent.CountDownLatch(1)
        val uri = Uri.parse("content://app.openworld/scene-progress.png")
        val resolver = ApplicationProvider.getApplicationContext<android.content.Context>().contentResolver
        shadowOf(resolver).registerInputStream(uri, still("scene").inputStream())
        model.choose(uri, resolver)
        compose.setContent { OpenWorldApp(model = model, onChoose = {}, onOpen = {}) }
        compose.onNodeWithText("Continue").performClick()
        compose.onNodeWithText("Continue").performClick()
        compose.onNodeWithText("Continue").performClick()
        compose.onNodeWithText("Analyze").performClick()
        val deadline = System.currentTimeMillis() + 30_000
        while (System.currentTimeMillis() < deadline && model.liveCrops.isEmpty()) {
            compose.waitForIdle()
            Thread.sleep(20)
        }
        assertTrue(model.liveCrops.isNotEmpty())
        assertEquals(Step.Estimate, model.step)
        assertTrue(model.scanning)
        val label = model.liveCrops.first().label
        assertTrue(model.liveCrops.all { it.frameLabel == "Frame 1." })
        assertTrue(
            label,
            label == "Possible candidate. Not an identification." ||
                label == "Not compared." ||
                label == "A vehicle is not a person." ||
                label == "This plate text is not published on a poster." ||
                label == "The plate could not be read." ||
                label == "No poster publishes a plate." ||
                label == "Below the locked cutoff. Not a candidate." ||
                label == "This face could not be scored.",
        )
        compose.onAllNodesWithText("Scanning").assertCountEquals(2)
        compose.onNodeWithText("Crops from this file.").assertExists()
        compose.onNodeWithText("Frame 1.").assertExists()
        compose.onNodeWithText(label).assertExists()
        compose.onNodeWithText("Back").assertIsNotEnabled()
        model.back()
        assertEquals(Step.Estimate, model.step)
        model.progressGate?.countDown()
        waitForScan(compose, model)
        compose.onAllNodesWithText("Scanning").assertCountEquals(0)
        assertEquals("Possible candidate. Not an identification.", model.summary)
        assertTrue(compose.onAllNodesWithText("Possible candidate. Not an identification.").fetchSemanticsNodes().size >= 2)
    }

    @Test
    fun analyzeSaysScanningUntilTheResultIsReady() {
        val model = FlowModel()
        val gate = java.util.concurrent.CountDownLatch(1)
        model.scanGate = gate
        val uri = Uri.parse("content://app.openworld/blank-scan.png")
        val resolver = ApplicationProvider.getApplicationContext<android.content.Context>().contentResolver
        shadowOf(resolver).registerInputStream(uri, still("blank").inputStream())
        model.choose(uri, resolver)
        compose.setContent { OpenWorldApp(model = model, onChoose = {}, onOpen = {}) }
        compose.onNodeWithText("Continue").performClick()
        compose.onNodeWithText("Continue").performClick()
        compose.onNodeWithText("Continue").performClick()
        compose.onNodeWithText("Analyze").performClick()
        compose.onAllNodesWithText("Scanning").assertCountEquals(2)
        compose.onNodeWithText("Back").assertIsNotEnabled()
        model.back()
        assertEquals(Step.Estimate, model.step)
        gate.countDown()
        waitForScan(compose, model)
        compose.onAllNodesWithText("Scanning").assertCountEquals(0)
        compose.onNodeWithText("No candidate is not a clearance.").assertExists()
    }

    @Test
    fun bothClassesOffKeepsAnalyzeOnThePage() {
        val model = FlowModel()
        val uri = Uri.parse("content://app.openworld/blank-classes.png")
        val resolver = ApplicationProvider.getApplicationContext<android.content.Context>().contentResolver
        shadowOf(resolver).registerInputStream(uri, still("blank").inputStream())
        model.choose(uri, resolver)
        compose.setContent { OpenWorldApp(model = model, onChoose = {}, onOpen = {}) }
        compose.onNodeWithText("Continue").performClick()
        compose.onNodeWithText("Continue").performClick()
        compose.onNodeWithText("Continue").performClick()
        compose.onNodeWithText("Analyze").assertIsEnabled()
        model.includeMissing = false
        model.includeWanted = false
        model.refreshClassLine()
        compose.onNodeWithText("Analyze").assertIsNotEnabled()
        compose.onNodeWithText("Choose missing, wanted, or both.", substring = true).assertExists()
        compose.onNodeWithText("Missing").assertExists()
        compose.onNodeWithText("Wanted").assertExists()
        compose.onAllNodesWithText("Missing.").assertCountEquals(0)
        compose.onAllNodesWithText("Wanted.").assertCountEquals(0)
    }

    @Test
    fun anEmptyCatalogNamesTheMissingProgram() {
        val model = FlowModel()
        model.step = Step.Bundle
        model.bundleRows = emptyList()
        compose.setContent { OpenWorldApp(model = model, onChoose = {}, onOpen = {}) }
        compose.onAllNodesWithText("Model bundle").assertCountEquals(0)
        compose.onAllNodesWithText("Scores are not comparable across bundles. Results name the bundle you pick.").assertCountEquals(0)
        compose.onAllNodesWithText("The scan program is not on this device. Refusing.").assertCountEquals(1)
        compose.onNodeWithText("Continue").assertIsNotEnabled()
        model.continueFromBundle()
        assertEquals(Step.Bundle, model.step)
    }

    @Test
    fun anUnreadableCatalogNamesTheCatalog() {
        val model = FlowModel()
        model.applyBundlePayload(
            JSONObject(
                """{"status":"refused","summary":"The bundle catalog could not be read. Refusing.","message":"The bundle catalog could not be read. Refusing."}"""
            )
        )
        model.step = Step.Bundle
        compose.setContent { OpenWorldApp(model = model, onChoose = {}, onOpen = {}) }
        compose.onAllNodesWithText("Model bundle").assertCountEquals(0)
        compose.onAllNodesWithText("Scores are not comparable across bundles. Results name the bundle you pick.").assertCountEquals(0)
        compose.onAllNodesWithText("The bundle catalog could not be read. Refusing.").assertCountEquals(1)
        compose.onAllNodesWithText("The scan program is not on this device. Refusing.").assertCountEquals(0)
        compose.onNodeWithText("Continue").assertIsNotEnabled()
        model.continueFromBundle()
        assertEquals(Step.Bundle, model.step)
    }

    @Test
    fun anOldFileStaysAWarningOnTheEstimate() {
        val model = FlowModel()
        val uri = Uri.parse("content://app.openworld/old-estimate.png")
        val resolver = ApplicationProvider.getApplicationContext<android.content.Context>().contentResolver
        val cursor = object : RoboCursor() {
            override fun close() {
                moveToPosition(-1)
            }
        }
        cursor.setColumnNames(listOf(OpenableColumns.DISPLAY_NAME, "last_modified"))
        cursor.setResults(
            arrayOf(arrayOf<Any>("old-estimate.png", System.currentTimeMillis() - 40L * 24 * 60 * 60 * 1000))
        )
        shadowOf(resolver).setCursor(uri, cursor)
        shadowOf(resolver).registerInputStream(uri, still("blank").inputStream())
        model.choose(uri, resolver)
        compose.setContent { OpenWorldApp(model = model, onChoose = {}, onOpen = {}) }
        compose.onNodeWithText("This file is older than about 30 days.").assertExists()
        compose.onNodeWithText("Continue").performClick()
        compose.onNodeWithText("Continue").performClick()
        compose.onNodeWithText("Continue").performClick()
        compose.onNodeWithText("This file is older than about 30 days.").assertExists()
        compose.onNodeWithText("Analyze").assertIsEnabled()
    }

    @Test
    fun theClassLineSitsAboveTheCandidate() {
        val model = FlowModel()
        model.step = Step.Results
        model.status = "complete"
        model.summary = "Possible candidate. Not an identification."
        model.context = "Missing and wanted."
        model.detail = "Nothing is uploaded."
        model.candidateRows = listOf(
            CandidateRow(
                wording = "Possible candidate. Not an identification.",
                uncertainty = "Score 0.90.",
                title = "Fixture subject A",
                posterClassLabel = "Missing",
                url = "https://www.fbi.gov/wanted",
                cropPath = null,
                framePath = null,
            )
        )
        compose.setContent { OpenWorldApp(model = model, onChoose = {}, onOpen = {}) }
        val texts = compose.onAllNodes(SemanticsMatcher("has text") {
            it.config.getOrNull(SemanticsProperties.Text) != null
        }, useUnmergedTree = true).fetchSemanticsNodes().map { node ->
            node.config[SemanticsProperties.Text].joinToString { it.text }
        }
        val classAt = texts.indexOfFirst { it.contains("Missing and wanted.") }
        val openAt = texts.indexOfFirst { it == "Open FBI page" }
        val disclosureAt = texts.indexOfFirst { it.contains("Nothing is uploaded.") }
        assertTrue("$texts", classAt >= 0 && openAt > classAt && disclosureAt > openAt)
        assertTrue(texts.any { it == "Fixture subject A (Missing)" })
        assertTrue(texts.none { it.contains("(missing)") || it.contains("(wanted)") })
    }

    @Test
    fun aPosterWithoutATitleKeepsTheClass() {
        val model = FlowModel()
        model.step = Step.Results
        model.status = "complete"
        model.summary = "Possible candidate. Not an identification."
        model.candidateRows = listOf(
            CandidateRow(
                wording = "Possible candidate. Not an identification.",
                uncertainty = "Score 0.90.",
                title = "",
                posterClassLabel = "Missing",
                url = "https://www.fbi.gov/wanted",
                cropPath = null,
                framePath = null,
            )
        )
        compose.setContent { OpenWorldApp(model = model, onChoose = {}, onOpen = {}) }
        compose.onNodeWithText("Missing").assertExists()
        compose.onAllNodesWithText(" (Missing)", substring = true).assertCountEquals(0)
        assertEquals("Fixture subject A", posterLine("Fixture subject A", ""))
        assertEquals("", posterLine("", ""))
    }

    @Test
    fun aCandidateWithoutAPageKeepsTheCard() {
        val out = File(System.getProperty("java.io.tmpdir") ?: ".", "openworld-rows")
        val candidates = org.json.JSONArray(
            """[{"wording":"Possible candidate. Not an identification.","uncertainty":"Score 0.90.","poster_title":"Fixture subject A","poster_class_label":"Missing","fbi_url":"","frame_label":"Frame 1."}]"""
        )
        val rows = candidateRowsFrom(candidates, out)
        assertEquals(1, rows.size)
        assertEquals("", rows[0].url)
        assertEquals("Possible candidate. Not an identification.", rows[0].wording)
        assertEquals("Fixture subject A", rows[0].title)
        val model = FlowModel()
        model.step = Step.Results
        model.status = "complete"
        model.summary = "Possible candidate. Not an identification."
        model.candidateRows = rows
        compose.setContent { OpenWorldApp(model = model, onChoose = {}, onOpen = {}) }
        compose.onNodeWithText("Fixture subject A (Missing)").assertExists()
        compose.onAllNodesWithText("Open FBI page").assertCountEquals(0)
        compose.onAllNodesWithText("Possible candidate. Not an identification.").assertCountEquals(2)
    }

    @Test
    fun aRefusedEstimateDoesNotAskForAClass() {
        val model = FlowModel()
        model.step = Step.Estimate
        model.estimateText = "Bad codec or unreadable file. Refusing."
        model.canAnalyze = false
        compose.setContent { OpenWorldApp(model = model, onChoose = {}, onOpen = {}) }
        compose.onNodeWithText("Bad codec or unreadable file. Refusing.").assertExists()
        compose.onAllNodesWithText("Estimate").assertCountEquals(0)
        compose.onAllNodesWithText("Missing").assertCountEquals(0)
        compose.onAllNodesWithText("Wanted").assertCountEquals(0)
        compose.onAllNodesWithText("Choose missing, wanted, or both.").assertCountEquals(0)
        compose.onAllNodesWithText("Analyze").assertCountEquals(0)
    }

    @Test
    fun anUnreadableImportShowsTheRefusalOnTheChoosePage() {
        val model = FlowModel()
        val resolver = ApplicationProvider.getApplicationContext<android.content.Context>().contentResolver
        model.choose(Uri.parse("content://app.openworld/missing.png"), resolver)
        compose.setContent { OpenWorldApp(model = model, onChoose = {}, onOpen = {}) }
        fun texts(): List<String> = compose.onAllNodes(SemanticsMatcher("has text") {
            it.config.getOrNull(SemanticsProperties.Text) != null
        }, useUnmergedTree = true).fetchSemanticsNodes().map { node ->
            node.config[SemanticsProperties.Text].joinToString { it.text }
        }
        val first = texts()
        val refusal = first.indexOf("The file could not be read. Refusing.")
        val title = first.indexOf("Choose a photo or video")
        assertTrue("$first", refusal >= 0 && title > refusal)
        compose.onAllNodesWithText("Continue").assertCountEquals(0)
        val file = still("blank")
        val uri = Uri.parse("content://app.openworld/${file.name}")
        shadowOf(resolver).registerInputStream(uri, file.inputStream())
        model.choose(uri, resolver)
        compose.waitForIdle()
        val opened = texts()
        assertFalse("$opened", opened.contains("The file could not be read. Refusing."))
        assertTrue(opened.contains("This file stays on this device."))
        val kept = model.fileName
        model.choose(Uri.parse("content://app.openworld/also-missing.png"), resolver)
        compose.waitForIdle()
        val again = texts()
        val refusalAgain = again.indexOf("The file could not be read. Refusing.")
        val back = again.indexOf("Back")
        val device = again.indexOf("This file stays on this device.")
        assertTrue("$again", refusalAgain >= 0 && back > refusalAgain && device > back)
        assertEquals(1, again.count { it == "The file could not be read. Refusing." })
        assertTrue(again.contains(kept))
    }

    @Test
    fun anUnreadableImportOnAResultStaysOnThatResult() {
        val model = FlowModel()
        val resolver = ApplicationProvider.getApplicationContext<android.content.Context>().contentResolver
        val scene = still("blank")
        val sceneUri = Uri.parse("content://app.openworld/${scene.name}")
        shadowOf(resolver).registerInputStream(sceneUri, scene.inputStream())
        model.choose(sceneUri, resolver)
        model.continueFromDevice()
        model.continueFromSize()
        assertEquals(Step.Estimate, model.step)
        val kept = model.fileName
        model.choose(Uri.parse("content://app.openworld/missing-on-estimate.png"), resolver)
        assertEquals(Step.Estimate, model.step)
        assertEquals(kept, model.fileName)
        assertEquals("The file could not be read. Refusing.", model.pickNotice)
        compose.setContent { OpenWorldApp(model = model, onChoose = {}, onOpen = {}) }
        fun texts(): List<String> = compose.onAllNodes(SemanticsMatcher("has text") {
            it.config.getOrNull(SemanticsProperties.Text) != null
        }, useUnmergedTree = true).fetchSemanticsNodes().map { node ->
            node.config[SemanticsProperties.Text].joinToString { it.text }
        }
        val estimate = texts()
        val refusal = estimate.indexOf("The file could not be read. Refusing.")
        val title = estimate.indexOf("Estimate")
        assertTrue("$estimate", refusal >= 0 && title > refusal)
        assertEquals(1, estimate.count { it == "The file could not be read. Refusing." })
        assertTrue(estimate.contains("Analyze"))

        model.analyze()
        val result = model.resultDir
        assertNotNull(result)
        assertTrue(File(result!!, "result.json").isFile)
        model.choose(Uri.parse("content://app.openworld/missing-on-result.png"), resolver)
        compose.waitForIdle()
        assertEquals(Step.Results, model.step)
        assertEquals(kept, model.fileName)
        assertTrue(File(result, "result.json").isFile)
        val shown = texts()
        val again = shown.indexOf("The file could not be read. Refusing.")
        val headline = shown.indexOf(model.summary)
        assertTrue("$shown", again >= 0 && headline > again)
        assertEquals(1, shown.count { it == "The file could not be read. Refusing." })
        compose.onNodeWithText("Delete").assertExists()

        val next = still("blank")
        val uri = Uri.parse("content://app.openworld/${next.name}")
        shadowOf(resolver).registerInputStream(uri, next.inputStream())
        model.choose(uri, resolver)
        compose.waitForIdle()
        assertEquals(Step.Device, model.step)
        assertEquals(next.name, model.fileName)
        assertNull(model.pickNotice)
        assertNull(model.resultDir)
        assertFalse(result.exists())
    }

    @Test
    fun aNewFileThatCannotRemoveTheResultSaysSoOnce() {
        val model = FlowModel()
        val resolver = ApplicationProvider.getApplicationContext<android.content.Context>().contentResolver
        val scene = still("blank")
        val sceneUri = Uri.parse("content://app.openworld/${scene.name}")
        shadowOf(resolver).registerInputStream(sceneUri, scene.inputStream())
        model.choose(sceneUri, resolver)
        model.continueFromDevice()
        model.continueFromSize()
        model.analyze()
        val result = model.resultDir
        assertNotNull(result)
        val kept = model.fileName
        result!!.setWritable(false)
        try {
            val next = still("blank")
            val uri = Uri.parse("content://app.openworld/${next.name}")
            shadowOf(resolver).registerInputStream(uri, next.inputStream())
            model.choose(uri, resolver)
            assertEquals(Step.Results, model.step)
            assertEquals(kept, model.fileName)
            assertEquals("The result could not be deleted.", model.deleteNotice)
            assertNull(model.pickNotice)
            assertTrue(result.exists())
            compose.setContent { OpenWorldApp(model = model, onChoose = {}, onOpen = {}) }
            compose.onAllNodesWithText("The result could not be deleted.").assertCountEquals(1)
            compose.onNodeWithText(model.summary).assertExists()
        } finally {
            result.setWritable(true)
            result.deleteRecursively()
        }
    }

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
        compose.onNodeWithText("Fast. Selected.").assertExists()
        compose.onNodeWithText("Accurate. Selected.").assertDoesNotExist()
        compose.onNodeWithText("Phones and long video.", substring = true).assertExists()
        compose.onNodeWithText("Not measured yet.").assertExists()
        compose.onNodeWithText("Continue").performClick()
        compose.onNodeWithText("640 px on the long side. Selected.").assertExists()
        compose.onNodeWithText("Complete. Every decoded frame. Selected.").assertExists()
        compose.onNodeWithText("Measured. 5 frames a second, plus the tracker. Selected.").assertDoesNotExist()
        compose.onNodeWithText("A face under 64 px on that image is left out.", substring = true).assertExists()
        compose.onNodeWithText("the label is \"Not compared.\"", substring = true).assertExists()
        compose.onNodeWithText("Continue").performClick()
        compose.onNodeWithText("This scan runs on the CPU. It will be slower, warmer, and use more battery.", substring = true).assertExists()
        compose.onNodeWithText("This phone may get hot.", substring = true).assertExists()
        compose.onNodeWithText("A long scan uses a lot of battery.", substring = true).assertExists()
        compose.onNodeWithText("Analyze").assertIsEnabled().performClick()
        waitForScan(compose, model)
        assertTrue(model.context.startsWith("1 frame analyzed."))
        assertTrue(model.context.contains("640 px on the long side."))
        assertTrue(model.context.indexOf("Every decoded frame.") > model.context.indexOf("640 px on the long side."))
        compose.onNodeWithText("1 frame analyzed.", substring = true).assertExists()
        compose.onNodeWithText("640 px on the long side.", substring = true).assertExists()
        compose.onNodeWithText("Every decoded frame.", substring = true).assertExists()
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
        compose.onNodeWithText("Fixture subject A (Missing)").assertExists()
        compose.onNodeWithText("Fixture vehicle C (Wanted)").assertExists()
        assertTrue(model.candidateRows.any { it.uncertainty.startsWith("Score ") && it.uncertainty.contains("keeps a candidate at") && !it.uncertainty.contains("Possible candidate") })
        assertTrue(model.candidateRows.any { it.uncertainty.contains("The plate reads") && it.uncertainty.contains("FIX123") && !it.uncertainty.contains("Possible candidate") })
        compose.onNodeWithText("keeps a candidate at", substring = true).assertExists()
        compose.onNodeWithText("The plate reads", substring = true).assertExists()
        compose.onAllNodesWithText("(missing)", substring = true).assertCountEquals(0)
        compose.onAllNodesWithText("(wanted)", substring = true).assertCountEquals(0)
        compose.onNodeWithText("Crops from this file.").assertExists()
        compose.onAllNodesWithText("Crop").assertCountEquals(model.candidateRows.size)
        compose.onAllNodesWithText("Frame 1.").assertCountEquals(model.candidateRows.size + model.strip.size)
        assertTrue(model.candidateRows.all { it.frameLabel == "Frame 1." })
        assertTrue(model.strip.all { it.frameLabel == "Frame 1." })
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
        compose.onAllNodesWithText("No candidate is not a clearance.", substring = true).assertCountEquals(2)
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
        assertTrue(textUsesWarningColor("The file was not fully decoded."))
        assertFalse(textUsesWarningColor("Incomplete."))
        model.incompleteReason = "The result could not be written."
        compose.onNodeWithText("The result could not be written.").assertExists()
        compose.onNodeWithText("Incomplete.").assertExists()
        assertTrue(textUsesWarningColor("The result could not be written."))
    }

    @Test
    fun aMissingOutputDirectoryRefusesTheEstimate() {
        val model = FlowModel()
        val uri = Uri.parse("content://app.openworld/no-output.png")
        val resolver = ApplicationProvider.getApplicationContext<android.content.Context>().contentResolver
        shadowOf(resolver).registerInputStream(uri, still("blank").inputStream())
        model.choose(uri, resolver)
        model.continueFromDevice()
        model.continueFromSize()
        assertTrue(model.estimateOk)
        val blocker = File.createTempFile("ow-not-a-dir", "")
        val previous = System.getProperty("java.io.tmpdir")
        System.setProperty("java.io.tmpdir", blocker.absolutePath)
        try {
            model.analyze()
        } finally {
            if (previous != null) System.setProperty("java.io.tmpdir", previous)
            blocker.delete()
        }
        assertEquals(Step.Estimate, model.step)
        assertFalse(model.estimateOk)
        assertFalse(model.canAnalyze)
        assertEquals("The output directory could not be created. Refusing.", model.estimateText)
        model.refreshClassLine()
        assertEquals("The output directory could not be created. Refusing.", model.estimateText)
        model.oldFile = true
        compose.setContent { OpenWorldApp(model = model, onChoose = {}, onOpen = {}) }
        compose.onNodeWithText("The output directory could not be created. Refusing.").assertExists()
        compose.onAllNodesWithText("Estimate").assertCountEquals(0)
        compose.onAllNodesWithText("This file is older than about 30 days.").assertCountEquals(0)
        compose.onAllNodesWithText("Analyze").assertCountEquals(0)
        compose.onAllNodesWithText("Missing").assertCountEquals(0)
        compose.onAllNodesWithText("Wanted").assertCountEquals(0)
    }

    @Test
    fun aRefusedScanWithoutAResultHidesDelete() {
        val model = FlowModel()
        val uri = Uri.parse("content://app.openworld/unknown-bundle.png")
        val resolver = ApplicationProvider.getApplicationContext<android.content.Context>().contentResolver
        shadowOf(resolver).registerInputStream(uri, still("blank").inputStream())
        model.choose(uri, resolver)
        model.bundleId = "not-in-the-catalog"
        model.analyze()
        assertEquals(Step.Results, model.step)
        assertEquals("That bundle is not in the catalog. Refusing.", model.summary)
        assertNull(model.resultDir)
        assertFalse(model.detail.contains("No candidate is not a clearance."))
        compose.setContent { OpenWorldApp(model = model, onChoose = {}, onOpen = {}) }
        compose.onNodeWithText("That bundle is not in the catalog. Refusing.").assertExists()
        compose.onAllNodesWithText("Delete").assertCountEquals(0)
        compose.onAllNodesWithText("No candidate is not a clearance.").assertCountEquals(0)
        model.chooseAnother()
        assertEquals(Step.Choose, model.step)
        assertEquals("", model.summary)
    }

    @Test
    fun aPosterPackFileRefusesBeforeTheScan() {
        val model = FlowModel()
        val uri = Uri.parse("content://app.openworld/poster-pack.png")
        val resolver = ApplicationProvider.getApplicationContext<android.content.Context>().contentResolver
        shadowOf(resolver).registerInputStream(uri, still("blank").inputStream())
        model.choose(uri, resolver)
        val blocker = File.createTempFile("ow-posters", "")
        blocker.writeText("keep")
        model.fixturePackDirectory = blocker
        model.analyze()
        assertEquals(Step.Results, model.step)
        assertEquals("refused", model.status)
        assertEquals("The poster pack could not be read. Refusing.", model.summary)
        assertNull(model.resultDir)
        assertTrue(model.detail.contains("Nothing is uploaded."))
        assertTrue(model.detail.contains("On-device does not mean the file is real."))
        assertFalse(model.detail.contains("No candidate is not a clearance."))
        assertEquals("keep", blocker.readText())
        compose.setContent { OpenWorldApp(model = model, onChoose = {}, onOpen = {}) }
        compose.onNodeWithText("The poster pack could not be read. Refusing.").assertExists()
        compose.onAllNodesWithText("No candidate is not a clearance.").assertCountEquals(0)
        compose.onAllNodesWithText("The poster pack is missing. Refusing.").assertCountEquals(0)
        compose.onAllNodesWithText("Delete").assertCountEquals(0)
        compose.onNodeWithText("Choose another file").assertExists()
        blocker.delete()
    }

    @Test
    fun aDecodeFailureDoesNotRepeatTheRefusal() {
        val model = FlowModel()
        val outcome = model.decodeFailure("Bad codec or unreadable file. Refusing.")
        model.step = Step.Results
        model.summary = outcome.summary
        model.status = outcome.status
        model.detail = outcome.detail
        model.incompleteReason = outcome.incompleteReason
        compose.setContent { OpenWorldApp(model = model, onChoose = {}, onOpen = {}) }
        compose.onAllNodesWithText("Bad codec or unreadable file. Refusing.").assertCountEquals(1)
        compose.onAllNodesWithText("No candidate is not a clearance.").assertCountEquals(0)
        assertTrue(outcome.detail.contains("Nothing is uploaded."))
        assertTrue(outcome.detail.contains("On-device does not mean the file is real."))
        model.summary = ""
        model.status = "refused"
        compose.onNodeWithText("Refusing.").assertExists()
        compose.onAllNodesWithText("Incomplete.").assertCountEquals(0)
        model.summary = "The result could not be written."
        compose.onNodeWithText("The result could not be written.").assertExists()
        compose.onAllNodesWithText("Refusing.").assertCountEquals(0)
        val stopped = model.decodeFailure("The file was not fully decoded.")
        assertEquals("Incomplete.", stopped.summary)
        assertEquals("The file was not fully decoded.", stopped.incompleteReason)
        assertEquals("", stopped.detail)
    }

    @Test
    fun anUnreadableProgramAnswerUsesTheCommandRefusal() {
        val model = FlowModel()
        val uri = Uri.parse("content://app.openworld/unreadable.png")
        val resolver = ApplicationProvider.getApplicationContext<android.content.Context>().contentResolver
        shadowOf(resolver).registerInputStream(uri, still("blank").inputStream())
        model.choose(uri, resolver)
        Core.stdoutForTest = { "" }
        try {
            model.continueFromDevice()
            assertEquals("The bundle catalog could not be read. Refusing.", model.bundleNotice)
            Core.stdoutForTest = { "not json" }
            model.continueFromDevice()
            compose.setContent { OpenWorldApp(model = model, onChoose = {}, onOpen = {}) }
            compose.onAllNodesWithText("Model bundle").assertCountEquals(0)
            compose.onAllNodesWithText("The bundle catalog could not be read. Refusing.").assertCountEquals(1)
            compose.onAllNodesWithText("The scan program is not on this device. Refusing.").assertCountEquals(0)
            model.continueFromSize()
            assertEquals("The scan could not be read. Refusing.", model.estimateText)
            assertFalse(model.canAnalyze)
            compose.onNodeWithText("The scan could not be read. Refusing.").assertExists()
            compose.onAllNodesWithText("Analyze").assertCountEquals(0)
            Core.stdoutForTest = null
            model.continueFromSize()
            assertTrue(model.canAnalyze)
            Core.stdoutForTest = { args ->
                if (args.contains("posters")) """{"id":"fixture-v0","posters":3}""" else "not json"
            }
            model.analyze()
            assertEquals(Step.Results, model.step)
            assertEquals("The scan could not be read. Refusing.", model.summary)
            assertNull(model.resultDir)
            assertTrue(model.detail.contains("Nothing is uploaded."))
            assertFalse(model.detail.contains("No candidate is not a clearance."))
            compose.onNodeWithText("The scan could not be read. Refusing.").assertExists()
            compose.onNodeWithText("Nothing is uploaded.", substring = true).assertExists()
            compose.onAllNodesWithText("No candidate is not a clearance.").assertCountEquals(0)
            compose.onAllNodesWithText("Delete").assertCountEquals(0)
        } finally {
            Core.stdoutForTest = null
        }
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

    @Test
    fun aRefusedDeleteKeepsTheResultOnScreen() {
        val model = FlowModel()
        val uri = Uri.parse("content://app.openworld/keep.png")
        val resolver = ApplicationProvider.getApplicationContext<android.content.Context>().contentResolver
        shadowOf(resolver).registerInputStream(uri, still("scene").inputStream())
        model.choose(uri, resolver)
        model.continueFromDevice()
        model.continueFromSize()
        model.analyze()
        val result = model.resultDir
        assertNotNull(result)
        assertTrue(File(result!!, "result.json").delete())
        model.deleteResult()
        assertEquals("Possible candidate. Not an identification.", model.summary)
        assertTrue(model.candidateRows.isNotEmpty())
        assertEquals(
            "Refusing to delete a directory that is not an OpenWorld result.",
            model.deleteNotice,
        )
        assertTrue(result.exists())
        compose.setContent { OpenWorldApp(model = model, onChoose = {}, onOpen = {}) }
        assertTrue(compose.onAllNodesWithText("Possible candidate. Not an identification.").fetchSemanticsNodes().isNotEmpty())
        compose.onNodeWithText("Refusing to delete a directory that is not an OpenWorld result.").assertExists()
        compose.onNodeWithText("Delete").assertExists()
        assertTrue(compose.onAllNodesWithText("Open FBI page").fetchSemanticsNodes().isNotEmpty())
        model.back()
        assertEquals(Step.Estimate, model.step)
        model.deleteNotice = "The result could not be deleted."
        compose.onNodeWithText("The result could not be deleted.").assertExists()
        model.deleteNotice = null
        model.analyze()
        assertEquals(Step.Results, model.step)
        assertEquals("Possible candidate. Not an identification.", model.summary)
        assertTrue(result.exists())
        assertTrue(result != model.resultDir)
    }

    /** Glyph cores of a warning are the same brown the desktop uses. A title stays off that brown. */
    private fun textUsesWarningColor(text: String): Boolean {
        val pixels = compose.onNodeWithText(text).captureToImage().toPixelMap()
        var brown = 0
        for (y in 0 until pixels.height) {
            for (x in 0 until pixels.width) {
                val color = pixels[x, y]
                if (color.alpha < 0.4f) continue
                val red = color.red
                val green = color.green
                val blue = color.blue
                if (red in 0.35f..0.75f && green in 0.15f..0.55f && blue < 0.2f && red > green && green > blue) {
                    brown++
                }
            }
        }
        return brown > 8
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
        compose.onNodeWithText("Import a file you already have, or receive it from the share sheet. There is no camera.").assertExists()
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

private fun waitForScan(compose: androidx.compose.ui.test.junit4.ComposeContentTestRule, model: FlowModel) {
    val deadline = System.currentTimeMillis() + 20_000
    while (System.currentTimeMillis() < deadline && !(model.step == Step.Results && !model.scanning)) {
        Thread.sleep(50)
        compose.waitForIdle()
    }
    assertEquals(Step.Results, model.step)
    assertFalse(model.scanning)
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

private fun pngWithOrientation(tag: Int): ByteArray {
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
    val signature = byteArrayOf(0x89.toByte(), 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A)
    val chunk = be32(tiff.size) + "eXIf".encodeToByteArray() + tiff + be32(0)
    val end = be32(0) + "IEND".encodeToByteArray() + be32(0)
    return signature + chunk + end
}

private fun be32(value: Int): ByteArray {
    return byteArrayOf(
        (value shr 24).toByte(),
        (value shr 16).toByte(),
        (value shr 8).toByte(),
        value.toByte(),
    )
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

private data class TiffPage(val width: Int, val height: Int, val rgb: ByteArray, val tag: Int)

private fun rgbTiff(pages: List<TiffPage>): ByteArray {
    val entryCount = 10
    val ifdLen = 2 + entryCount * 12 + 4
    var cursor = 8
    val layout = ArrayList<IntArray>()
    for (page in pages) {
        check(page.rgb.size == page.width * page.height * 3)
        val ifd = cursor
        val bits = ifd + ifdLen
        val pixels = bits + 6
        cursor = pixels + page.rgb.size
        layout.add(intArrayOf(ifd, bits, pixels))
    }
    val out = ByteArray(cursor)
    out[0] = 0x49
    out[1] = 0x49
    out[2] = 0x2A
    out[4] = 8
    for (index in pages.indices) {
        val page = pages[index]
        val ifd = layout[index][0]
        val bits = layout[index][1]
        val pixels = layout[index][2]
        val next = if (index + 1 < layout.size) layout[index + 1][0] else 0
        put16(out, ifd, entryCount)
        val entries = arrayOf(
            intArrayOf(256, 4, 1, page.width),
            intArrayOf(257, 4, 1, page.height),
            intArrayOf(258, 3, 3, bits),
            intArrayOf(259, 3, 1, 1),
            intArrayOf(262, 3, 1, 2),
            intArrayOf(273, 4, 1, pixels),
            intArrayOf(274, 3, 1, page.tag),
            intArrayOf(277, 3, 1, 3),
            intArrayOf(278, 4, 1, page.height),
            intArrayOf(279, 4, 1, page.rgb.size),
        )
        var at = ifd + 2
        for (entry in entries) {
            put16(out, at, entry[0])
            put16(out, at + 2, entry[1])
            put32(out, at + 4, entry[2])
            put32(out, at + 8, entry[3])
            at += 12
        }
        put32(out, at, next)
        put16(out, bits, 8)
        put16(out, bits + 2, 8)
        put16(out, bits + 4, 8)
        page.rgb.copyInto(out, pixels)
    }
    return out
}

private fun put16(out: ByteArray, offset: Int, value: Int) {
    out[offset] = (value and 0xFF).toByte()
    out[offset + 1] = ((value shr 8) and 0xFF).toByte()
}

private fun put32(out: ByteArray, offset: Int, value: Int) {
    put16(out, offset, value and 0xFFFF)
    put16(out, offset + 2, (value shr 16) and 0xFFFF)
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
