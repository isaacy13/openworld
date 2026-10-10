// SPDX-License-Identifier: Apache-2.0
package app.openworld

import android.graphics.BitmapFactory
import androidx.compose.foundation.Image
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
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
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

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
    MaterialTheme {
        Scaffold(topBar = { TopAppBar(title = { Text("OpenWorld") }) }) { padding ->
            Column(
                modifier = Modifier
                    .padding(padding)
                    .padding(20.dp)
                    .fillMaxSize()
                    .verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                model.pickNotice?.let { Text(it) }
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
                        Text(model.fileName, style = MaterialTheme.typography.titleMedium)
                        if (model.oldFile) Text("This file is older than about 30 days.", color = warningColor)
                        disclosure.forEach { Text(it) }
                        Text("Fixture posters. Real FBI photos stay off.")
                        Button(onClick = model::continueFromDevice) { Text("Continue") }
                    }
                    Step.Bundle -> {
                        Text("Model bundle", style = MaterialTheme.typography.headlineMedium)
                        Text("Scores are not comparable across bundles. Results name the bundle you pick.")
                        if (model.bundleRows.isEmpty()) {
                            Text("The scan program is not on this device. Refusing.")
                        }
                        model.bundleRows.forEach { (id, title, curve) ->
                            val lines = title.lines()
                            val name = lines.firstOrNull().orEmpty()
                            val best = lines.drop(1).joinToString("\n")
                            androidx.compose.foundation.layout.Row {
                                RadioButton(selected = model.bundleId == id, onClick = { model.bundleId = id })
                                Column {
                                    Text(markedChoice(name, model.bundleId == id), style = MaterialTheme.typography.titleMedium)
                                    if (best.isNotBlank()) Text(best)
                                    Text(curve)
                                }
                            }
                        }
                        Button(onClick = model::continueFromBundle) { Text("Continue") }
                    }
                    Step.Size -> {
                        Text("Detection size", style = MaterialTheme.typography.headlineMedium)
                        Text("Smaller frames are a resize of each decoded frame in memory. A face under 64 px on that image is left out. Evidence crops come from the original frame. If that crop is under 112 px on the short side, the label is \"Not compared.\" Full resolution is slower.")
                        listOf("320" to "320 px on the long side", "480" to "480 px on the long side", "640" to "640 px on the long side", "full" to "Full resolution").forEach { (value, label) ->
                            androidx.compose.foundation.layout.Row {
                                RadioButton(selected = model.longSide == value, onClick = { model.longSide = value })
                                Text(markedChoice(label, model.longSide == value), modifier = Modifier.padding(top = 12.dp))
                            }
                        }
                        Text("Coverage", style = MaterialTheme.typography.titleMedium)
                        androidx.compose.foundation.layout.Row {
                            RadioButton(selected = model.coverage == "complete", onClick = { model.coverage = "complete" })
                            Text(markedChoice("Complete. Every decoded frame.", model.coverage == "complete"), modifier = Modifier.padding(top = 12.dp))
                        }
                        androidx.compose.foundation.layout.Row {
                            RadioButton(selected = model.coverage == "measured", onClick = { model.coverage = "measured" })
                            Text(markedChoice("Measured. 5 frames a second, plus the tracker.", model.coverage == "measured"), modifier = Modifier.padding(top = 12.dp))
                        }
                        if (model.coverage == "measured") Text("A brief face can be missed.")
                        Button(onClick = model::continueFromSize) { Text("Continue") }
                    }
                    Step.Estimate -> {
                        Text(if (model.scanning) "Scanning" else "Estimate", style = MaterialTheme.typography.headlineMedium)
                        if (model.scanning && model.liveCrops.isNotEmpty()) {
                            Text("Crops from this file.", style = MaterialTheme.typography.titleMedium)
                            model.liveCrops.forEach { crop ->
                                Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                                    val bitmap = BitmapFactory.decodeFile(crop.path)
                                    if (bitmap != null) {
                                        Image(
                                            bitmap = bitmap.asImageBitmap(),
                                            contentDescription = crop.label,
                                            modifier = Modifier.size(112.dp),
                                        )
                                    }
                                    Text(crop.frameLabel)
                                    Text(crop.label)
                                }
                            }
                        }
                        if (model.oldFile) Text("This file is older than about 30 days.", color = warningColor)
                        Text(model.estimateText)
                        if (model.estimateOk) {
                            Row {
                                Checkbox(
                                    checked = model.includeMissing,
                                    onCheckedChange = {
                                        model.includeMissing = it
                                        model.refreshClassLine()
                                    },
                                    enabled = !model.scanning,
                                )
                                Text("Missing", modifier = Modifier.padding(top = 12.dp))
                            }
                            Row {
                                Checkbox(
                                    checked = model.includeWanted,
                                    onCheckedChange = {
                                        model.includeWanted = it
                                        model.refreshClassLine()
                                    },
                                    enabled = !model.scanning,
                                )
                                Text("Wanted", modifier = Modifier.padding(top = 12.dp))
                            }
                            Button(onClick = model::startScan, enabled = model.canAnalyze && !model.scanning) {
                                Text(if (model.scanning) "Scanning" else "Analyze")
                            }
                        }
                        model.deleteNotice?.let { Text(it) }
                    }
                    Step.Results -> {
                        Text(
                            model.summary.ifBlank {
                                when (model.status) {
                                    "complete" -> if (model.fbiUrl != null) ProductCopy.possible else ProductCopy.clearance
                                    else -> ProductCopy.incomplete
                                }
                            },
                            style = MaterialTheme.typography.headlineMedium,
                        )
                        if (model.incompleteReason.isNotBlank()) Text(model.incompleteReason)
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
                            Button(onClick = { model.prepareLeave(row.url) }) { Text("Open FBI page") }
                        }
                        if (model.strip.isNotEmpty()) {
                            Text("Crops from this file.", style = MaterialTheme.typography.titleMedium)
                            Row(
                                modifier = Modifier.horizontalScroll(rememberScrollState()),
                                horizontalArrangement = Arrangement.spacedBy(12.dp),
                            ) {
                                model.strip.forEach { crop ->
                                    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                                        val bitmap = BitmapFactory.decodeFile(crop.path)
                                        if (bitmap != null) {
                                            Image(
                                                bitmap = bitmap.asImageBitmap(),
                                                contentDescription = crop.label,
                                                modifier = Modifier.size(112.dp),
                                            )
                                        }
                                        Text(crop.frameLabel, style = MaterialTheme.typography.bodySmall)
                                        Text(crop.label, style = MaterialTheme.typography.bodySmall)
                                    }
                                }
                            }
                        }
                        Text(model.detail)
                        model.leaveNotice?.let { Text(it) }
                        if (model.resultDir != null) {
                            Button(onClick = model::deleteResult) { Text("Delete") }
                        }
                        model.deleteNotice?.let { Text(it) }
                        Button(onClick = model::chooseAnother) { Text("Choose another file") }
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
