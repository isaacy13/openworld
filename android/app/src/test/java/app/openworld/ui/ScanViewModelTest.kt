// SPDX-License-Identifier: Apache-2.0
package app.openworld.ui

import app.openworld.AppEnv
import app.openworld.core.JobStart
import app.openworld.core.Status
import app.openworld.core.TrackStatus
import app.openworld.media.Probe
import app.openworld.media.Refusal
import app.openworld.platform.Imported
import app.openworld.platform.Paths
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import java.io.File

@OptIn(ExperimentalCoroutinesApi::class)
class ScanViewModelTest {
    private val dispatcher = StandardTestDispatcher()
    private val core = FakeCore()
    private val reader = FakeReader()
    private val device = FakeDevice()
    private val imports = FakeImports()
    private val marker = FakeMarker()
    private val root = File("/data/openworld")
    private val file = Imported(File(root, "imports/clip.mp4"), "clip.mp4", "video/mp4", 1_000_000, null, "content://media/1")

    @Before
    fun setUp() = Dispatchers.setMain(dispatcher)

    @After
    fun tearDown() = Dispatchers.resetMain()

    private fun TestScope.model(): ScanViewModel {
        val env = AppEnv(
            core = core,
            reader = reader,
            device = device,
            imports = imports,
            marker = marker,
            paths = Paths(File(root, "bundles"), File(root, "posters"), File(root, "results")),
            install = {},
            io = dispatcher,
            now = { 1_760_868_000_000L },
        )
        return ScanViewModel(env).also { advanceUntilIdle() }
    }

    private fun ScanViewModel.screen() = state.value.screen

    private fun TestScope.reviewed(): ScanViewModel = model().also {
        it.openImported(file)
        advanceUntilIdle()
    }

    @Test
    fun startsReadyWithTheDemoPack() = runTest(dispatcher) {
        val vm = model()
        val state = vm.state.value
        assertTrue(state.ready)
        assertTrue(state.demoBuild)
        assertEquals(1, state.posters?.missing)
        assertTrue(state.screen is ScanState.Start)
        assertEquals(listOf("copy", "version", "sweep", "bundles", "posters_check"), core.calls.map { it.getString("op") })
    }

    @Test
    fun reviewShowsTheEstimateForThePushedMedia() = runTest(dispatcher) {
        val review = reviewed().screen() as ScanState.Review
        assertTrue(review.media is MediaState.Ready)
        assertEquals("fast", review.bundleId)
        assertEquals(listOf("fast", "accurate"), review.bundles.map { it.id })
        val estimate = (review.estimate as EstimateState.Ready).value
        assertEquals("duration", estimate.human?.code)
        assertTrue(review.canAnalyze)

        val request = core.calls.last { it.getString("op") == "estimate" }
        assertEquals(3, request.getJSONObject("media").getLong("frames"))
        assertEquals("phone", request.getJSONObject("device").getString("kind"))
        assertFalse(request.has("out"))
        // The disclosure and the file's warnings are the core's, from the estimate.
        assertEquals(FakeCore.DISCLOSURE, review.disclosure.map { it.code })
        assertTrue(review.warnings.isEmpty())
    }

    @Test
    fun theFileWarningsComeFromTheCoresEstimate() = runTest(dispatcher) {
        val vm = model()
        vm.openImported(file.copy(mtimeUnix = 1_000_000_000L))
        advanceUntilIdle()
        val review = vm.screen() as ScanState.Review
        assertEquals(1_000_000_000L, core.calls.last { it.getString("op") == "estimate" }.getLong("file_mtime_unix"))
        assertEquals(listOf("old_file"), review.warnings.map { it.code })
    }

    @Test
    fun bothClassesOffDisablesAnalyze() = runTest(dispatcher) {
        val vm = reviewed()
        vm.setMissing(false)
        vm.setWanted(false)
        advanceUntilIdle()
        val review = vm.screen() as ScanState.Review
        assertTrue(review.bothOff)
        assertFalse(review.canAnalyze)
        assertEquals("choose_class", (review.estimate as EstimateState.Ready).value.classes?.code)

        vm.analyze()
        advanceUntilIdle()
        assertTrue(vm.screen() is ScanState.Review)
        assertTrue(core.starts.isEmpty())
    }

