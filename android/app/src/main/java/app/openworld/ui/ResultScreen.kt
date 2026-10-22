// SPDX-License-Identifier: Apache-2.0
@file:OptIn(ExperimentalMaterial3Api::class, ExperimentalLayoutApi::class)

package app.openworld.ui

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.calculateEndPadding
import androidx.compose.foundation.layout.calculateStartPadding
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyGridItemScope
import androidx.compose.foundation.lazy.grid.LazyGridScope
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.ArrowBack
import androidx.compose.material.icons.automirrored.outlined.OpenInNew
import androidx.compose.material.icons.outlined.CheckCircle
import androidx.compose.material.icons.outlined.DeleteOutline
import androidx.compose.material.icons.outlined.ErrorOutline
import androidx.compose.material.icons.outlined.ExpandMore
import androidx.compose.material.icons.outlined.Face
import androidx.compose.material.icons.outlined.HideSource
import androidx.compose.material.icons.outlined.Info
import androidx.compose.material.icons.outlined.Lock
import androidx.compose.material.icons.outlined.PersonSearch
import androidx.compose.material.icons.outlined.Science
import androidx.compose.material.icons.outlined.Search
import androidx.compose.material.icons.outlined.Timer
import androidx.compose.material.icons.outlined.WarningAmber
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.AssistChip
import androidx.compose.material3.AssistChipDefaults
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedCard
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.animation.core.animateFloatAsState
import app.openworld.R
import app.openworld.core.Candidate
import app.openworld.core.Line
import app.openworld.core.Report
import app.openworld.core.Status
import app.openworld.core.TrackStatus
import app.openworld.ui.theme.OpenWorldColors
import app.openworld.ui.theme.Tone
import java.text.NumberFormat

// Scanning.

@Composable
fun ScanningScreen(scan: ScanState.Scanning, snackbar: SnackbarHostState, onStop: () -> Unit) {
    var askStop by rememberSaveable { mutableStateOf(false) }
    BackHandler { askStop = true }
    Scaffold(
        topBar = { TopAppBar(title = { Text(stringResource(R.string.scanning_title)) }) },
        bottomBar = {
            Surface(tonalElevation = 3.dp) {
                OutlinedButton(
                    onClick = onStop,
                    enabled = !scan.stopping,
                    modifier = Modifier
                        .fillMaxWidth()
                        .navigationBarsPadding()
                        .padding(horizontal = 16.dp, vertical = 12.dp)
                        .heightIn(min = 52.dp),
                ) {
                    Text(stringResource(if (scan.stopping) R.string.stopping else R.string.stop))
                }
            }
        },
        snackbarHost = { SnackbarHost(snackbar) },
    ) { padding ->
        FaceGrid(padding) {
            full { ProgressCard(scan) }
            scan.coverageBanner?.let { banner ->
                full { ToneCard(OpenWorldColors.status.candidate, Icons.Outlined.WarningAmber) { Text(text(banner)) } }
            }
            full {
                Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    SectionTitle(stringResource(R.string.scan_faces_title))
                    if (scan.faces.isEmpty()) {
                        Text(
                            stringResource(R.string.scan_faces_empty),
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
            }
            items(scan.faces, key = { it.trackId }) { face -> FaceCell(face, showReason = false) }
        }
    }
    if (askStop) {
        AlertDialog(
            onDismissRequest = { askStop = false },
            title = { Text(stringResource(R.string.stop_ask_title)) },
            text = { Text(stringResource(R.string.stop_ask_text)) },
            confirmButton = {
                TextButton(onClick = {
                    askStop = false
                    onStop()
                }) { Text(stringResource(R.string.stop)) }
            },
            dismissButton = { TextButton(onClick = { askStop = false }) { Text(stringResource(R.string.stop_ask_keep)) } },
        )
    }
}

@Composable
private fun ProgressCard(scan: ScanState.Scanning) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow),
    ) {
        Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Icon(Icons.Outlined.Lock, contentDescription = null, tint = MaterialTheme.colorScheme.primary)
                Text(stringResource(R.string.scanning_on_device), style = MaterialTheme.typography.titleMedium)
            }
            val done = scan.frame + 1
            if (scan.info.video && scan.total > 0 && done > 0) {
                val fraction = (done.toFloat() / scan.total).coerceIn(0f, 1f)
                LinearProgressIndicator(progress = { fraction }, modifier = Modifier.fillMaxWidth())
            } else {
                LinearProgressIndicator(modifier = Modifier.fillMaxWidth())
            }
            Text(
                text = progressText(scan),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite },
            )
        }
    }
}

