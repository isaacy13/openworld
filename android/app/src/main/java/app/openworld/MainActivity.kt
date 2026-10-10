// SPDX-License-Identifier: Apache-2.0
package app.openworld

import android.content.ClipData
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.lifecycle.ViewModelProvider

class MainActivity : ComponentActivity() {
    private lateinit var model: FlowModel
    private val picker = registerForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        if (uri != null) model.choose(uri, contentResolver)
    }

    companion object {
        /** Photos and video, and every other file. An extensionless video stays selectable. */
        internal val chooseFileMimeTypes = arrayOf("image/*", "video/*", "*/*")
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        model = ViewModelProvider(this).get(FlowModel::class.java)
        if (!model.launchHandled) {
            model.launchHandled = true
            handleShare(intent)
        }
        setContent {
            OpenWorldApp(model = model, onChoose = { picker.launch(chooseFileMimeTypes) }, onOpen = { url ->
                startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url)))
            })
        }
    }

    override fun onDestroy() {
        if (::model.isInitialized && !isChangingConfigurations) model.abandonScan()
        super.onDestroy()
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        handleShare(intent)
    }

    private fun handleShare(intent: Intent?) {
        if (intent?.action != Intent.ACTION_SEND) return
        val uri = shareUri(intent)
        if (uri != null) model.choose(uri, contentResolver) else model.noteUnreadableShare()
    }

    /** The file on a share. A parcel, a text address, or the clip. */
    internal fun shareUri(intent: Intent): Uri? {
        val parcel = if (android.os.Build.VERSION.SDK_INT >= 33) {
            intent.getParcelableExtra(Intent.EXTRA_STREAM, Uri::class.java)
        } else {
            @Suppress("DEPRECATION")
            intent.getParcelableExtra(Intent.EXTRA_STREAM)
        }
        if (parcel != null) return parcel
        val text = intent.getStringExtra(Intent.EXTRA_STREAM)
        fileUri(text)?.let { return it }
        val clip: ClipData = intent.clipData ?: return null
        for (index in 0 until clip.itemCount) {
            fileUri(clip.getItemAt(index).uri)?.let { return it }
        }
        return null
    }

    private fun fileUri(value: String?): Uri? {
        if (value.isNullOrBlank()) return null
        return fileUri(Uri.parse(value))
    }

    private fun fileUri(uri: Uri?): Uri? {
        val scheme = uri?.scheme
        if (scheme == "content" || scheme == "file") return uri
        return null
    }
}
