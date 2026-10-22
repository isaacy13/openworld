// SPDX-License-Identifier: Apache-2.0
package app.openworld.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.ArrowBack
import androidx.compose.material.icons.automirrored.outlined.ArrowForward
import androidx.compose.material.icons.outlined.Description
import androidx.compose.material.icons.outlined.ErrorOutline
import androidx.compose.material.icons.outlined.Info
import androidx.compose.material.icons.outlined.Lock
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.openworld.R
import app.openworld.core.Legal
import app.openworld.core.LegalDocument
import app.openworld.core.Line
import app.openworld.ui.theme.OpenWorldColors

/** The documents First run links to. About lists every document. */
private val FIRST_RUN_DOCUMENTS = listOf("terms", "privacy", "biometric")

/** First-run lines about where the file stays get the lock; the rest get the info icon. */
private val ON_DEVICE_LINES = setOf("welcome", "nothing_uploaded")

/**
 * Shown in front of every screen until Continue (docs/ux.md §0). No checkbox. Without a version
 * from the core there is nothing to accept, so the refusal shows and there is no Continue.
 */
@Composable
fun FirstRunScreen(legal: Legal?, changed: Boolean, refusal: Line?, onRead: (String) -> Unit, onContinue: () -> Unit) {
    Scaffold { padding ->
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
                    text = stringResource(R.string.first_run_title),
                    style = MaterialTheme.typography.displaySmall,
                    modifier = Modifier.semantics { heading() },
                )
                if (legal == null) {
                    ToneCard(OpenWorldColors.status.error, Icons.Outlined.ErrorOutline) {
                        Text(text(refusal), style = MaterialTheme.typography.bodyLarge)
                    }
                } else {
                    FirstRunBody(legal, changed, onRead, onContinue)
                }
            }
        }
    }
}

@Composable
private fun FirstRunBody(legal: Legal, changed: Boolean, onRead: (String) -> Unit, onContinue: () -> Unit) {
    if (changed) {
        legal.changed?.let { line ->
            ToneCard(OpenWorldColors.status.neutral, Icons.Outlined.Info) {
                Text(text(line), style = MaterialTheme.typography.bodyLarge)
            }
        }
    }
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        legal.firstRun.forEach { line ->
            IconLine(if (line.code in ON_DEVICE_LINES) Icons.Outlined.Lock else Icons.Outlined.Info, text(line))
        }
    }
    legal.accept?.let { line -> Text(text(line), style = MaterialTheme.typography.bodyMedium) }
    Column {
        legal.documents.filter { it.id in FIRST_RUN_DOCUMENTS }.forEach { document ->
            DocumentRow(document, onRead)
        }
    }
    Spacer(Modifier.height(8.dp))
    Button(onClick = onContinue, modifier = Modifier.fillMaxWidth().heightIn(min = 56.dp)) {
        Text(stringResource(R.string.first_run_continue))
    }
}

/** The version, the contact, and every built-in document (docs/ux.md, About). */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AboutScreen(legal: Legal?, onBack: () -> Unit, onRead: (String) -> Unit) {
    val context = LocalContext.current
    val version = remember(context) {
        runCatching { context.packageManager.getPackageInfo(context.packageName, 0).versionName }.getOrNull().orEmpty()
    }
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(stringResource(R.string.about_title)) },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Outlined.ArrowBack, contentDescription = stringResource(R.string.back))
                    }
                },
            )
        },
    ) { padding ->
        Box(modifier = Modifier.fillMaxSize().padding(padding), contentAlignment = Alignment.TopCenter) {
            Column(
                modifier = Modifier
                    .widthIn(max = 640.dp)
                    .fillMaxWidth()
                    .verticalScroll(rememberScrollState())
                    .padding(vertical = 8.dp),
            ) {
                ListItem(
                    headlineContent = { Text(stringResource(R.string.about_version)) },
                    supportingContent = { Text(version) },
                )
                if (legal != null) {
                    ListItem(
                        headlineContent = { Text(stringResource(R.string.about_contact)) },
                        supportingContent = { SelectionContainer { Text(legal.contact) } },
                    )
                    HorizontalDivider(modifier = Modifier.padding(horizontal = 16.dp))
                    legal.documents.forEach { document -> DocumentRow(document, onRead, gutter = 16.dp) }
                }
            }
        }
    }
}

/** A link to a built-in document. The ripple spans the row; `gutter` is its side padding. */
@Composable
private fun DocumentRow(document: LegalDocument, onRead: (String) -> Unit, gutter: Dp = 0.dp) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(role = Role.Button) { onRead(document.id) }
            .heightIn(min = 48.dp)
            .padding(horizontal = gutter, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Icon(Icons.Outlined.Description, contentDescription = null, tint = MaterialTheme.colorScheme.primary)
        Text(
            text(document.title),
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.primary,
            modifier = Modifier.weight(1f),
        )
        Icon(Icons.AutoMirrored.Outlined.ArrowForward, contentDescription = null, tint = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

/** A document's paragraphs, lazily: the licenses run to thousands of them. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ReadingSheet(reading: Reading, onDismiss: () -> Unit) {
    ModalBottomSheet(onDismissRequest = onDismiss, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)) {
        val paragraphs = reading.paragraphs
        LazyColumn(
            modifier = Modifier.fillMaxSize(),
            contentPadding = PaddingValues(start = 24.dp, end = 24.dp, bottom = 24.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            item {
                Text(
                    text(reading.title),
                    style = MaterialTheme.typography.headlineSmall,
                    modifier = Modifier.semantics { heading() },
                )
            }
            when {
                reading.refusal != null -> item {
                    ToneCard(OpenWorldColors.status.error, Icons.Outlined.ErrorOutline) {
                        Text(text(reading.refusal), style = MaterialTheme.typography.bodyLarge)
                    }
                }
                paragraphs == null -> item {
                    Box(modifier = Modifier.fillMaxWidth().padding(24.dp), contentAlignment = Alignment.Center) {
                        CircularProgressIndicator()
                    }
                }
                else -> items(paragraphs) { paragraph -> Text(paragraph, style = MaterialTheme.typography.bodyMedium) }
            }
        }
    }
}
