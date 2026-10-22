// SPDX-License-Identifier: Apache-2.0
package app.openworld

import android.app.ActivityManager
import android.app.Application
import app.openworld.core.Line
import app.openworld.core.NativeCore
import app.openworld.media.AndroidMediaReader
import app.openworld.platform.AndroidDevice
import app.openworld.platform.AndroidImports
import app.openworld.platform.BundleInstaller
import app.openworld.platform.Paths
import app.openworld.platform.PrefsMarker
import app.openworld.platform.PrefsTerms
import java.io.File

class OpenWorldApplication : Application() {
    val env: AppEnv by lazy {
        val paths = Paths(
            bundles = File(filesDir, "bundles"),
            posters = File(filesDir, "posters"),
            results = File(filesDir, "results"),
        )
        AppEnv(
            core = NativeCore { Line("missing_program", getString(R.string.core_missing_program)) },
            reader = AndroidMediaReader(checkNotNull(getSystemService(ActivityManager::class.java))),
            device = AndroidDevice(this),
            imports = AndroidImports(contentResolver, File(cacheDir, "imports")),
            marker = PrefsMarker(this),
            terms = PrefsTerms(this),
            paths = paths,
            install = {
                paths.results.mkdirs()
                BundleInstaller(this, paths.bundles).install()
            },
        )
    }
}
