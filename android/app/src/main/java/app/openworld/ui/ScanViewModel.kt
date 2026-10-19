// SPDX-License-Identifier: Apache-2.0
package app.openworld.ui

import android.net.Uri
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import app.openworld.AppEnv
import app.openworld.core.BundleRow
import app.openworld.core.Candidate
import app.openworld.core.Copy
import app.openworld.core.Estimate
import app.openworld.core.JobStart
import app.openworld.core.Line
import app.openworld.core.NativeCore
import app.openworld.core.PosterPack
import app.openworld.core.Report
import app.openworld.core.ScanJob
import app.openworld.core.Status
import app.openworld.core.Stop
import app.openworld.core.Track
import app.openworld.core.line
import app.openworld.core.request
import app.openworld.core.text
import app.openworld.media.MediaInfo
import app.openworld.media.Probe
import app.openworld.media.Refusal
import app.openworld.platform.ImportRefused
import app.openworld.platform.Imported
import app.openworld.platform.Shell
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.io.File
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import java.util.TimeZone

/**
 * The scan flow. Every core call and every decode runs on `env.io`; the screens only read [state].
 * The core decides what a result means; this class only carries its answers to the screen.
 */
class ScanViewModel(private val env: AppEnv) : ViewModel() {
    private val _state = MutableStateFlow(UiState())
    val state: StateFlow<UiState> = _state.asStateFlow()

    @Volatile private var copy = Copy.EMPTY
    @Volatile private var bundles: List<BundleRow> = emptyList()
    @Volatile private var bundleNote: Line? = null
    /** The disclosure from the core's last estimate, for a new review and a shell-made report. */
    @Volatile private var disclosure: List<Line> = emptyList()
    private val ready = CompletableDeferred<Unit>()
    /** One core call at a time outside a scan, so estimates never pile up. */
    private val calls = Mutex()
    private var importJob: Job? = null
    private var probeJob: Job? = null
    private var estimateJob: Job? = null
    @Volatile private var session: Session? = null
    /** The copy on disk for the file under review. */
    private var current: Imported? = null

    init {
        viewModelScope.launch { setup() }
    }

    private suspend fun setup() {
        val leftover = withContext(env.io) {
            // A failed install shows up as the catalog's refusal below.
            runCatching { env.install() }
            copy = Copy.from(env.core.call(request("copy")))
            val version = env.core.call(request("version"))
            env.core.call(request("sweep") { put("root", env.paths.results.path) })
            runCatching { env.imports.clear() }
            loadCatalog()
            val (pack, why) = checkPosters()
            // Only scans no longer running in this process: a recreated screen leaves its own scan alone.
            val leftover = env.marker.leftovers()
            for (out in leftover) {
                env.core.call(request("delete") { put("out", out.path) })
                env.marker.end(out)
            }
            _state.update { it.copy(demoBuild = version.optBoolean("demo_posters"), posters = pack, postersRefusal = why) }
            leftover
        }
        ready.complete(Unit)
        _state.update { s ->
            val killed = leftover.isNotEmpty()
            val idle = s.screen is ScanState.Start && (s.screen as ScanState.Start).importing == null
            when {
                killed && idle -> s.copy(ready = true, screen = ScanState.Done("", killedReport()))
                killed -> s.copy(ready = true, message = copy.line(Shell.SCAN_KILLED))
                else -> s.copy(ready = true)
            }
        }
    }

    private fun loadCatalog() {
        val answer = env.core.call(request("bundles") { put("bundles", env.paths.bundles.path) })
        bundles = if (answer.optBoolean("ok")) BundleRow.list(answer) else emptyList()
        bundleNote = answer.line("note")
    }

    private fun checkPosters(): Pair<PosterPack?, Line?> {
        val answer = env.core.call(request("posters_check") { put("posters", env.paths.posters.path) })
        return if (answer.optBoolean("ok")) {
            Pair(PosterPack.from(answer.optJSONObject("posters")), null)
        } else {
            Pair(null, answer.line("headline"))
        }
    }

