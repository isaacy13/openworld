// SPDX-License-Identifier: Apache-2.0
package app.openworld.ui

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.material3.SnackbarHostState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.lifecycle.compose.collectAsStateWithLifecycle

/** One flow, three screens (docs/ux.md). */
@Composable
fun OpenWorldApp(model: ScanViewModel, onPhotos: () -> Unit, onFiles: () -> Unit, onOpen: (String) -> Unit) {
    val state by model.state.collectAsStateWithLifecycle()
    val snackbar = remember { SnackbarHostState() }
    val message = state.message?.let { text(it) }
    LaunchedEffect(state.message) {
        if (message != null) {
            snackbar.showSnackbar(message)
            model.messageShown()
        }
    }

    val screen = state.screen
    BackHandler(enabled = screen is ScanState.Review || screen is ScanState.Done ||
        (screen is ScanState.Start && screen.importing != null)) { model.back() }

    AnimatedContent(
        targetState = screen,
        contentKey = { it::class },
        transitionSpec = { fadeIn() togetherWith fadeOut() },
        label = "screen",
    ) { shown ->
        when (shown) {
            is ScanState.Start -> StartScreen(
                state = state,
                start = shown,
                snackbar = snackbar,
                onPhotos = onPhotos,
                onFiles = onFiles,
                onCancelImport = model::cancelImport,
                onInstallDemo = model::installDemoPosters,
            )
            is ScanState.Review -> ReviewScreen(
                review = shown,
                posters = state.posters,
                snackbar = snackbar,
                onBack = model::back,
                onMissing = model::setMissing,
                onWanted = model::setWanted,
                onBundle = model::setBundle,
                onCoverage = model::setCoverage,
                onAnalyze = model::analyze,
                onChooseAnother = onPhotos,
            )
            is ScanState.Scanning -> ScanningScreen(scan = shown, snackbar = snackbar, onStop = { model.stop() })
            is ScanState.Done -> ResultScreen(
                done = shown,
                snackbar = snackbar,
                onBack = model::back,
                onLeave = model::openLeave,
                onCloseLeave = model::closeLeave,
                onOpen = { url ->
                    model.closeLeave()
                    onOpen(url)
                },
                onDelete = model::deleteResult,
                onScanAnother = model::scanAnother,
            )
        }
    }
}
