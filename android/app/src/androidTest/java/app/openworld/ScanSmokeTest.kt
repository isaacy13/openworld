// SPDX-License-Identifier: Apache-2.0
package app.openworld

import android.graphics.Bitmap
import android.net.Uri
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.printToString
import androidx.lifecycle.ViewModelProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import app.openworld.core.Line
import app.openworld.core.NativeCore
import app.openworld.core.request
import app.openworld.platform.Imported
import app.openworld.ui.ScanViewModel
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.nio.ByteBuffer

/** Launch, install the demo posters, scan the demo still, and see the candidate headline. */
@RunWith(AndroidJUnit4::class)
class ScanSmokeTest {
    @get:Rule
    val compose = createAndroidComposeRule<MainActivity>()

    private fun string(id: Int) = compose.activity.getString(id)

    private fun shown(text: String) = compose.onAllNodesWithText(text).fetchSemanticsNodes().isNotEmpty()

    private fun model() = ViewModelProvider(compose.activity)[ScanViewModel::class.java]

    /** Waits for [condition]; on a timeout, fails with the app state and the screen tree. */
    private fun waitFor(what: String, ms: Long, condition: () -> Boolean) {
        try {
            compose.waitUntil(ms, condition)
        } catch (timeout: Throwable) {
            throw AssertionError("$what after $ms ms.\nState: ${model().state.value}\nScreen:\n${compose.onRoot(useUnmergedTree = true).printToString()}", timeout)
        }
    }

    @Test
    fun theLinkedCoreAnswers() {
        val answer = NativeCore { Line("missing_program", "") }.call(request("version"))
        assertTrue(answer.toString(), answer.optBoolean("ok"))
        assertTrue(answer.optBoolean("demo_posters"))
    }

    @Test
    fun aDemoScanFindsTheDemoCandidate() {
        compose.onNodeWithText(string(R.string.app_name)).assertExists()
        val install = string(R.string.posters_install_demo)
        val badge = string(R.string.posters_demo_badge)
        waitFor("No demo install button or badge", STARTUP_MS) { shown(install) || shown(badge) }
        // The footer can sit below the fold: a tap off screen lands nowhere.
        if (!shown(badge)) compose.onNodeWithText(install).performScrollTo().performClick()
        waitFor("No demo badge after installing", STARTUP_MS) { shown(badge) }

        // The still goes through the app's own path: PNG on disk, BitmapFactory, RGBA push.
        val activity = compose.activity
        val file = File(activity.cacheDir, "imports/demo-scene.png")
        file.parentFile?.mkdirs()
        val bitmap = Bitmap.createBitmap(DemoScene.WIDTH, DemoScene.HEIGHT, Bitmap.Config.ARGB_8888)
        bitmap.copyPixelsFromBuffer(ByteBuffer.wrap(DemoScene.rgba()))
        file.outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
        val model = model()
        compose.runOnUiThread {
            model.openImported(Imported(file, file.name, "image/png", file.length(), null, Uri.fromFile(file).toString()))
        }

        val analyze = string(R.string.analyze)
        waitFor("Analyze never enabled", SCAN_MS) {
            compose.onAllNodes(hasText(analyze) and isEnabled()).fetchSemanticsNodes().isNotEmpty()
        }
        compose.onNodeWithText(analyze).performClick()

        val headline = "Possible candidate. Not an identification."
        waitFor("No candidate headline", SCAN_MS) { shown(headline) }
        compose.onAllNodesWithText(headline)[0].assertExists()
        compose.onAllNodesWithText(string(R.string.open_fbi_page))[0].assertExists()
    }

    private companion object {
        const val STARTUP_MS = 60_000L
        const val SCAN_MS = 120_000L
    }
}
