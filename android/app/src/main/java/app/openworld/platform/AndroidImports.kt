// SPDX-License-Identifier: Apache-2.0
package app.openworld.platform

import android.content.ContentResolver
import android.net.Uri
import android.provider.DocumentsContract
import android.provider.MediaStore
import android.provider.OpenableColumns
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext
import java.io.File
import java.io.IOException
import java.util.concurrent.ConcurrentHashMap

/** Copies a content URI into cacheDir/imports. Only content:// photos and videos are read. */
class AndroidImports(private val resolver: ContentResolver, private val dir: File) : Imports {

    override suspend fun copy(uri: Uri, onProgress: (Float?) -> Unit): Imported = withContext(Dispatchers.IO) {
        if (uri.scheme != ContentResolver.SCHEME_CONTENT) throw ImportRefused(Shell.IMPORT_TYPE)
        val mime = runCatching { resolver.getType(uri) }.getOrNull()
        if (mime != null && !mime.startsWith("image/") && !mime.startsWith("video/")) throw ImportRefused(Shell.IMPORT_TYPE)
        val facts = query(uri)
        if (!dir.isDirectory && !dir.mkdirs()) throw ImportRefused(Shell.IMPORT_FAILED)
        val size = facts.size
        if (size != null && size > dir.usableSpace - SPACE_MARGIN) throw ImportRefused(Shell.IMPORT_SPACE)
        val target = File(dir, "import-${System.currentTimeMillis()}${extension(facts.name, mime)}")
        try {
            val input = resolver.openInputStream(uri) ?: throw ImportRefused(Shell.IMPORT_FAILED)
            input.use { source ->
                target.outputStream().use { sink ->
                    val buffer = ByteArray(1 shl 20)
                    var copied = 0L
                    var reported = -1
                    while (true) {
                        coroutineContext.ensureActive()
                        val n = source.read(buffer)
                        if (n < 0) break
                        sink.write(buffer, 0, n)
                        copied += n
                        val percent = if (size != null && size > 0) (copied * 100 / size).toInt() else -1
                        if (percent != reported) {
                            reported = percent
                            onProgress(if (percent >= 0) percent.coerceAtMost(100) / 100f else null)
                        }
                    }
                }
            }
        } catch (e: ImportRefused) {
            target.delete()
            throw e
        } catch (e: IOException) {
            target.delete()
            throw ImportRefused(if (dir.usableSpace < SPACE_MARGIN) Shell.IMPORT_SPACE else Shell.IMPORT_FAILED)
        } catch (e: SecurityException) {
            target.delete()
            throw ImportRefused(Shell.IMPORT_FAILED)
        } catch (t: Throwable) {
            target.delete()
            throw t
        }
        facts.modifiedMs?.let { target.setLastModified(it) }
        Imported(
            file = target,
            name = facts.name ?: uri.lastPathSegment ?: target.name,
            mime = mime,
            size = target.length(),
            mtimeUnix = facts.modifiedMs?.div(1000),
            source = uri.toString(),
        )
    }

    override fun delete(imported: Imported) {
        imported.file.delete()
        held -= imported.file.path
    }

    override fun hold(imported: Imported) {
        held += imported.file.path
    }

    override fun clear() {
        dir.listFiles()?.filter { it.path !in held }?.forEach { it.deleteRecursively() }
    }

    private class Facts(val name: String?, val size: Long?, val modifiedMs: Long?)

    /** Name, size, and the original's modified time, from whichever columns the provider has. */
    private fun query(uri: Uri): Facts = try {
        resolver.query(uri, null, null, null, null)?.use { cursor ->
            if (!cursor.moveToFirst()) return@use null
            fun column(name: String) = cursor.getColumnIndex(name).takeIf { it >= 0 && !cursor.isNull(it) }
            val name = column(OpenableColumns.DISPLAY_NAME)?.let { cursor.getString(it) }
            val size = column(OpenableColumns.SIZE)?.let { cursor.getLong(it) }
            val modified = column(DocumentsContract.Document.COLUMN_LAST_MODIFIED)?.let { cursor.getLong(it) }
                ?: column(MediaStore.MediaColumns.DATE_MODIFIED)?.let { cursor.getLong(it) * 1000 }
            Facts(name, size, modified?.takeIf { it > 0 })
        } ?: Facts(null, null, null)
    } catch (_: Exception) {
        Facts(null, null, null)
    }

    /** Keep the extension so the platform decoders can tell what the copy is. */
    private fun extension(name: String?, mime: String?): String {
        val fromName = name?.substringAfterLast('.', "")?.lowercase()?.takeIf { it.length in 1..5 && it.all(Char::isLetterOrDigit) }
        if (fromName != null) return ".$fromName"
        val fromMime = mime?.substringAfter('/')?.substringBefore(';')?.takeIf { it.isNotEmpty() && it.all(Char::isLetterOrDigit) }
        return if (fromMime != null) ".$fromMime" else ""
    }

    private companion object {
        const val SPACE_MARGIN = 64L * 1024 * 1024
        /** Copies a scan in this process still reads. Gone with the process, as that scan is. */
        val held: MutableSet<String> = ConcurrentHashMap.newKeySet<String>()
    }
}
