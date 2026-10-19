// SPDX-License-Identifier: Apache-2.0
package app.openworld.platform

import android.content.Context
import android.content.res.AssetManager
import androidx.core.content.pm.PackageInfoCompat
import java.io.File
import java.io.IOException
import java.util.concurrent.ConcurrentHashMap

/**
 * In-progress marks, one per result folder, written synchronously so a killed process still leaves
 * them. A mark whose scan is still running in this process (a recreated screen) is not a leftover.
 */
class PrefsMarker(context: Context) : ScanMarker {
    private val prefs = context.applicationContext.getSharedPreferences("scan", Context.MODE_PRIVATE)

    override fun begin(out: File) {
        // Running first, so a leftover check between the two never sees an unclaimed mark.
        running += out.path
        prefs.edit().putString(KEY_PREFIX + out.path, out.path).commit()
    }

    override fun end(out: File) {
        prefs.edit().remove(KEY_PREFIX + out.path).commit()
        running -= out.path
    }

    override fun leftovers(): List<File> = prefs.all.entries
        .filter { it.key.startsWith(KEY_PREFIX) && it.key.removePrefix(KEY_PREFIX) !in running }
        .map { File(it.key.removePrefix(KEY_PREFIX)) }

    private companion object {
        const val KEY_PREFIX = "in_progress:"
        /** Scans begun in this process and not yet ended. Gone with the process, as a killed scan is. */
        val running: MutableSet<String> = ConcurrentHashMap.newKeySet<String>()
    }
}

/**
 * Copies the bundle catalog from the APK's assets to filesDir/bundles once per install or update.
 * The core checks every weight's SHA-256 itself when it loads a bundle.
 */
class BundleInstaller(private val context: Context, private val target: File) {
    fun install() {
        val info = context.packageManager.getPackageInfo(context.packageName, 0)
        val stamp = "${PackageInfoCompat.getLongVersionCode(info)}:${info.lastUpdateTime}"
        val stampFile = File(target.parentFile, "bundles.installed")
        if (target.isDirectory && runCatching { stampFile.readText() }.getOrNull() == stamp) return
        val staging = File(target.parentFile, "bundles.staging")
        staging.deleteRecursively()
        copyAssets(context.assets, "bundles", staging)
        target.deleteRecursively()
        if (!staging.renameTo(target)) throw IOException("bundles not installed")
        stampFile.writeText(stamp)
    }

    private fun copyAssets(assets: AssetManager, path: String, into: File) {
        val children = assets.list(path).orEmpty()
        if (children.isEmpty()) {
            // A file, or a folder the build left empty.
            val copied = runCatching {
                assets.open(path).use { input -> into.outputStream().use { input.copyTo(it, 1 shl 16) } }
            }
            if (copied.isFailure) into.delete()
            return
        }
        into.mkdirs()
        for (child in children) copyAssets(assets, "$path/$child", File(into, child))
    }
}
