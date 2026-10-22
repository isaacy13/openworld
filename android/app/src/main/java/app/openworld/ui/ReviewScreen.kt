// SPDX-License-Identifier: Apache-2.0
@file:OptIn(ExperimentalMaterial3Api::class, ExperimentalLayoutApi::class)

package app.openworld.ui

import android.text.format.DateUtils
import android.text.format.Formatter
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.calculateEndPadding
import androidx.compose.foundation.layout.calculateStartPadding
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.ArrowBack
import androidx.compose.material.icons.outlined.BatteryAlert
import androidx.compose.material.icons.outlined.Check
import androidx.compose.material.icons.outlined.Computer
import androidx.compose.material.icons.outlined.DeveloperBoard
import androidx.compose.material.icons.outlined.ErrorOutline
import androidx.compose.material.icons.outlined.ExpandMore
import androidx.compose.material.icons.outlined.Info
import androidx.compose.material.icons.outlined.Lock
import androidx.compose.material.icons.outlined.Memory
import androidx.compose.material.icons.outlined.Movie
import androidx.compose.material.icons.outlined.Photo
import androidx.compose.material.icons.outlined.Thermostat
import androidx.compose.material.icons.outlined.WarningAmber
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.FilterChipDefaults
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import app.openworld.R
import app.openworld.core.BundleRow
import app.openworld.core.Line
import app.openworld.core.PosterPack
import app.openworld.media.MediaInfo
import app.openworld.ui.theme.OpenWorldColors
import java.text.DateFormat
import java.text.NumberFormat
import java.util.Date

@Composable
fun ReviewScreen(
    review: ScanState.Review,
    posters: PosterPack?,
    snackbar: SnackbarHostState,
    onBack: () -> Unit,
    onMissing: (Boolean) -> Unit,
    onWanted: (Boolean) -> Unit,
    onBundle: (String) -> Unit,
    onCoverage: (String) -> Unit,
    onAnalyze: () -> Unit,
    onChooseAnother: () -> Unit,
) {
    var readAll by rememberSaveable { mutableStateOf(false) }
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(stringResource(R.string.review_title)) },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Outlined.ArrowBack, contentDescription = stringResource(R.string.back))
                    }
                },
            )
        },
        bottomBar = {
            Surface(tonalElevation = 3.dp) {
                Column(
                    modifier = Modifier.fillMaxWidth().navigationBarsPadding().padding(horizontal = 16.dp, vertical = 12.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    Button(
                        onClick = onAnalyze,
                        enabled = review.canAnalyze,
                        modifier = Modifier.fillMaxWidth().widthIn(max = 640.dp).heightIn(min = 56.dp),
                    ) {
                        Icon(Icons.Outlined.Lock, contentDescription = null, modifier = Modifier.size(20.dp))
                        Spacer(Modifier.size(8.dp))
                        Text(stringResource(R.string.analyze))
                    }
                    TextButton(onClick = onChooseAnother, modifier = Modifier.heightIn(min = 48.dp)) {
                        Text(stringResource(R.string.choose_another))
                    }
                }
            }
        },
        snackbarHost = { SnackbarHost(snackbar) },
    ) { padding ->
        LazyColumn(
            contentPadding = PaddingValues(
                start = padding.calculateStartPadding(LocalLayoutDirection.current) + 16.dp,
                end = padding.calculateEndPadding(LocalLayoutDirection.current) + 16.dp,
                top = padding.calculateTopPadding() + 8.dp,
                bottom = padding.calculateBottomPadding() + 16.dp,
            ),
            verticalArrangement = Arrangement.spacedBy(12.dp),
            modifier = Modifier.fillMaxWidth(),
        ) {
            item { FileCard(review) }
            when (val media = review.media) {
                is MediaState.Refused -> item {
                    ToneCard(OpenWorldColors.status.error, Icons.Outlined.ErrorOutline) {
                        Text(text(media.line), style = MaterialTheme.typography.bodyLarge)
                        Text(stringResource(R.string.hint_file), style = MaterialTheme.typography.bodyMedium)
                    }
                }
                is MediaState.Ready -> {
                    item { EstimateCard(review.estimate) }
                    item { CompareCard(review, posters, onMissing, onWanted) }
                    item { OptionsCard(review, media.info, onBundle, onCoverage) }
                }
                MediaState.Probing -> Unit
            }
            if (review.disclosure.isNotEmpty()) item { DisclosureCard(review.disclosure, onReadAll = { readAll = true }) }
        }
    }
    if (readAll) DisclosureSheet(review.disclosure, onDismiss = { readAll = false })
}

@Composable
private fun ReviewCard(content: @Composable () -> Unit) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow),
    ) {
        Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) { content() }
    }
}

