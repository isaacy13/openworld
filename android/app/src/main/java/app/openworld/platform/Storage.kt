// SPDX-License-Identifier: Apache-2.0
package app.openworld.platform

import android.content.Context
import android.content.res.AssetManager
import androidx.core.content.pm.PackageInfoCompat
import java.io.File
import java.io.IOException

/** The in-progress marker, written synchronously so a killed process still leaves it. */
class PrefsMarker(context: Context) : ScanMarker {
    private val prefs = context.applicationContext.getSharedPreferences("scan", Context.MODE_PRIVATE)

    override fun begin(out: File) {
        prefs.edit().putString(KEY, out.path).commit()
    }

    override fun end() {
        prefs.edit().remove(KEY).commit()
    }

    override fun leftover(): File? = prefs.getString(KEY, null)?.let(::File)

    private companion object {
        const val KEY = "in_progress"
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
