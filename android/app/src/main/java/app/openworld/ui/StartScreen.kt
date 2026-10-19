// SPDX-License-Identifier: Apache-2.0
@file:OptIn(ExperimentalLayoutApi::class)

package app.openworld.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.AddPhotoAlternate
import androidx.compose.material.icons.outlined.CloudDownload
import androidx.compose.material.icons.outlined.CloudOff
import androidx.compose.material.icons.outlined.ErrorOutline
import androidx.compose.material.icons.outlined.FolderOpen
import androidx.compose.material.icons.outlined.Lock
import androidx.compose.material.icons.outlined.NoPhotography
import androidx.compose.material.icons.outlined.PhotoLibrary
import androidx.compose.material.icons.outlined.Science
import androidx.compose.material.icons.outlined.Shield
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedCard
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import app.openworld.R
import app.openworld.core.PosterPack
import app.openworld.ui.theme.OpenWorldColors
import java.time.Duration
import java.time.Instant

@Composable
fun StartScreen(
    state: UiState,
    start: ScanState.Start,
    snackbar: SnackbarHostState,
    onPhotos: () -> Unit,
    onFiles: () -> Unit,
    onCancelImport: () -> Unit,
    onInstallDemo: () -> Unit,
    onUpdatePosters: () -> Unit,
) {
    Scaffold(snackbarHost = { SnackbarHost(snackbar) }) { padding ->
        Box(modifier = Modifier.fillMaxSize().padding(padding), contentAlignment = Alignment.TopCenter) {
            Column(
                modifier = Modifier
                    .widthIn(max = 640.dp)
                    .fillMaxWidth()
                    .verticalScroll(rememberScrollState())
                    .padding(horizontal = 16.dp, vertical = 24.dp),
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                Text(
                    text = stringResource(R.string.app_name),
                    style = MaterialTheme.typography.displaySmall,
                    modifier = Modifier.semantics { heading() },
                )
                Text(
                    text = stringResource(R.string.start_subtitle),
                    style = MaterialTheme.typography.bodyLarge,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                state.notAffiliated?.let { line ->
                    Text(
                        text = text(line),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                ChooseCard(start.importing, state.ready, onPhotos, onFiles, onCancelImport)
                start.notice?.let { notice ->
                    ToneCard(tone = OpenWorldColors.status.error, icon = Icons.Outlined.ErrorOutline) {
                        Text(text(notice), style = MaterialTheme.typography.bodyMedium)
                    }
                }
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    TrustChip(Icons.Outlined.Lock, stringResource(R.string.trust_on_device))
                    TrustChip(Icons.Outlined.CloudOff, stringResource(R.string.trust_nothing_uploaded))
                    TrustChip(Icons.Outlined.NoPhotography, stringResource(R.string.trust_no_camera))
                }
                Spacer(Modifier.height(8.dp))
                PosterFooter(state, onInstallDemo, onUpdatePosters)
                Text(
                    text = stringResource(R.string.results_kept),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
    }
}

@Composable
private fun ChooseCard(importing: Importing?, ready: Boolean, onPhotos: () -> Unit, onFiles: () -> Unit, onCancel: () -> Unit) {
    OutlinedCard(modifier = Modifier.fillMaxWidth()) {
        Column(
            modifier = Modifier.fillMaxWidth().padding(24.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            Surface(shape = CircleShape, color = MaterialTheme.colorScheme.primaryContainer, contentColor = MaterialTheme.colorScheme.onPrimaryContainer) {
                Icon(Icons.Outlined.AddPhotoAlternate, contentDescription = null, modifier = Modifier.padding(16.dp).size(32.dp))
            }
            if (importing != null) {
                val progress = importing.progress
                Text(
                    text = if (progress != null) {
                        stringResource(R.string.start_copying_percent, (progress * 100).toInt())
                    } else {
                        stringResource(R.string.start_copying)
                    },
                    style = MaterialTheme.typography.bodyLarge,
                    textAlign = TextAlign.Center,
                )
                if (progress != null) {
                    LinearProgressIndicator(progress = { progress }, modifier = Modifier.fillMaxWidth())
                } else {
                    LinearProgressIndicator(modifier = Modifier.fillMaxWidth())
                }
                TextButton(onClick = onCancel) { Text(stringResource(R.string.cancel)) }
            } else {
                Button(onClick = onPhotos, modifier = Modifier.fillMaxWidth().heightIn(min = 56.dp)) {
                    Icon(Icons.Outlined.PhotoLibrary, contentDescription = null, modifier = Modifier.size(20.dp))
                    Spacer(Modifier.size(8.dp))
                    Text(stringResource(R.string.start_choose))
                }
                OutlinedButton(onClick = onFiles, modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp)) {
                    Icon(Icons.Outlined.FolderOpen, contentDescription = null, modifier = Modifier.size(20.dp))
                    Spacer(Modifier.size(8.dp))
                    Text(stringResource(R.string.start_files))
                }
                if (!ready) {
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        CircularProgressIndicator(modifier = Modifier.size(16.dp), strokeWidth = 2.dp)
                        Text(stringResource(R.string.start_getting_ready), style = MaterialTheme.typography.bodySmall)
                    }
                } else {
                    Text(
                        text = stringResource(R.string.start_card_hint),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        textAlign = TextAlign.Center,
                    )
                }
            }
        }
    }
}

@Composable
private fun TrustChip(icon: ImageVector, label: String) {
    Surface(
        shape = MaterialTheme.shapes.small,
        color = MaterialTheme.colorScheme.secondaryContainer,
        contentColor = MaterialTheme.colorScheme.onSecondaryContainer,
    ) {
        Row(
            modifier = Modifier.heightIn(min = 32.dp).padding(horizontal = 12.dp, vertical = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Icon(icon, contentDescription = null, modifier = Modifier.size(18.dp))
            Text(label, style = MaterialTheme.typography.labelLarge)
        }
    }
}

@Composable
private fun PosterFooter(state: UiState, onInstallDemo: () -> Unit, onUpdatePosters: () -> Unit) {
    val pack = state.posters
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            if (pack != null) {
                if (pack.demo) {
                    Badge(stringResource(R.string.posters_demo_badge), OpenWorldColors.status.demo, Icons.Outlined.Science)
                } else {
                    Icon(Icons.Outlined.Shield, contentDescription = null, tint = MaterialTheme.colorScheme.onSurfaceVariant)
                }
                Text(
                    text = packLine(pack),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            } else {
                Icon(Icons.Outlined.Shield, contentDescription = null, tint = MaterialTheme.colorScheme.onSurfaceVariant)
                Text(
                    text = stringResource(R.string.posters_none),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
        // An expired or unreadable pack says why.
        val why = state.postersRefusal
        if (pack == null && why != null && why.code != "missing_pack") {
            Text(text(why), style = MaterialTheme.typography.bodySmall, color = OpenWorldColors.status.error.content)
        }
        val update = state.update
        val updating = update is PosterUpdate.Running
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            if (pack == null && state.demoBuild && state.ready) {
                OutlinedButton(onClick = onInstallDemo, enabled = !updating) {
                    Icon(Icons.Outlined.Science, contentDescription = null, modifier = Modifier.size(18.dp))
                    Spacer(Modifier.size(8.dp))
                    Text(stringResource(R.string.posters_install_demo))
                }
            }
            if (state.ready) {
                OutlinedButton(onClick = onUpdatePosters, enabled = !updating) {
                    Icon(Icons.Outlined.CloudDownload, contentDescription = null, modifier = Modifier.size(18.dp))
                    Spacer(Modifier.size(8.dp))
                    Text(stringResource(R.string.posters_update))
                }
            }
        }
        UpdateStatus(update)
    }
}

/** Update posters' progress, then what it kept and left out, or its refusal, in quiet type. */
@Composable
private fun UpdateStatus(update: PosterUpdate) {
    val style = MaterialTheme.typography.bodySmall
    val color = MaterialTheme.colorScheme.onSurfaceVariant
    when (update) {
        PosterUpdate.Idle -> Unit
        is PosterUpdate.Running -> {
            val pages = update.pages
            Text(
                text = when {
                    update.posters > 0 -> stringResource(R.string.posters_update_checking, update.checked, update.posters)
                    update.page == 0 -> stringResource(R.string.posters_update_reading)
                    pages != null -> stringResource(R.string.posters_update_page_of, update.page, pages)
                    else -> stringResource(R.string.posters_update_page, update.page)
                },
                style = style,
                color = color,
            )
            if (update.posters > 0) {
                LinearProgressIndicator(progress = { update.checked.toFloat() / update.posters }, modifier = Modifier.fillMaxWidth())
            } else {
                LinearProgressIndicator(modifier = Modifier.fillMaxWidth())
            }
        }
        is PosterUpdate.Done -> {
            Text(pluralStringResource(R.plurals.posters_update_kept, update.kept.toInt(), update.kept), style = style, color = color)
            update.skipped.forEach { row ->
                Text(stringResource(R.string.posters_update_skipped, row.count, text(row.reason)), style = style, color = color)
            }
        }
        is PosterUpdate.Refused -> Text(text(update.line), style = style, color = color)
    }
}

/** "2 posters · fetched today". */
@Composable
private fun packLine(pack: PosterPack): String {
    val count = pack.missing + pack.wanted
    val posters = pluralStringResource(R.plurals.posters_count, count, count)
    val days = runCatching { Duration.between(Instant.parse(pack.createdAt), Instant.now()).toDays() }.getOrNull()
    val fetched = when {
        days == null -> return posters
        days <= 0L -> stringResource(R.string.posters_fetched_today)
        days == 1L -> stringResource(R.string.posters_fetched_yesterday)
        else -> pluralStringResource(R.plurals.posters_fetched_days, days.toInt(), days.toInt())
    }
    return stringResource(R.string.scan_join, posters, fetched)
}
