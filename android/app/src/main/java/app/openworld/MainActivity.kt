// SPDX-License-Identifier: Apache-2.0
package app.openworld

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts

class MainActivity : ComponentActivity() {
    private val model = FlowModel()
    private val picker = registerForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        if (uri != null) model.choose(uri, contentResolver)
    }

    companion object {
        /** Photos and video, and every other file. An extensionless video stays selectable. */
        internal val chooseFileMimeTypes = arrayOf("image/*", "video/*", "*/*")
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        handleShare(intent)
        setContent {
            OpenWorldApp(model = model, onChoose = { picker.launch(chooseFileMimeTypes) }, onOpen = { url ->
                startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url)))
            })
        }
    }

    override fun onDestroy() {
        model.abandonScan()
        super.onDestroy()
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        handleShare(intent)
    }

    private fun handleShare(intent: Intent?) {
        if (intent?.action == Intent.ACTION_SEND) {
            val uri = if (android.os.Build.VERSION.SDK_INT >= 33) {
                intent.getParcelableExtra(Intent.EXTRA_STREAM, Uri::class.java)
            } else {
                @Suppress("DEPRECATION")
                intent.getParcelableExtra(Intent.EXTRA_STREAM)
            }
            if (uri != null) model.choose(uri, contentResolver)
        }
    }
}
