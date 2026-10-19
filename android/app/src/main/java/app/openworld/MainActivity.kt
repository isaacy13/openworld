// SPDX-License-Identifier: Apache-2.0
package app.openworld

import android.content.ActivityNotFoundException
import android.content.ContentResolver
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.viewModels
import androidx.core.content.IntentCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import app.openworld.ui.OpenWorldApp
import app.openworld.ui.ScanState
import app.openworld.ui.ScanViewModel
import app.openworld.ui.theme.OpenWorldTheme
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch

class MainActivity : ComponentActivity() {
    private val model: ScanViewModel by viewModels { ScanViewModel.factory((application as OpenWorldApplication).env) }

    private val photos = registerForActivityResult(ActivityResultContracts.PickVisualMedia()) { uri ->
        if (uri != null) model.choose(uri)
    }
    private val files = registerForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        if (uri != null) model.choose(uri)
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        // A recreated window keeps the screen it had; only a fresh launch reads its share.
        if (savedInstanceState == null) handleShare(intent)
        lifecycleScope.launch {
            repeatOnLifecycle(Lifecycle.State.STARTED) {
                model.state.map { it.screen is ScanState.Scanning }.distinctUntilChanged().collect { keepScreenOn(it) }
            }
        }
        setContent {
            OpenWorldTheme {
                OpenWorldApp(
                    model = model,
                    onPhotos = { photos.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageAndVideo)) },
                    onFiles = { files.launch(arrayOf("image/*", "video/*")) },
                    onOpen = ::openPage,
                )
            }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        handleShare(intent)
    }

    /** A shared photo or video. Only a content:// stream is read; text and file:// are refused. */
    private fun handleShare(intent: Intent?) {
        if (intent?.action != Intent.ACTION_SEND) return
        val type = intent.type.orEmpty()
        val uri = IntentCompat.getParcelableExtra(intent, Intent.EXTRA_STREAM, Uri::class.java)
        val media = type.startsWith("image/") || type.startsWith("video/")
        if (uri == null || uri.scheme != ContentResolver.SCHEME_CONTENT || !media) {
            model.refuseShare()
            return
        }
        model.choose(uri)
    }

    /** Opens only the URL the core's `leave` call approved. */
    private fun openPage(url: String) {
        try {
            startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url)).addCategory(Intent.CATEGORY_BROWSABLE))
        } catch (_: ActivityNotFoundException) {
            model.openFailed()
        }
    }

    private fun keepScreenOn(on: Boolean) {
        if (on) {
            window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        } else {
            window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        }
    }
}