    @Test
    fun eventsBecomeFacesAndTheReportBecomesTheResult() = runTest(dispatcher) {
        val vm = reviewed()
        val seen = mutableListOf<ScanState.Scanning>()
        reader.beforeFrame = { i -> if (i == 1) seen += vm.screen() as ScanState.Scanning }
        vm.analyze()
        advanceUntilIdle()

        // After frame 0 the face is there and still being looked at.
        val mid = seen.single()
        assertEquals(0L, mid.frame)
        assertEquals(1, mid.faces.size)
        assertNull(mid.faces[0].status)
        assertTrue(mid.faces[0].crop!!.endsWith("crops/face_1.jpg"))

        val done = vm.screen() as ScanState.Done
        assertEquals(Status.COMPLETE, done.report.status)
        assertEquals("possible_candidate", done.report.headline.code)
        assertEquals(1, done.report.candidates.size)
        assertEquals(TrackStatus.CANDIDATE, done.faces.single().status)
        assertEquals(true, core.lastJob?.finishedClean)

        // A pushed job names no op, and its result folder is under results.
        val request = core.starts.single()
        assertFalse(request.has("op"))
        assertTrue(request.getString("out").startsWith(File(root, "results").path))
        assertEquals(listOf(File(request.getString("out"))), marker.begun)
        assertTrue(marker.marks.isEmpty())
        assertTrue(device.closed)
        assertEquals(listOf(file), imports.deleted)
    }

    @Test
    fun stopIsIncompleteAndSaysWhy() = runTest(dispatcher) {
        val vm = reviewed()
        reader.beforeFrame = { i -> if (i == 1) vm.stop() }
        vm.analyze()
        advanceUntilIdle()
        val done = vm.screen() as ScanState.Done
        assertEquals(Status.INCOMPLETE, done.report.status)
        assertEquals("cancelled", done.report.reason?.code)
        assertTrue(done.report.candidates.isEmpty())
        assertEquals("cancel", core.lastJob?.stopReason)
    }

    @Test
    fun heatFromTheDeviceStopsTheScan() = runTest(dispatcher) {
        val vm = reviewed()
        reader.beforeFrame = { i -> if (i == 1) device.fire("heat") }
        vm.analyze()
        advanceUntilIdle()
        val done = vm.screen() as ScanState.Done
        assertEquals(Status.INCOMPLETE, done.report.status)
        assertEquals("stopped_heat", done.report.reason?.code)
    }

    @Test
    fun aDecoderCrashIsIncompleteNotAClearance() = runTest(dispatcher) {
        val vm = reviewed()
        reader.crash = true
        vm.analyze()
        advanceUntilIdle()
        val done = vm.screen() as ScanState.Done
        assertEquals(Status.INCOMPLETE, done.report.status)
        assertEquals("not_fully_decoded", done.report.reason?.code)
        assertEquals(false, core.lastJob?.finishedClean)
    }

    @Test
    fun aRefusedJobShowsTheRefusal() = runTest(dispatcher) {
        core.onStart = { _, _ -> JobStart.Refused(FakeCore.refusal("missing_pack")) }
        val vm = reviewed()
        vm.analyze()
        advanceUntilIdle()
        val done = vm.screen() as ScanState.Done
        assertEquals(Status.REFUSED, done.report.status)
        assertEquals("missing_pack", done.report.refusal)
        assertEquals("missing_pack", done.report.headline.code)
        assertNull(done.report.out)
        // The empty result folder a refusal may leave is removed.
        assertTrue(core.calls.any { it.getString("op") == "delete" })
    }

    @Test
    fun anAnimatedFileIsRefusedBeforeAnalyze() = runTest(dispatcher) {
        reader.probe = Probe.Refused(Refusal.ANIMATED)
        val review = reviewed().screen() as ScanState.Review
        assertEquals("refuse_animated", (review.media as MediaState.Refused).line.code)
        assertFalse(review.canAnalyze)
        assertTrue(core.calls.none { it.getString("op") == "estimate" })
    }