@Composable
private fun FileCard(review: ScanState.Review) {
    val context = LocalContext.current
    val file = review.file
    val info = (review.media as? MediaState.Ready)?.info
    ReviewCard {
        Row(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
            SquareImage(rememberThumbnail(file), stringResource(R.string.file_thumbnail), Modifier.size(88.dp))
            Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                // The whole name, wrapped at word breaks.
                Text(file.name, style = MaterialTheme.typography.titleMedium)
                val video = info?.video ?: (file.mime?.startsWith("video/") == true)
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    Icon(
                        if (video) Icons.Outlined.Movie else Icons.Outlined.Photo,
                        contentDescription = null,
                        modifier = Modifier.size(16.dp),
                        tint = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    Text(
                        text = fileFacts(context, info, video, file.size),
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                info?.recordedUnix?.let { unix ->
                    Text(
                        text = stringResource(R.string.file_recorded, DateFormat.getDateTimeInstance(DateFormat.MEDIUM, DateFormat.SHORT).format(Date(unix * 1000))),
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                if (review.media is MediaState.Probing) {
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        CircularProgressIndicator(modifier = Modifier.size(16.dp), strokeWidth = 2.dp)
                        Text(stringResource(R.string.file_reading), style = MaterialTheme.typography.bodySmall)
                    }
                }
            }
        }
        val caution = OpenWorldColors.status.candidate.content
        review.warnings.forEach { IconLine(Icons.Outlined.WarningAmber, text(it), tint = caution) }
        // Only the still is scanned, so the core ends the scan Incomplete: the incomplete tone, not a candidate's.
        if (info?.motionPhoto == true) {
            IconLine(Icons.Outlined.WarningAmber, stringResource(R.string.motion_photo_note), tint = OpenWorldColors.status.incomplete.content)
        }
    }
}

private fun fileFacts(context: android.content.Context, info: MediaInfo?, video: Boolean, size: Long): String {
    val kind = context.getString(if (video) R.string.file_video else R.string.file_photo)
    val parts = mutableListOf(kind)
    if (info != null) {
        if (info.video && info.durationSec > 0) {
            parts += DateUtils.formatElapsedTime(info.durationSec.toLong().coerceAtLeast(1))
        } else if (!info.video) {
            parts += "${info.width} × ${info.height}"
        }
    }
    if (size > 0) parts += Formatter.formatShortFileSize(context, size)
    return parts.joinToString(" · ")
}

@Composable
private fun EstimateCard(estimate: EstimateState) {
    ReviewCard {
        Text(
            stringResource(R.string.estimate_title),
            style = MaterialTheme.typography.labelLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.semantics { heading() },
        )
        when (estimate) {
            EstimateState.Loading -> Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                CircularProgressIndicator(modifier = Modifier.size(24.dp), strokeWidth = 2.dp)
                Text(stringResource(R.string.estimating), style = MaterialTheme.typography.bodyLarge)
            }
            is EstimateState.Refused -> IconLine(Icons.Outlined.ErrorOutline, text(estimate.line), tint = OpenWorldColors.status.error.content)
            is EstimateState.Ready -> {
                val value = estimate.value
                Text(text(value.human), style = MaterialTheme.typography.headlineMedium)
                val frames = NumberFormat.getIntegerInstance().format(value.framesAnalyzed)
                val framesText = pluralStringResource(R.plurals.estimate_frames, value.framesAnalyzed.coerceIn(0, Int.MAX_VALUE.toLong()).toInt(), frames)
                Text(
                    text = listOfNotNull(framesText, value.detectionLine?.let { text(it) }).joinToString(" · "),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                if (value.notes.isNotEmpty() || value.coverageBanner != null) HorizontalDivider(Modifier.padding(vertical = 4.dp))
                value.coverageBanner?.let { IconLine(Icons.Outlined.WarningAmber, text(it), tint = OpenWorldColors.status.candidate.content) }
                value.notes.forEach { note -> IconLine(noteIcon(note), text(note)) }
                value.caveat?.let {
                    Text(text(it), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
        }
    }
}

private fun noteIcon(note: Line): ImageVector = when (note.code) {
    "cpu_note" -> Icons.Outlined.Memory
    "gpu_note" -> Icons.Outlined.DeveloperBoard
    "heat_note", "already_warm" -> Icons.Outlined.Thermostat
    "battery_note" -> Icons.Outlined.BatteryAlert
    "suggest_computer" -> Icons.Outlined.Computer
    else -> Icons.Outlined.Info
}

@Composable
private fun CompareCard(review: ScanState.Review, posters: PosterPack?, onMissing: (Boolean) -> Unit, onWanted: (Boolean) -> Unit) {
    ReviewCard {
        SectionTitle(stringResource(R.string.compare_title))
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            ClassChip(stringResource(R.string.class_missing), posters?.missing, review.missing, onMissing)
            ClassChip(stringResource(R.string.class_wanted), posters?.wanted, review.wanted, onWanted)
        }
        if (review.bothOff) {
            val classes = (review.estimate as? EstimateState.Ready)?.value?.classes
            if (classes != null) IconLine(Icons.Outlined.Info, text(classes), tint = OpenWorldColors.status.error.content)
        }
    }
}

@Composable
private fun ClassChip(label: String, count: Int?, selected: Boolean, onChange: (Boolean) -> Unit) {
    FilterChip(
        selected = selected,
        onClick = { onChange(!selected) },
        label = { Text(if (count != null) stringResource(R.string.class_count, label, count) else label) },
        leadingIcon = if (selected) {
            { Icon(Icons.Outlined.Check, contentDescription = null, modifier = Modifier.size(FilterChipDefaults.IconSize)) }
        } else {
            null
        },
        modifier = Modifier.heightIn(min = 48.dp),
    )
}

@Composable
private fun OptionsCard(review: ScanState.Review, info: MediaInfo, onBundle: (String) -> Unit, onCoverage: (String) -> Unit) {
    var open by rememberSaveable { mutableStateOf(false) }
    val turn by animateFloatAsState(if (open) 180f else 0f, label = "chevron")
    ReviewCard {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .heightIn(min = 48.dp)
                .selectable(selected = open, role = Role.Button, onClick = { open = !open }),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            SectionTitle(stringResource(R.string.options_title), Modifier.weight(1f))
            Icon(Icons.Outlined.ExpandMore, contentDescription = null, modifier = Modifier.rotate(turn))
        }
        AnimatedVisibility(visible = open) {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Text(stringResource(R.string.options_bundle), style = MaterialTheme.typography.labelLarge)
                Column(Modifier.selectableGroup()) {
                    review.bundles.forEach { row -> BundleOption(row, row.id == review.bundleId) { onBundle(row.id) } }
                }
                review.bundleNote?.let {
                    Text(text(it), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
                if (info.video) {
                    Text(stringResource(R.string.options_coverage), style = MaterialTheme.typography.labelLarge)
                    val choices = listOf(ScanState.COMPLETE to R.string.coverage_every_frame, ScanState.MEASURED to R.string.coverage_measured)
                    SingleChoiceSegmentedButtonRow(modifier = Modifier.fillMaxWidth()) {
                        choices.forEachIndexed { index, (value, label) ->
                            SegmentedButton(
                                selected = review.coverage == value,
                                onClick = { onCoverage(value) },
                                shape = SegmentedButtonDefaults.itemShape(index = index, count = choices.size),
                            ) {
                                Text(stringResource(label))
                            }
                        }
                    }
                    if (review.coverage == ScanState.MEASURED) {
                        IconLine(Icons.Outlined.WarningAmber, stringResource(R.string.coverage_brief), tint = OpenWorldColors.status.candidate.content)
                    }
                }
            }
        }
    }
}

@Composable
private fun BundleOption(row: BundleRow, selected: Boolean, onSelect: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = 56.dp)
            .selectable(selected = selected, role = Role.RadioButton, onClick = onSelect)
            .padding(vertical = 8.dp),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        RadioButton(selected = selected, onClick = null)
        Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(row.name, style = MaterialTheme.typography.titleSmall)
            Text(row.bestFor, style = MaterialTheme.typography.bodyMedium)
            row.curveLine?.let {
                Text(text(it), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            if (!row.weightsReady) {
                Text(stringResource(R.string.bundle_no_weights), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            if (!row.official) {
                Text(stringResource(R.string.bundle_unofficial), style = MaterialTheme.typography.bodySmall, color = OpenWorldColors.status.candidate.content)
            }
        }
    }
}

@Composable
fun DisclosureCard(lines: List<Line>, onReadAll: (() -> Unit)?, all: Boolean = false) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow),
    ) {
        Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Icon(Icons.Outlined.Lock, contentDescription = null, tint = MaterialTheme.colorScheme.primary)
                SectionTitle(stringResource(R.string.disclosure_title))
            }
            (if (all) lines else lines.take(3)).forEach { IconLine(Icons.Outlined.Check, text(it)) }
            if (onReadAll != null && lines.size > 3) {
                TextButton(onClick = onReadAll, contentPadding = PaddingValues(horizontal = 0.dp)) {
                    Text(stringResource(R.string.disclosure_read_all))
                }
            }
        }
    }
}

@Composable
fun DisclosureSheet(lines: List<Line>, onDismiss: () -> Unit) {
    ModalBottomSheet(onDismissRequest = onDismiss, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 24.dp)
                .padding(bottom = 24.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text(
                stringResource(R.string.disclosure_title),
                style = MaterialTheme.typography.headlineSmall,
                modifier = Modifier.semantics { heading() },
            )
            lines.forEach { IconLine(Icons.Outlined.Check, text(it)) }
            TextButton(onClick = onDismiss, modifier = Modifier.align(Alignment.End)) { Text(stringResource(R.string.close)) }
        }
    }
}
