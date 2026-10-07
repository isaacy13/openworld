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

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        handleShare(intent)
        setContent {
            OpenWorldApp(model = model, onChoose = { picker.launch(arrayOf("image/*", "video/*")) }, onOpen = { url ->
                startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url)))
            })
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        handleShare(intent)
    }

    private fun handleShare(intent: Intent?) {
        if (intent?.action == Intent.ACTION_SEND) {
            val uri = intent.getParcelableExtra(Intent.EXTRA_STREAM, Uri::class.java)
            if (uri != null) model.choose(uri, contentResolver)
        }
    }
}