    // Start.

    /** A picked or shared file. Copied off the main thread, then reviewed. */
    fun choose(uri: Uri) {
        if (_state.value.screen is ScanState.Scanning) {
            message(copy.line(Shell.SCAN_BUSY))
            return
        }
        importJob?.cancel()
        dropImport()
        _state.update { it.copy(screen = ScanState.Start(importing = Importing(null, null))) }
        importJob = viewModelScope.launch {
            try {
                val imported = env.imports.copy(uri) { progress ->
                    _state.update { s ->
                        val start = s.screen as? ScanState.Start ?: return@update s
                        if (start.importing == null) s else s.copy(screen = start.copy(importing = Importing(null, progress)))
                    }
                }
                openImported(imported)
            } catch (e: CancellationException) {
                throw e
            } catch (e: ImportRefused) {
                _state.update { it.copy(screen = ScanState.Start(notice = copy.line(e.code))) }
            } catch (t: Throwable) {
                _state.update { it.copy(screen = ScanState.Start(notice = copy.line(Shell.IMPORT_FAILED))) }
            }
        }
    }

    /** A share that was not a content:// photo or video. */
    fun refuseShare() {
        if (_state.value.screen is ScanState.Scanning) return
        message(copy.line(Shell.SHARE_NOT_FILE))
    }

    fun cancelImport() {
        importJob?.cancel()
        importJob = null
        _state.update { it.copy(screen = ScanState.Start()) }
    }

    fun installDemoPosters() {
        viewModelScope.launch {
            val answer = withContext(env.io) {
                calls.withLock { env.core.call(request("posters_demo") { put("out", env.paths.posters.path) }) }
            }
            if (!answer.optBoolean("ok")) message(answer.line("headline"))
            refreshPosters()
        }
    }

    private suspend fun refreshPosters() {
        val (pack, why) = withContext(env.io) { calls.withLock { checkPosters() } }
        _state.update { it.copy(posters = pack, postersRefusal = why) }
        if (_state.value.screen is ScanState.Review) estimate()
    }

    // Review.

    /** Review a copied file. Internal so tests can skip the content resolver. */
    internal fun openImported(imported: Imported) {
        current?.takeIf { it != imported }?.let { env.imports.delete(it) }
        current = imported
        probeJob?.cancel()
        estimateJob?.cancel()
        _state.update {
            it.copy(screen = ScanState.Review(file = imported, bundleId = preselected(), disclosure = disclosure))
        }
        probeJob = viewModelScope.launch {
            ready.await()
            val probe = withContext(env.io) {
                try {
                    env.reader.probe(imported.file, imported.mime)
                } catch (e: CancellationException) {
                    throw e
                } catch (t: Throwable) {
                    Probe.Refused(Refusal.BAD_CODEC)
                }
            }
            updateReview(imported) { r ->
                val base = r.copy(bundles = bundles, bundleNote = bundleNote, bundleId = r.bundleId.takeIf { id -> bundles.any { it.id == id } } ?: preselected())
                when (probe) {
                    is Probe.Ready -> base.copy(media = MediaState.Ready(probe.info))
                    is Probe.Refused -> base.copy(media = MediaState.Refused(copy.line(probe.code)))
                }
            }
            if (probe is Probe.Ready) estimate()
        }
    }

    fun setMissing(on: Boolean) = changeOptions { it.copy(missing = on) }

    fun setWanted(on: Boolean) = changeOptions { it.copy(wanted = on) }

    fun setBundle(id: String) = changeOptions { it.copy(bundleId = id) }

    fun setCoverage(coverage: String) = changeOptions { it.copy(coverage = coverage) }

    private fun changeOptions(change: (ScanState.Review) -> ScanState.Review) {
        _state.update { s -> (s.screen as? ScanState.Review)?.let { s.copy(screen = change(it)) } ?: s }
        estimate()
    }

