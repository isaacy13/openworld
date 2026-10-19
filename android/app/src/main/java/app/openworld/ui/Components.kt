// SPDX-License-Identifier: Apache-2.0
package app.openworld.ui

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Matrix
import android.media.MediaMetadataRetriever
import android.net.Uri
import android.os.Build
import android.util.LruCache
import android.util.Size
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Image
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.exifinterface.media.ExifInterface
import app.openworld.core.Line
import app.openworld.platform.Imported
import app.openworld.ui.theme.Tone
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.File
import java.util.concurrent.ConcurrentHashMap

// Words.

/** The words for a Line: a translation named core_<code> when one exists, else the core's text. */
@Composable
fun text(line: Line?): String {
    if (line == null) return ""
    // Read so a language change recomposes.
    LocalConfiguration.current
    return Words.of(LocalContext.current, line)
}

object Words {
    private val ids = ConcurrentHashMap<String, Int>()

    fun of(context: Context, line: Line): String {
        val translated = id(context, "core_${line.code}")
        val template = when {
            translated != 0 -> context.getString(translated)
            line.text.isNotEmpty() -> return line.text
            // A shell code: its words are a string by the same name.
            else -> id(context, line.code).takeIf { it != 0 }?.let { context.getString(it) } ?: return line.code
        }
        return line.args.entries.fold(template) { acc, (name, value) -> acc.replace("{$name}", value.toString()) }
    }

    @SuppressLint("DiscouragedApi")
    private fun id(context: Context, name: String): Int =
        ids.getOrPut(name) { context.resources.getIdentifier(name, "string", context.packageName) }
}

// Images, decoded off the main thread and kept.

private val bitmaps = object : LruCache<String, ImageBitmap>(48 * 1024 * 1024) {
    override fun sizeOf(key: String, value: ImageBitmap) = value.width * value.height * 4
}

/** A picture file the core wrote, sampled to about `maxPx`. `revision` reloads a rewritten file. */
@Composable
fun rememberFileImage(path: String?, revision: Int = 0, maxPx: Int = 512): ImageBitmap? {
    val image by produceState(initialValue = path?.let { bitmaps.get(key(it, revision, maxPx)) }, path, revision, maxPx) {
        if (path == null) {
            value = null
            return@produceState
        }
        // The last picture stays up until the rewritten one is decoded.
        value = bitmaps.get(key(path, revision, maxPx)) ?: withContext(Dispatchers.IO) { loadImage(path, revision, maxPx) } ?: value
    }
    return image
}

private fun key(path: String, revision: Int, maxPx: Int) = "$path#$revision#$maxPx"

private fun loadImage(path: String, revision: Int, maxPx: Int): ImageBitmap? {
    val key = key(path, revision, maxPx)
    bitmaps.get(key)?.let { return it }
    val image = sampled(File(path), maxPx)?.asImageBitmap() ?: return null
    bitmaps.put(key, image)
    return image
}

private fun sampled(file: File, maxPx: Int): Bitmap? = runCatching {
    val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    BitmapFactory.decodeFile(file.path, bounds)
    if (bounds.outWidth <= 0 || bounds.outHeight <= 0) return null
    var sample = 1
    while (maxOf(bounds.outWidth, bounds.outHeight) / (sample * 2) >= maxPx) sample *= 2
    BitmapFactory.decodeFile(file.path, BitmapFactory.Options().apply { inSampleSize = sample })
}.getOrNull()

/** The system thumbnail of the picked file, else one made from the copy. */
@Composable
fun rememberThumbnail(file: Imported): ImageBitmap? {
    val context = LocalContext.current.applicationContext
    val image by produceState<ImageBitmap?>(initialValue = null, file.source, file.file) {
        value = withContext(Dispatchers.IO) { thumbnail(context, file)?.asImageBitmap() }
    }
    return image
}