/** "Frame 412 of 1,800 · about 1 minute left". */
@Composable
private fun progressText(scan: ScanState.Scanning): String {
    if (!scan.info.video) return stringResource(R.string.scan_photo)
    if (scan.frame < 0) return stringResource(R.string.scan_starting)
    val numbers = NumberFormat.getIntegerInstance()
    val frames = stringResource(R.string.scan_frame_of, numbers.format(scan.frame + 1), numbers.format(scan.total))
    val left = scan.secondsLeft ?: return frames
    val time = when {
        left < 60 -> stringResource(R.string.scan_less_than_minute)
        left < 3600 -> ((left + 30) / 60).toInt().let { pluralStringResource(R.plurals.scan_minutes_left, it, it) }
        else -> ((left + 1800) / 3600).toInt().let { pluralStringResource(R.plurals.scan_hours_left, it, it) }
    }
    return stringResource(R.string.scan_join, frames, time)
}

// Result.

@Composable
fun ResultScreen(
    done: ScanState.Done,
    snackbar: SnackbarHostState,
    onBack: () -> Unit,
    onLeave: (Candidate) -> Unit,
    onCloseLeave: () -> Unit,
    onOpen: (String) -> Unit,
    onDelete: () -> Unit,
    onScanAnother: () -> Unit,
) {
    val report = done.report
    var askDelete by rememberSaveable { mutableStateOf(false) }
    var frameOf by rememberSaveable { mutableLongStateOf(-1L) }
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(stringResource(R.string.result_title)) },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Outlined.ArrowBack, contentDescription = stringResource(R.string.back))
                    }
                },
            )
        },
        snackbarHost = { SnackbarHost(snackbar) },
    ) { padding ->
        val others = done.faces.filter { it.status != TrackStatus.CANDIDATE }
        FaceGrid(padding) {
            full { HeadlineCard(report) }
            report.coverageBanner?.let { banner ->
                full { ToneCard(OpenWorldColors.status.candidate, Icons.Outlined.WarningAmber) { Text(text(banner)) } }
            }
            if (done.motionPhoto) {
                full { ToneCard(OpenWorldColors.status.incomplete, Icons.Outlined.WarningAmber) { Text(stringResource(R.string.motion_photo_note)) } }
            }
            report.candidates.forEach { candidate ->
                full { CandidateCard(candidate, onShowFrame = { frameOf = candidate.trackId }, onLeave = { onLeave(candidate) }) }
            }
            if (report.status != Status.REFUSED) full { DetailsCard(report) }
            if (others.isNotEmpty()) full { SectionTitle(stringResource(R.string.other_faces_title)) }
            items(others, key = { it.trackId }) { face -> FaceCell(face, showReason = true) }
            if (report.disclosure.isNotEmpty()) full { DisclosureCard(report.disclosure, onReadAll = null, all = true) }
            full {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.padding(top = 8.dp)) {
                    Button(onClick = onScanAnother, modifier = Modifier.fillMaxWidth().heightIn(min = 52.dp)) {
                        Text(stringResource(R.string.scan_another))
                    }
                    if (report.out != null) {
                        OutlinedButton(
                            onClick = { askDelete = true },
                            modifier = Modifier.fillMaxWidth().heightIn(min = 52.dp),
                            colors = ButtonDefaults.outlinedButtonColors(contentColor = MaterialTheme.colorScheme.error),
                            border = BorderStroke(1.dp, MaterialTheme.colorScheme.error),
                        ) {
                            Icon(Icons.Outlined.DeleteOutline, contentDescription = null, modifier = Modifier.size(20.dp))
                            Spacer(Modifier.size(8.dp))
                            Text(stringResource(R.string.delete_result))
                        }
                    }
                }
            }
        }
    }

    report.candidates.firstOrNull { it.trackId == frameOf }?.let { candidate ->
        FrameDialog(candidate, onDismiss = { frameOf = -1L })
    }
    done.leave?.let { leave -> LeaveSheet(leave, onOpen = onOpen, onStay = onCloseLeave) }
    if (askDelete) {
        AlertDialog(
            onDismissRequest = { askDelete = false },
            icon = { Icon(Icons.Outlined.DeleteOutline, contentDescription = null) },
            title = { Text(stringResource(R.string.delete_ask_title)) },
            text = { Text(stringResource(R.string.delete_ask_text)) },
            confirmButton = {
                TextButton(onClick = {
                    askDelete = false
                    onDelete()
                }) { Text(stringResource(R.string.delete), color = MaterialTheme.colorScheme.error) }
            },
            dismissButton = { TextButton(onClick = { askDelete = false }) { Text(stringResource(R.string.cancel)) } },
        )
    }
}