    private fun estimate() {
        val review = _state.value.screen as? ScanState.Review ?: return
        val info = (review.media as? MediaState.Ready)?.info ?: return
        estimateJob?.cancel()
        updateReview(review.file) { it.copy(estimate = EstimateState.Loading) }
        estimateJob = viewModelScope.launch {
            val answer = withContext(env.io) {
                calls.withLock { env.core.call(scanRequest("estimate", review, info, null)) }
            }
            if (answer.optBoolean("ok")) {
                // The file's warnings and the disclosure come from the core, as the estimate does.
                val value = Estimate.from(answer)
                if (value.disclosure.isNotEmpty()) disclosure = value.disclosure
                updateReview(review.file) {
                    it.copy(estimate = EstimateState.Ready(value), warnings = value.warnings, disclosure = value.disclosure)
                }
            } else {
                val refused = EstimateState.Refused(answer.line("headline") ?: copy.line(Refusal.BAD_CODEC))
                updateReview(review.file) { it.copy(estimate = refused) }
            }
        }
    }

    private fun updateReview(file: Imported, change: (ScanState.Review) -> ScanState.Review) {
        _state.update { s ->
            val review = s.screen as? ScanState.Review
            if (review == null || review.file != file) s else s.copy(screen = change(review))
        }
    }

    // Scan.

    fun analyze() {
        val review = _state.value.screen as? ScanState.Review ?: return
        if (!review.canAnalyze || session != null) return
        val info = (review.media as MediaState.Ready).info
        val out = newResultDir()
        val job = scanRequest(null, review, info, out)
        val banner = (review.estimate as? EstimateState.Ready)?.value?.coverageBanner
        val session = Session()
        this.session = session
        estimateJob?.cancel()
        _state.update {
            it.copy(screen = ScanState.Scanning(fileName = review.file.name, info = info, out = out, coverageBanner = banner))
        }
        val startedAt = env.now()
        viewModelScope.launch {
            val watch = runCatching { env.device.watch { reason -> stop(reason) } }.getOrNull()
            try {
                val done = try {
                    withContext(env.io) { runScan(session, job, review, info, out, startedAt) }
                } catch (e: CancellationException) {
                    throw e
                } catch (t: Throwable) {
                    ScanState.Done(review.file.name, failedReport(out))
                }
                current = null
                _state.update { it.copy(screen = done) }
            } finally {
                runCatching { watch?.close() }
                this@ScanViewModel.session = null
            }
        }
    }

    /** Blocking, on the scan thread. Any failure here ends as Incomplete or a refusal, never a clearance. */
    private fun runScan(session: Session, job: JSONObject, review: ScanState.Review, info: MediaInfo, out: File, startedAt: Long): ScanState.Done {
        try {
            env.marker.begin(out)
            val started = try {
                env.core.start(job) { event -> onEvent(event, out, startedAt) }
            } catch (t: Throwable) {
                JobStart.Refused(NativeCore.refusal(copy.line("missing_program")))
            }
            val report = when (started) {
                is JobStart.Refused -> Report.from(started.answer, null)
                is JobStart.Started -> finishJob(session, started.job, review, info, out)
            }
            if (report.status == Status.REFUSED) {
                // A refused job may have made an empty result folder. It is ours to remove.
                env.core.call(request("delete") { put("out", out.path) })
            }
            val seen = (_state.value.screen as? ScanState.Scanning)?.faces.orEmpty().associateBy { it.trackId }
            val faces = report.tracks.map { track -> faceOf(track, seen[track.trackId]) }
            return ScanState.Done(review.file.name, report, faces, motionPhoto = info.motionPhoto)
        } finally {
            // This scan's own mark only; another scan's stays.
            runCatching { env.marker.end(out) }
            runCatching { env.imports.delete(review.file) }
        }
    }

