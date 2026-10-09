// SPDX-License-Identifier: Apache-2.0
package app.openworld

import android.content.Intent
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