private fun thumbnail(context: Context, file: Imported): Bitmap? = runCatching {
    if (Build.VERSION.SDK_INT >= 29) {
        runCatching { context.contentResolver.loadThumbnail(Uri.parse(file.source), Size(THUMB_PX, THUMB_PX), null) }
            .getOrNull()?.let { return it }
    }
    if (file.mime?.startsWith("video/") == true) {
        val retriever = MediaMetadataRetriever()
        try {
            retriever.setDataSource(file.file.path)
            retriever.frameAtTime?.let { frame -> scaled(frame) }
        } finally {
            runCatching { retriever.release() }
        }
    } else {
        val bitmap = sampled(file.file, THUMB_PX) ?: return null
        val degrees = runCatching { ExifInterface(file.file).rotationDegrees }.getOrDefault(0)
        if (degrees == 0) bitmap else Bitmap.createBitmap(bitmap, 0, 0, bitmap.width, bitmap.height, Matrix().apply { postRotate(degrees.toFloat()) }, true)
    }
}.getOrNull()

private fun scaled(frame: Bitmap): Bitmap {
    val long = maxOf(frame.width, frame.height)
    if (long <= THUMB_PX) return frame
    val k = THUMB_PX.toFloat() / long
    return Bitmap.createScaledBitmap(frame, (frame.width * k).toInt().coerceAtLeast(1), (frame.height * k).toInt().coerceAtLeast(1), true)
}

private const val THUMB_PX = 384

/** A square picture with a placeholder while it loads. */
@Composable
fun SquareImage(image: ImageBitmap?, description: String?, modifier: Modifier = Modifier, crisp: Boolean = false) {
    Box(
        modifier = modifier
            .clip(MaterialTheme.shapes.medium)
            .background(MaterialTheme.colorScheme.surfaceVariant),
        contentAlignment = Alignment.Center,
    ) {
        if (image != null) {
            Image(
                bitmap = image,
                contentDescription = description,
                contentScale = ContentScale.Crop,
                filterQuality = if (crisp) FilterQuality.None else FilterQuality.Low,
                modifier = Modifier.fillMaxSize(),
            )
        } else {
            Icon(Icons.Outlined.Image, contentDescription = description, tint = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
}

// Small pieces.

@Composable
fun SectionTitle(title: String, modifier: Modifier = Modifier) {
    Text(
        text = title,
        style = MaterialTheme.typography.titleMedium,
        modifier = modifier.semantics { heading() },
    )
}

/** An icon and a sentence, for notes, cautions, and disclosure lines. */
@Composable
fun IconLine(icon: ImageVector, text: String, modifier: Modifier = Modifier, tint: androidx.compose.ui.graphics.Color? = null) {
    Row(modifier = modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Icon(
            imageVector = icon,
            contentDescription = null,
            tint = tint ?: MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.size(20.dp),
        )
        Text(text = text, style = MaterialTheme.typography.bodyMedium, color = tint ?: MaterialTheme.colorScheme.onSurface)
    }
}

/** A small pill in a status tone. Not interactive. */
@Composable
fun Badge(text: String, tone: Tone, icon: ImageVector? = null) {
    Surface(color = tone.container, contentColor = tone.content, shape = CircleShape) {
        Row(
            modifier = Modifier.padding(horizontal = 10.dp, vertical = 4.dp),
            horizontalArrangement = Arrangement.spacedBy(6.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (icon != null) Icon(icon, contentDescription = null, modifier = Modifier.size(16.dp))
            Text(text = text, style = MaterialTheme.typography.labelLarge, fontWeight = FontWeight.Medium)
        }
    }
}

/** A card tinted by a status tone, with its icon, so status is never color alone. */
@Composable
fun ToneCard(tone: Tone, icon: ImageVector, modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    Surface(color = tone.container, contentColor = tone.content, shape = MaterialTheme.shapes.medium, modifier = modifier.fillMaxWidth()) {
        Row(modifier = Modifier.padding(16.dp), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Icon(icon, contentDescription = null, modifier = Modifier.size(24.dp))
            Column(verticalArrangement = Arrangement.spacedBy(4.dp)) { content() }
        }
    }
}