    private fun finishJob(session: Session, job: ScanJob, review: ScanState.Review, info: MediaInfo, out: File): Report {
        session.attach(job)
        val clean = try {
            env.reader.decode(review.file.file, info, job, session::stopRequested)
        } catch (t: Throwable) {
            false
        }
        return try {
            Report.from(job.finish(clean), out)
        } catch (t: Throwable) {
            runCatching { job.free() }
            failedReport(out)
        }
    }

    private fun failedReport(out: File?) =
        Report.incomplete(copy.line("incomplete", "Incomplete."), copy.line("not_fully_decoded"), incompleteDisclosure(), out)

    /** Events arrive on the scan thread. */
    private fun onEvent(event: JSONObject, out: File, startedAt: Long) {
        when (event.optString("event")) {
            "progress" -> {
                val frame = event.optLong("frame")
                val analyzed = event.optLong("frames_analyzed")
                val elapsed = env.now() - startedAt
                updateScanning { s ->
                    val done = frame + 1
                    val left = if (done > 0 && elapsed >= 2_000 && s.total > done) elapsed * (s.total - done) / done / 1000 else null
                    s.copy(frame = frame, analyzed = analyzed, secondsLeft = left)
                }
            }
            "face" -> {
                val id = event.optLong("track_id")
                val crop = event.text("crop")?.let { File(out, it).path }
                val frame = event.optLong("frame")
                updateScanning { s ->
                    s.copy(faces = upsert(s.faces, id) { old -> old?.copy(crop = crop ?: old.crop, revision = old.revision + 1) ?: Face(id, frame, crop) })
                }
            }
            "track" -> {
                val track = Track.from(event.optJSONObject("track") ?: return, out)
                updateScanning { s -> s.copy(faces = upsert(s.faces, track.trackId) { old -> faceOf(track, old) }) }
            }
        }
    }

    private fun updateScanning(change: (ScanState.Scanning) -> ScanState.Scanning) {
        _state.update { s -> (s.screen as? ScanState.Scanning)?.let { s.copy(screen = change(it)) } ?: s }
    }

    /** Stop the running scan. The result is Incomplete and says why. */
    fun stop(reason: String = Stop.CANCEL) {
        val running = session ?: return
        running.stop(reason)
        updateScanning { it.copy(stopping = true) }
    }

    // Result.

    fun openLeave(candidate: Candidate) {
        val url = candidate.poster.fbiUrl ?: return
        viewModelScope.launch {
            val answer = withContext(env.io) { env.core.call(request("leave") { put("url", url) }) }
            val approved = answer.text("url")
            if (answer.optBoolean("ok") && approved != null) {
                updateDone { it.copy(leave = Leave(answer.line("message"), approved, candidate.leaveNotes)) }
            } else {
                message(answer.line("headline"))
            }
        }
    }

    fun closeLeave() = updateDone { it.copy(leave = null) }

    fun deleteResult() {
        val done = _state.value.screen as? ScanState.Done ?: return
        val out = done.report.out
        if (out == null) {
            toStart()
            return
        }
        viewModelScope.launch {
            val answer = withContext(env.io) { env.core.call(request("delete") { put("out", out.path) }) }
            if (answer.optBoolean("ok")) {
                _state.update { it.copy(screen = ScanState.Start(), message = answer.line("headline")) }
                refreshPosters()
            } else {
                message(answer.line("headline"))
            }
        }
    }

    fun scanAnother() {
        toStart()
        viewModelScope.launch { refreshPosters() }
    }

    /** Back from Review or a result. During a scan the screen asks before calling [stop]. */
    fun back() {
        when (val screen = _state.value.screen) {
            is ScanState.Review -> toStart()
            is ScanState.Done -> toStart()
            is ScanState.Start -> if (screen.importing != null) cancelImport()
            is ScanState.Scanning -> Unit
        }
    }

    fun openFailed() = message(copy.line(Shell.OPEN_FAILED))

    fun messageShown() = _state.update { it.copy(message = null) }

    override fun onCleared() {
        session?.stop(Stop.CANCEL)
        super.onCleared()
    }