    @Test
    fun aScanTheSystemKilledShowsAsIncompleteOnTheNextLaunch() = runTest(dispatcher) {
        val leftover = File(root, "results/2026-10-19T10-00-00Z")
        marker.marks += leftover
        val vm = model()
        val done = vm.screen() as ScanState.Done
        assertEquals(Status.INCOMPLETE, done.report.status)
        assertEquals("scan_killed", done.report.reason?.code)
        // No estimate has run, so the disclosure is the core's from the copy call.
        assertEquals(FakeCore.DISCLOSURE - "no_clearance", done.report.disclosure.map { it.code })
        val delete = core.calls.single { it.getString("op") == "delete" }
        assertEquals(leftover.path, delete.getString("out"))
        assertTrue(marker.marks.isEmpty())
    }

    @Test
    fun aScanStillRunningInThisProcessIsNotKilledAndKeepsItsMark() = runTest(dispatcher) {
        // An earlier screen's scan, still running after the screen was recreated.
        val other = File(root, "results/2026-10-19T09-00-00Z")
        marker.begin(other)
        val vm = model()
        assertTrue(vm.screen() is ScanState.Start)
        assertNull(vm.state.value.message)
        assertTrue(core.calls.none { it.getString("op") == "delete" })

        vm.openImported(file)
        advanceUntilIdle()
        vm.analyze()
        advanceUntilIdle()
        assertTrue(vm.screen() is ScanState.Done)
        // This scan ended only its own mark.
        assertEquals(setOf(other), marker.marks)
    }

    @Test
    fun aRecreatedScreenClearsLeftoverCopiesButNotTheOneARunningScanReads() = runTest(dispatcher) {
        val leftover = File(root, "imports/old.mp4")
        imports.files += leftover
        val vm = model()
        assertTrue(imports.files.isEmpty())
        imports.files += setOf(file.file, leftover)
        vm.openImported(file)
        advanceUntilIdle()
        val during = mutableListOf<Set<File>>()
        reader.beforeFrame = { i ->
            if (i == 1) {
                // What a recreated screen's setup does while this scan runs.
                imports.clear()
                during += imports.files.toSet()
            }
        }
        vm.analyze()
        advanceUntilIdle()
        assertEquals(listOf(setOf(file.file)), during)
        assertEquals(Status.COMPLETE, (vm.screen() as ScanState.Done).report.status)
        // The scan ended, so its copy is deleted and no longer held.
        assertTrue(imports.files.isEmpty())
        assertTrue(imports.held.isEmpty())
    }

    @Test
    fun leavingOpensOnlyTheUrlTheCoreApproved() = runTest(dispatcher) {
        val vm = reviewed()
        vm.analyze()
        advanceUntilIdle()
        val candidate = (vm.screen() as ScanState.Done).report.candidates.single()

        core.leave = { _ -> FakeCore.refusal("only_fbi") }
        vm.openLeave(candidate)
        advanceUntilIdle()
        assertNull((vm.screen() as ScanState.Done).leave)
        assertEquals("only_fbi", vm.state.value.message?.code)

        core.leave = { _ -> org.json.JSONObject().put("ok", true).put("message", line("leaving")).put("url", "https://www.fbi.gov/wanted/x") }
        vm.openLeave(candidate)
        advanceUntilIdle()
        val leave = (vm.screen() as ScanState.Done).leave
        assertNotNull(leave)
        assertEquals("https://www.fbi.gov/wanted/x", leave!!.url)
        assertEquals("not_sending", leave.notes.single().code)
    }

    @Test
    fun deleteGoesBackToStart() = runTest(dispatcher) {
        val vm = reviewed()
        vm.analyze()
        advanceUntilIdle()
        vm.deleteResult()
        advanceUntilIdle()
        assertTrue(vm.screen() is ScanState.Start)
        assertEquals("deleted", vm.state.value.message?.code)
    }
}
