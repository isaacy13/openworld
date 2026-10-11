// SPDX-License-Identifier: Apache-2.0
package app.openworld

import android.graphics.BitmapFactory
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.Image
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Checkbox
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.text
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp

/** A break between characters, so a long name with no spaces wraps on the page. */
internal fun wrappingFileName(name: String): String {
    if (name.isEmpty()) return name
    return name.map { it.toString() }.joinToString("\u200B")
}

internal fun posterLine(title: String, classLabel: String): String {
    return when {
        title.isBlank() -> classLabel
        classLabel.isBlank() -> title
        else -> "$title ($classLabel)"
    }
}

internal fun markedChoice(title: String, selected: Boolean): String {
    if (!selected) return title
    return if (title.endsWith(".")) "$title Selected." else "$title. Selected."
}

private object ProductCopy {
    const val possible = "Possible candidate. Not an identification."
    const val incomplete = "Incomplete."
    const val clearance = "No candidate is not a clearance."
    const val leaving = "You are leaving OpenWorld."
}

private val warningColor = Color(0xFF8A5A00)

private val disclosure = listOf(
    "Nothing is uploaded.",
    "Nobody is enrolled.",
    "OpenWorld does not train on this file.",
    "OpenWorld does not contact an agency.",
    "A candidate is not an identification.",
    ProductCopy.clearance,
    "This file is not authenticated.",
    "On-device does not mean the file is real.",
)

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun OpenWorldApp(model: FlowModel, onChoose: () -> Unit, onOpen: (String) -> Unit) {
    BackHandler(enabled = model.step != Step.Choose || model.scanning) {
        model.back()
    }
    MaterialTheme {
        Scaffold(topBar = { TopAppBar(title = { Text("OpenWorld") }) }) { padding ->
            val scroll = rememberScrollState()
            LaunchedEffect(model.step, model.leaveNotice, model.deleteNotice) { scroll.scrollTo(0) }
            Column(
                modifier = Modifier
                    .padding(padding)
                    .padding(20.dp)
                    .fillMaxSize(),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                model.pickNotice?.let { Text(it, color = warningColor) }
                if (model.step == Step.Results) {
                    model.leaveNotice?.let { Text(it, color = warningColor) }
                }
                model.deleteNotice?.let { Text(it, color = warningColor) }
                Column(
                    modifier = Modifier
                        .weight(1f)
                        .verticalScroll(scroll),
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                if (model.step != Step.Choose) {
                    TextButton(onClick = model::back, enabled = !model.scanning) { Text("Back") }
                }
                when (model.step) {
                    Step.Choose -> {
                        Text("Choose a photo or video", style = MaterialTheme.typography.headlineMedium)
                        Text("Import a file you already have, or receive it from the share sheet. There is no camera.")
                        Button(onClick = onChoose) { Text("Choose File") }
                    }
                    Step.Device -> {
                        Text("This file stays on this device.", style = MaterialTheme.typography.headlineMedium)
                        if (model.fileName.isNotBlank()) {
                            Box(
                                Modifier
                                    .fillMaxWidth()
                                    .clearAndSetSemantics { text = AnnotatedString(model.fileName) },
                            ) {
                                Text(
                                    wrappingFileName(model.fileName),
                                    modifier = Modifier.fillMaxWidth(),
                                    style = MaterialTheme.typography.titleMedium,
                                )
                            }
                        }
                        if (model.oldFile) Text("This file is older than about 30 days.", color = warningColor)
                        disclosure.forEach { Text(it) }
                        Text("Fixture posters. Real FBI photos stay off.")
                        Button(onClick = model::continueFromDevice) { Text("Continue") }
                    }
                    Step.Bundle -> {
                        val bundleRefused = model.bundleRows.isEmpty()
                        if (bundleRefused) {
                            Text(
                                model.bundleNotice ?: "The scan program is not on this device. Refusing.",
                                style = MaterialTheme.typography.headlineMedium,
                                color = warningColor,
                            )
                        } else {
                            Text("Model bundle", style = MaterialTheme.typography.headlineMedium)
                            Text("Scores are not comparable across bundles. Results name the bundle you pick.")
                        }
                        model.bundleRows.forEach { (id, title, curve) ->
                            val lines = title.lines()
                            val name = lines.firstOrNull().orEmpty()
                            val best = lines.drop(1).joinToString("\n")
                            Row(
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .selectable(
                                        selected = model.bundleId == id,
                                        onClick = { model.bundleId = id },
                                        role = Role.RadioButton,
                                    ),
                            ) {
                                RadioButton(selected = model.bundleId == id, onClick = null)
                                Column {
                                    Text(markedChoice(name, model.bundleId == id), style = MaterialTheme.typography.titleMedium)
                                    if (best.isNotBlank()) Text(best)
                                    Text(curve)
                                }
                            }
                        }
                        Button(onClick = model::continueFromBundle, enabled = model.bundleRows.isNotEmpty()) { Text("Continue") }
                    }
                    Step.Size -> {
                        Text("Detection size", style = MaterialTheme.typography.headlineMedium)
                        Text("Smaller frames are a resize of each decoded frame in memory. A face under 64 px on that image is left out. Evidence crops come from the original frame. If that crop is under 112 px on the short side, the label is \"Not compared.\" Full resolution is slower.")
                        listOf("320" to "320 px on the long side", "480" to "480 px on the long side", "640" to "640 px on the long side", "full" to "Full resolution").forEach { (value, label) ->
                            Row(
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .selectable(
                                        selected = model.longSide == value,
                                        onClick = { model.longSide = value },
                                        role = Role.RadioButton,
                                    ),
                            ) {
                                RadioButton(selected = model.longSide == value, onClick = null)
                                Text(markedChoice(label, model.longSide == value), modifier = Modifier.padding(top = 12.dp))
                            }
                        }
                        Text("Coverage", style = MaterialTheme.typography.titleMedium)
                        Row(
                            modifier = Modifier
                                .fillMaxWidth()
                                .selectable(
                                    selected = model.coverage == "complete",
                                    onClick = { model.coverage = "complete" },
                                    role = Role.RadioButton,
                                ),
                        ) {
                            RadioButton(selected = model.coverage == "complete", onClick = null)
                            Text(markedChoice("Complete. Every decoded frame.", model.coverage == "complete"), modifier = Modifier.padding(top = 12.dp))
                        }
                        Row(
                            modifier = Modifier
                                .fillMaxWidth()
                                .selectable(
                                    selected = model.coverage == "measured",
                                    onClick = { model.coverage = "measured" },
                                    role = Role.RadioButton,
                                ),
                        ) {
                            RadioButton(selected = model.coverage == "measured", onClick = null)
                            Text(markedChoice("Measured. 5 frames a second, plus the tracker.", model.coverage == "measured"), modifier = Modifier.padding(top = 12.dp))
                        }
                        if (model.coverage == "measured") Text("A brief face can be missed.")
                        Button(onClick = model::continueFromSize) { Text("Continue") }
                    }
                    Step.Estimate -> {
                        val refused = !model.scanning && !model.estimateOk && model.estimateText.isNotBlank()
                        if (refused) {
                            Text(model.estimateText, style = MaterialTheme.typography.headlineMedium, color = warningColor)
                        } else {
                            Text(if (model.scanning) "Scanning" else "Estimate", style = MaterialTheme.typography.headlineMedium)
                        }
                        if (model.scanning && model.liveCrops.isNotEmpty()) {
                            Text("Crops from this file.", style = MaterialTheme.typography.titleMedium)
                            model.liveCrops.forEach { crop ->
                                Column(
                                    modifier = Modifier.width(140.dp),
                                    verticalArrangement = Arrangement.spacedBy(4.dp),
                                    horizontalAlignment = Alignment.CenterHorizontally,
                                ) {
                                    val bitmap = BitmapFactory.decodeFile(crop.path)
                                    if (bitmap != null) {
                                        Image(
                                            bitmap = bitmap.asImageBitmap(),
                                            contentDescription = crop.label,
                                            modifier = Modifier.size(112.dp),
                                        )
                                    }
                                    Text(crop.frameLabel, textAlign = TextAlign.Center)
                                    Text(crop.label, textAlign = TextAlign.Center)
                                }
                            }
                        }
                        if (model.estimateOk) {
                            if (model.oldFile) Text("This file is older than about 30 days.", color = warningColor)
                            Text(model.estimateText)
                            Row(
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .toggleable(
                                        value = model.includeMissing,
                                        enabled = !model.scanning,
                                        role = Role.Checkbox,
                                        onValueChange = {
                                            model.includeMissing = it
                                            model.refreshClassLine()
                                        },
                                    ),
                            ) {
                                Checkbox(
                                    checked = model.includeMissing,
                                    onCheckedChange = null,
                                    enabled = !model.scanning,
                                )
                                Text("Missing", modifier = Modifier.padding(top = 12.dp))
                            }
                            Row(
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .toggleable(
                                        value = model.includeWanted,
                                        enabled = !model.scanning,
                                        role = Role.Checkbox,
                                        onValueChange = {
                                            model.includeWanted = it
                                            model.refreshClassLine()
                                        },
                                    ),
                            ) {
                                Checkbox(
                                    checked = model.includeWanted,
                                    onCheckedChange = null,
                                    enabled = !model.scanning,
                                )
                                Text("Wanted", modifier = Modifier.padding(top = 12.dp))
                            }
                            Button(onClick = model::startScan, enabled = model.canAnalyze && !model.scanning) {
                                Text(if (model.scanning) "Scanning" else "Analyze")
                            }
                        }
                    }
                    Step.Results -> {
                        val headline = model.summary.ifBlank {
                            when (model.status) {
                                "complete" -> if (model.candidateRows.isNotEmpty()) ProductCopy.possible else ProductCopy.clearance
                                "refused" -> "Refusing."
                                "deleted" -> "Deleted."
                                else -> ProductCopy.incomplete
                            }
                        }
                        val refusedResult = model.status == "refused" || headline.endsWith("Refusing.")
                        Text(
                            headline,
                            style = MaterialTheme.typography.headlineMedium,
                            color = if (refusedResult) warningColor else Color.Unspecified,
                        )
                        if (model.incompleteReason.isNotBlank()) Text(model.incompleteReason, color = warningColor)
                        if (model.context.isNotBlank()) Text(model.context)
                        if (model.warnings.isNotBlank()) Text(model.warnings, color = warningColor)
                        model.candidateRows.forEach { row ->
                            Text(row.wording, style = MaterialTheme.typography.titleMedium)
                            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                evidencePicture(row.cropPath, "Crop")
                                evidencePicture(row.framePath, row.frameLabel)
                            }
                            if (row.uncertainty.isNotBlank()) Text(row.uncertainty)
                            val poster = posterLine(row.title, row.posterClassLabel)
                            if (poster.isNotBlank()) Text(poster)
                            if (row.url.isNotBlank()) {
                                Button(onClick = { model.prepareLeave(row.url) }) { Text("Open FBI page") }
                            }
                        }
                        if (model.strip.isNotEmpty()) {
                            Text("Crops from this file.", style = MaterialTheme.typography.titleMedium)
                            Row(
                                modifier = Modifier.horizontalScroll(rememberScrollState()),
                                horizontalArrangement = Arrangement.spacedBy(12.dp),
                            ) {
                                model.strip.forEach { crop ->
                                    Column(
                                        modifier = Modifier.width(140.dp),
                                        verticalArrangement = Arrangement.spacedBy(4.dp),
                                        horizontalAlignment = Alignment.CenterHorizontally,
                                    ) {
                                        val bitmap = BitmapFactory.decodeFile(crop.path)
                                        if (bitmap != null) {
                                            Image(
                                                bitmap = bitmap.asImageBitmap(),
                                                contentDescription = crop.label,
                                                modifier = Modifier.size(112.dp),
                                            )
                                        }
                                        Text(
                                            crop.frameLabel,
                                            style = MaterialTheme.typography.bodySmall,
                                            textAlign = TextAlign.Center,
                                        )
                                        Text(
                                            crop.label,
                                            style = MaterialTheme.typography.bodySmall,
                                            textAlign = TextAlign.Center,
                                        )
                                    }
                                }
                            }
                        }
                        if (model.detail.isNotBlank()) Text(model.detail)
                        if (model.resultDir != null) {
                            Button(onClick = model::deleteResult) { Text("Delete") }
                        }
                        Button(onClick = model::chooseAnother) { Text("Choose another file") }
                    }
                }
                }
            }
        }
        val url = model.leavingUrl
        if (model.step == Step.Results && url != null) {
            AlertDialog(
                onDismissRequest = { model.leavingUrl = null },
                title = { Text(ProductCopy.leaving) },
                text = { Text(url) },
                confirmButton = {
                    TextButton(onClick = {
                        onOpen(url)
                        model.leavingUrl = null
                    }) { Text("Open") }
                },
                dismissButton = {
                    TextButton(onClick = { model.leavingUrl = null }) { Text("Stay") }
                },
            )
        }
    }
}

@Composable
private fun evidencePicture(path: String?, caption: String) {
    if (path.isNullOrBlank()) return
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        val bitmap = BitmapFactory.decodeFile(path)
        if (bitmap != null) {
            Image(
                bitmap = bitmap.asImageBitmap(),
                contentDescription = caption,
                modifier = Modifier.size(112.dp),
            )
        }
        Text(caption, style = MaterialTheme.typography.bodySmall)
    }
}

@Composable
private fun BundleChoice(id: String, name: String, bestFor: String, model: FlowModel) {
    androidx.compose.foundation.layout.Row {
        RadioButton(selected = model.bundleId == id, onClick = { model.bundleId = id })
        Column {
            Text(name, style = MaterialTheme.typography.titleMedium)
            Text(bestFor)
        }
    }
}