    // Helpers.

    private fun toStart() {
        probeJob?.cancel()
        estimateJob?.cancel()
        dropImport()
        _state.update { it.copy(screen = ScanState.Start()) }
    }

    private fun dropImport() {
        current?.let { env.imports.delete(it) }
        current = null
    }

    private fun updateDone(change: (ScanState.Done) -> ScanState.Done) {
        _state.update { s -> (s.screen as? ScanState.Done)?.let { s.copy(screen = change(it)) } ?: s }
    }

    private fun message(line: Line?) {
        if (line != null) _state.update { it.copy(message = line) }
    }

    private fun preselected(): String = bundles.firstOrNull { it.preselected }?.id ?: bundles.firstOrNull()?.id ?: "fast"

    /**
     * The disclosure for a report the core never wrote: the core's last one, without the clearance
     * line, which only a complete scan may show.
     */
    private fun incompleteDisclosure(): List<Line> = disclosure.filter { it.code != NO_CLEARANCE }

    private fun killedReport() = Report.incomplete(copy.line("incomplete", "Incomplete."), copy.line(Shell.SCAN_KILLED), incompleteDisclosure(), null)

    private fun scanRequest(op: String?, review: ScanState.Review, info: MediaInfo, out: File?): JSONObject = request(op) {
        put("input", review.file.file.path)
        put("bundles", env.paths.bundles.path)
        put("bundle", review.bundleId)
        put("posters", env.paths.posters.path)
        if (out != null) put("out", out.path)
        put("coverage", review.coverage)
        put("missing", review.missing)
        put("wanted", review.wanted)
        put("accelerate", true)
        put("device", deviceJson())
        review.file.mtimeUnix?.let { put("file_mtime_unix", it) }
        put("media", info.toJson())
    }

    private fun deviceJson(): JSONObject {
        val status = runCatching { env.device.status() }.getOrNull()
        return JSONObject().apply {
            put("kind", "phone")
            status?.thermal?.let { put("thermal", it) }
            status?.battery?.let { put("battery", it.toDouble()) }
            status?.charging?.let { put("charging", it) }
        }
    }

    private fun newResultDir(): File {
        val stamp = SimpleDateFormat("yyyy-MM-dd'T'HH-mm-ss'Z'", Locale.US).apply { timeZone = TimeZone.getTimeZone("UTC") }.format(Date(env.now()))
        var dir = File(env.paths.results, stamp)
        var n = 1
        while (dir.exists()) dir = File(env.paths.results, "$stamp-${n++}")
        return dir
    }

    /** Stop requests from any thread, carried to the job once it exists. */
    private class Session {
        private var job: ScanJob? = null
        private var reason: String? = null

        @Synchronized
        fun attach(job: ScanJob) {
            this.job = job
            reason?.let { job.stop(it) }
        }

        @Synchronized
        fun stop(why: String) {
            if (reason != null) return
            reason = why
            job?.stop(why)
        }

        @Synchronized
        fun stopRequested(): Boolean = reason != null
    }

    companion object {
        /** The disclosure line only a complete scan may show. */
        private const val NO_CLEARANCE = "no_clearance"

        fun factory(env: AppEnv): ViewModelProvider.Factory = viewModelFactory {
            initializer { ScanViewModel(env) }
        }

        private fun faceOf(track: Track, old: Face?) = Face(
            trackId = track.trackId,
            frame = track.firstFrame,
            crop = track.crop ?: old?.crop,
            revision = (old?.revision ?: 0) + 1,
            status = track.status,
            label = track.label,
            reason = track.reason,
            frames = track.frames,
        )

        private fun upsert(faces: List<Face>, id: Long, change: (Face?) -> Face): List<Face> {
            val index = faces.indexOfFirst { it.trackId == id }
            if (index < 0) return faces + change(null)
            return faces.toMutableList().also { it[index] = change(faces[index]) }
        }
    }
}
