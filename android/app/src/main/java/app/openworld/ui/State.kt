// SPDX-License-Identifier: Apache-2.0
package app.openworld.ui

import app.openworld.core.BundleRow
import app.openworld.core.Estimate
import app.openworld.core.Line
import app.openworld.core.PosterPack
import app.openworld.core.Report
import app.openworld.core.TrackStatus
import app.openworld.media.MediaInfo
import app.openworld.platform.Imported
import java.io.File

data class UiState(
    /** Bundles installed and the core answered. */
    val ready: Boolean = false,
    /** This build can write the demo poster pack. */
    val demoBuild: Boolean = false,
    val posters: PosterPack? = null,
    /** Why there is no usable pack, when there is none. */
    val postersRefusal: Line? = null,
    val screen: ScanState = ScanState.Start(),
    /** A one-off notice for the snackbar. */
    val message: Line? = null,
)

/** The flow: Start, Review, Scanning, Done. Exactly one at a time. */
sealed interface ScanState {
    data class Start(val importing: Importing? = null, val notice: Line? = null) : ScanState

    data class Review(
        val file: Imported,
        val media: MediaState = MediaState.Probing,
        val missing: Boolean = true,
        val wanted: Boolean = true,
        val bundles: List<BundleRow> = emptyList(),
        val bundleId: String = "fast",
        val bundleNote: Line? = null,
        val coverage: String = COMPLETE,
        val estimate: EstimateState = EstimateState.Loading,
        /** Age and timestamp cautions for the file card, from the core's estimate. */
        val warnings: List<Line> = emptyList(),
        /** Every disclosure line, from the core's estimate, for the "What happens to this file" card. */
        val disclosure: List<Line> = emptyList(),
    ) : ScanState {
        val bothOff: Boolean get() = !missing && !wanted

        val canAnalyze: Boolean
            get() = media is MediaState.Ready && !bothOff && estimate is EstimateState.Ready && estimate.value.canStart
    }

    data class Scanning(
        val fileName: String,
        val info: MediaInfo,
        val out: File,
        /** The last frame the core reported, counting from 0. */
        val frame: Long = -1,
        val analyzed: Long = 0,
        val secondsLeft: Long? = null,
        val faces: List<Face> = emptyList(),
        val coverageBanner: Line? = null,
        val stopping: Boolean = false,
    ) : ScanState {
        val total: Long get() = info.frames
    }

    data class Done(
        val fileName: String,
        val report: Report,
        val faces: List<Face> = emptyList(),
        val leave: Leave? = null,
        val motionPhoto: Boolean = false,
    ) : ScanState

    companion object {
        const val COMPLETE = "complete"
        const val MEASURED = "measured"
    }
}

data class Importing(val name: String?, val progress: Float?)

sealed interface MediaState {
    data object Probing : MediaState

    data class Ready(val info: MediaInfo) : MediaState

    data class Refused(val line: Line) : MediaState
}

sealed interface EstimateState {
    data object Loading : EstimateState

    data class Ready(val value: Estimate) : EstimateState

    data class Refused(val line: Line) : EstimateState
}

/** A face as the grid shows it. `status` is null while the core is still looking. */
data class Face(
    val trackId: Long,
    val frame: Long,
    val crop: String?,
    /** Bumped when the core rewrites the crop file, so the image reloads. */
    val revision: Int = 0,
    val status: TrackStatus? = null,
    val label: Line? = null,
    val reason: Line? = null,
    val frames: Line? = null,
)

/** The leaving sheet, for the one URL the core's `leave` call approved. */
data class Leave(val message: Line?, val url: String, val notes: List<Line>)
