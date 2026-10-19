// SPDX-License-Identifier: Apache-2.0
package app.openworld.platform

import android.net.Uri
import java.io.File

/** Where the app keeps the catalog, the poster pack, and results. */
class Paths(val bundles: File, val posters: File, val results: File)

/** A shared or picked file, copied into the app's cache. */
data class Imported(
    val file: File,
    val name: String,
    val mime: String?,
    val size: Long,
    /** The original's modified time, Unix seconds, when the provider gives one. */
    val mtimeUnix: Long?,
    /** The content URI, for the system thumbnail. */
    val source: String,
)

/** An import the app refuses. `code` names a core copy code or a shell string. */
class ImportRefused(val code: String) : Exception(code)

interface Imports {
    /** Copy `uri` into the cache. Throws [ImportRefused]. Runs off the main thread. */
    suspend fun copy(uri: Uri, onProgress: (Float?) -> Unit): Imported

    fun delete(imported: Imported)

    /** Remove every leftover copy. */
    fun clear()
}

data class DeviceStatus(val thermal: String?, val battery: Float?, val charging: Boolean?)

interface Device {
    fun status(): DeviceStatus

    /**
     * Watch heat, battery, and the app leaving the foreground while a scan runs.
     * `onStop` gets "heat", "battery", or "background". Call on the main thread; close to stop watching.
     */
    fun watch(onStop: (String) -> Unit): AutoCloseable
}

/**
 * Survives the process, so a scan the system killed shows as Incomplete on the next launch.
 * One mark per result folder: a scan ends only its own.
 */
interface ScanMarker {
    fun begin(out: File)

    fun end(out: File)

    /** The result folders of scans that never ended and are not running in this process. */
    fun leftovers(): List<File>
}

/** Shell refusal and notice codes, shown from R.string by the same name. */
object Shell {
    const val SCAN_KILLED = "scan_killed"
    const val IMPORT_FAILED = "import_failed"
    const val IMPORT_TYPE = "import_type"
    const val IMPORT_SPACE = "import_space"
    const val SHARE_NOT_FILE = "share_not_file"
    const val SCAN_BUSY = "scan_busy"
    const val OPEN_FAILED = "open_failed"
}
