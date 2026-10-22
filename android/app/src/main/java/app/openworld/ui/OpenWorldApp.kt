// SPDX-License-Identifier: Apache-2.0
package app.openworld.ui

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.lifecycle.compose.collectAsStateWithLifecycle

/** First run, then one flow, three screens (docs/ux.md). About and the reading sheet sit beside them. */
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
    val terms = state.terms
    BackHandler(enabled = screen is ScanState.Review || screen is ScanState.Done ||
        (screen is ScanState.Start && screen.importing != null)) { model.back() }
    BackHandler(enabled = state.about) { model.closeAbout() }
    // Back on First run leaves the app on it: acceptance is never skipped.
    BackHandler(enabled = terms !is Terms.Accepted) {}

    state.reading?.let { ReadingSheet(it, onDismiss = model::closeDocument) }
    when {
        // Nothing shows before the core names the version to accept.
        terms is Terms.Checking -> Surface(modifier = Modifier.fillMaxSize()) {}
        terms is Terms.FirstRun -> FirstRunScreen(state.legal, terms.changed, null, model::openDocument, model::acceptTerms)
        terms is Terms.Refused -> FirstRunScreen(null, false, terms.headline, model::openDocument, model::acceptTerms)
        state.about -> AboutScreen(state.legal, onBack = model::closeAbout, onRead = model::openDocument)
        else -> Screens(model, state, snackbar, onPhotos, onFiles, onOpen)
    }
}

@Composable
private fun Screens(model: ScanViewModel, state: UiState, snackbar: SnackbarHostState, onPhotos: () -> Unit, onFiles: () -> Unit, onOpen: (String) -> Unit) {
    AnimatedContent(
        targetState = state.screen,
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
                onUpdatePosters = model::updatePosters,
                onAbout = model::openAbout,
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