/** The headline card, by state. Status is shown by icon and words, never by color alone. */
@Composable
private fun HeadlineCard(report: Report) {
    val status = OpenWorldColors.status
    val (tone, icon) = when {
        report.status == Status.REFUSED -> status.error to Icons.Outlined.ErrorOutline
        report.status == Status.INCOMPLETE -> status.incomplete to Icons.Outlined.WarningAmber
        report.candidates.isNotEmpty() -> status.candidate to Icons.Outlined.PersonSearch
        else -> status.neutral to Icons.Outlined.Info
    }
    ToneCard(tone, icon, Modifier.semantics { liveRegion = LiveRegionMode.Polite }) {
        Text(
            text = text(report.headline),
            style = MaterialTheme.typography.headlineSmall,
            modifier = Modifier.semantics { heading() },
        )
        when {
            report.status == Status.REFUSED -> Text(stringResource(refusalHint(report.refusal)), style = MaterialTheme.typography.bodyMedium)
            report.status == Status.INCOMPLETE -> report.reason?.let { Text(text(it), style = MaterialTheme.typography.bodyLarge) }
            report.candidates.isNotEmpty() -> Text(
                pluralStringResource(R.plurals.candidates_count, report.candidates.size, report.candidates.size),
                style = MaterialTheme.typography.bodyLarge,
            )
            else -> report.facesLine?.let { Text(text(it), style = MaterialTheme.typography.bodyLarge) }
        }
        report.perceptionNote?.let { note ->
            Spacer(Modifier.size(4.dp))
            Badge(text(note), status.demo, Icons.Outlined.Science)
        }
    }
}

private fun refusalHint(code: String?): Int = when (code) {
    "missing_pack", "expired_pack", "old_pack", "future_pack", "bad_hash", "unreadable_pack", "bad_vectors",
    "embedder_mismatch", "unknown_perception", "fixture_off" -> R.string.hint_posters
    "bad_codec", "unreadable", "too_large", "no_fps", "refuse_animated", "refuse_hdr" -> R.string.hint_file
    "runtime", "missing_weights" -> R.string.hint_runtime
    else -> R.string.hint_default
}

@Composable
private fun CandidateCard(candidate: Candidate, onShowFrame: () -> Unit, onLeave: () -> Unit) {
    val tone = OpenWorldColors.status.candidate
    OutlinedCard(
        modifier = Modifier.fillMaxWidth(),
        border = BorderStroke(2.dp, tone.content),
        colors = CardDefaults.outlinedCardColors(containerColor = MaterialTheme.colorScheme.surface),
    ) {
        Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Icon(Icons.Outlined.PersonSearch, contentDescription = null, tint = tone.content)
                Text(text(candidate.headline), style = MaterialTheme.typography.titleSmall, color = tone.content)
            }
            val frameNumber = candidate.frameIndex + 1
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                Evidence(
                    label = stringResource(R.string.evidence_file),
                    path = candidate.crop,
                    description = stringResource(R.string.face_description, frameNumber.toInt(), stringResource(R.string.chip_candidate)),
                    modifier = Modifier.weight(1f),
                )
                Evidence(
                    label = stringResource(R.string.evidence_poster),
                    path = candidate.poster.image,
                    description = stringResource(R.string.poster_description, candidate.poster.title),
                    modifier = Modifier.weight(1f),
                )
            }
            Text(candidate.poster.title, style = MaterialTheme.typography.titleLarge)
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Badge(candidate.poster.classLabel, tone)
            }
            candidate.band?.let { Text(text(it), style = MaterialTheme.typography.bodyLarge) }
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    text(candidate.frameLabel),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.weight(1f),
                )
                if (candidate.frame != null) {
                    TextButton(onClick = onShowFrame, modifier = Modifier.heightIn(min = 48.dp)) {
                        Text(stringResource(R.string.show_frame))
                    }
                }
            }
            Button(
                onClick = onLeave,
                enabled = candidate.poster.fbiUrl != null,
                modifier = Modifier.fillMaxWidth().heightIn(min = 52.dp),
            ) {
                Icon(Icons.AutoMirrored.Outlined.OpenInNew, contentDescription = null, modifier = Modifier.size(20.dp))
                Spacer(Modifier.size(8.dp))
                Text(stringResource(R.string.open_fbi_page))
            }
            if (candidate.poster.fbiUrl == null) {
                Text(stringResource(R.string.no_fbi_page), style = MaterialTheme.typography.bodySmall)
            }
        }
    }
}

@Composable
private fun Evidence(label: String, path: String?, description: String, modifier: Modifier = Modifier) {
    Column(modifier = modifier, verticalArrangement = Arrangement.spacedBy(6.dp)) {
        // Demo poster markers are a few pixels a module; nearest-neighbor keeps their edges.
        SquareImage(rememberFileImage(path, maxPx = 768), description, Modifier.fillMaxWidth().aspectRatio(1f), crisp = true)
        Text(label, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

@Composable
private fun FrameDialog(candidate: Candidate, onDismiss: () -> Unit) {
    Dialog(onDismissRequest = onDismiss) {
        Surface(shape = MaterialTheme.shapes.extraLarge, tonalElevation = 6.dp) {
            Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Text(text(candidate.frameLabel), style = MaterialTheme.typography.titleMedium, modifier = Modifier.semantics { heading() })
                val image = rememberFileImage(candidate.frame, maxPx = 2048)
                Box(modifier = Modifier.fillMaxWidth().heightIn(min = 120.dp), contentAlignment = Alignment.Center) {
                    if (image != null) {
                        Image(
                            bitmap = image,
                            contentDescription = stringResource(R.string.frame_description),
                            contentScale = ContentScale.Fit,
                            modifier = Modifier.fillMaxWidth(),
                        )
                    }
                }
                TextButton(onClick = onDismiss, modifier = Modifier.align(Alignment.End)) { Text(stringResource(R.string.close)) }
            }
        }
    }
}

/** "You are leaving OpenWorld." Only the URL the core approved opens. */
@Composable
private fun LeaveSheet(leave: Leave, onOpen: (String) -> Unit, onStay: () -> Unit) {
    ModalBottomSheet(onDismissRequest = onStay, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 24.dp)
                .padding(bottom = 24.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Icon(Icons.AutoMirrored.Outlined.OpenInNew, contentDescription = null, tint = MaterialTheme.colorScheme.primary)
            Text(text(leave.message), style = MaterialTheme.typography.headlineSmall, modifier = Modifier.semantics { heading() })
            leave.notes.forEach { IconLine(Icons.Outlined.Info, text(it)) }
            Text(leave.url, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            Button(onClick = { onOpen(leave.url) }, modifier = Modifier.fillMaxWidth().heightIn(min = 52.dp)) {
                Text(stringResource(R.string.open_fbi_page))
            }
            OutlinedButton(onClick = onStay, modifier = Modifier.fillMaxWidth().heightIn(min = 52.dp)) {
                Text(stringResource(R.string.stay))
            }
        }
    }
}

@Composable
private fun DetailsCard(report: Report) {
    var open by rememberSaveable { mutableStateOf(false) }
    val turn by animateFloatAsState(if (open) 180f else 0f, label = "chevron")
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow),
    ) {
        Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .heightIn(min = 48.dp)
                    .selectable(selected = open, role = Role.Button, onClick = { open = !open }),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                SectionTitle(stringResource(R.string.details_title), Modifier.weight(1f))
                Icon(Icons.Outlined.ExpandMore, contentDescription = null, modifier = Modifier.rotate(turn))
            }
            if (open) {
                SelectionContainer {
                    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                        Detail(R.string.details_frames, report.framesLine)
                        Detail(R.string.details_coverage, report.coverageLine)
                        Detail(R.string.details_detection, report.detectionLine)
                        Detail(R.string.details_chance, report.chance)
                        Detail(R.string.details_classes, report.classes)
                        DetailText(
                            R.string.details_bundle,
                            listOf("${report.bundleName} ${report.bundleVersion}".trim()),
                            hashes = report.weights.map { "${it.role}: ${it.name} · SHA-256 ${it.sha256}" },
                        )
                        report.posters?.let { pack ->
                            DetailText(
                                R.string.details_posters,
                                listOf(pack.id, stringResource(R.string.details_created, pack.createdAt, pack.expiresAt)),
                                hashes = listOf("SHA-256 ${pack.sha256}"),
                            )
                        }
                        DetailText(R.string.details_device, listOfNotNull(report.provider.ifEmpty { null }, report.deviceNote?.let { text(it) }))
                        if (report.warnings.isNotEmpty()) DetailText(R.string.details_warnings, report.warnings.map { text(it) })
                        if (report.expiresAt.isNotEmpty()) {
                            Text(stringResource(R.string.details_expires, report.expiresAt), style = MaterialTheme.typography.bodySmall)
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun Detail(label: Int, line: Line?) {
    if (line != null) DetailText(label, listOf(text(line)))
}

/** A labelled detail. `hashes` follow `values` in a monospace face. */
@Composable
private fun DetailText(label: Int, values: List<String>, hashes: List<String> = emptyList()) {
    if ((values + hashes).all { it.isBlank() }) return
    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Text(stringResource(label), style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.onSurfaceVariant)
        values.filter { it.isNotBlank() }.forEach { Text(it, style = MaterialTheme.typography.bodyMedium) }
        hashes.filter { it.isNotBlank() }.forEach {
            Text(it, style = MaterialTheme.typography.bodySmall.copy(fontFamily = FontFamily.Monospace))
        }
    }
}

// Faces.

/** The scan and result screens are one grid: full-width cards, then face cells. */
@Composable
private fun FaceGrid(padding: PaddingValues, content: LazyGridScope.() -> Unit) {
    LazyVerticalGrid(
        columns = GridCells.Adaptive(minSize = 112.dp),
        contentPadding = PaddingValues(
            start = padding.calculateStartPadding(LocalLayoutDirection.current) + 16.dp,
            end = padding.calculateEndPadding(LocalLayoutDirection.current) + 16.dp,
            top = padding.calculateTopPadding() + 8.dp,
            bottom = padding.calculateBottomPadding() + 24.dp,
        ),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
        modifier = Modifier.fillMaxWidth(),
        content = content,
    )
}

private fun LazyGridScope.full(content: @Composable LazyGridItemScope.() -> Unit) {
    item(span = { GridItemSpan(maxLineSpan) }, content = content)
}

private data class FaceLook(val tone: Tone, val icon: ImageVector, val label: Int)

@Composable
private fun lookOf(status: TrackStatus?): FaceLook {
    val colors = OpenWorldColors.status
    return when (status) {
        null -> FaceLook(colors.neutral, Icons.Outlined.Search, R.string.chip_looking)
        TrackStatus.CANDIDATE -> FaceLook(colors.candidate, Icons.Outlined.PersonSearch, R.string.chip_candidate)
        TrackStatus.BELOW_CUTOFF -> FaceLook(colors.neutral, Icons.Outlined.CheckCircle, R.string.chip_compared)
        TrackStatus.TOO_BRIEF -> FaceLook(colors.neutral, Icons.Outlined.Timer, R.string.chip_too_brief)
        TrackStatus.NOT_COMPARED -> FaceLook(colors.neutral, Icons.Outlined.HideSource, R.string.chip_not_compared)
        TrackStatus.UNSCORED -> FaceLook(colors.neutral, Icons.Outlined.HideSource, R.string.chip_unscored)
    }
}

@Composable
private fun LazyGridItemScope.FaceCell(face: Face, showReason: Boolean) {
    var details by rememberSaveable { mutableStateOf(false) }
    val look = lookOf(face.status)
    val label = stringResource(look.label)
    Column(modifier = Modifier.animateItem(), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        SquareImage(
            image = rememberFileImage(face.crop, face.revision, maxPx = 384),
            description = stringResource(R.string.face_description, (face.frame + 1).toInt(), label),
            modifier = Modifier.fillMaxWidth().aspectRatio(1f),
        )
        AssistChip(
            onClick = { details = true },
            label = { Text(label, maxLines = 1) },
            leadingIcon = { Icon(look.icon, contentDescription = null, modifier = Modifier.size(AssistChipDefaults.IconSize)) },
            colors = AssistChipDefaults.assistChipColors(
                containerColor = look.tone.container,
                labelColor = look.tone.content,
                leadingIconContentColor = look.tone.content,
            ),
            border = null,
            modifier = Modifier.fillMaxWidth(),
        )
        if (showReason) {
            (face.reason ?: face.label)?.let {
                Text(text(it), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
    }
    if (details) {
        AlertDialog(
            onDismissRequest = { details = false },
            icon = { Icon(Icons.Outlined.Face, contentDescription = null) },
            title = { Text(text(face.frames).ifEmpty { label }) },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(if (face.label != null) text(face.label) else label)
                    face.reason?.let { Text(text(it)) }
                }
            },
            confirmButton = { TextButton(onClick = { details = false }) { Text(stringResource(R.string.close)) } },
        )
    }
}
