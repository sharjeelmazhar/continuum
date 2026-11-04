package io.github.sharjeelmazhar.continuum.mirror

import android.media.projection.MediaProjectionConfig
import android.media.projection.MediaProjectionManager
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.result.contract.ActivityResultContracts

/** Invisible screen that shows Android's "Start now" prompt for screen sharing. */
class MirrorConsentActivity : ComponentActivity() {
    private val ask = registerForActivityResult(ActivityResultContracts.StartActivityForResult()) { r ->
        val data = r.data
        if (r.resultCode == RESULT_OK && data != null) MirrorService.begin(this, r.resultCode, data) else Mirror.denied()
        finish()
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        Mirror.cancelAsk(this)
        if (savedInstanceState != null) return
        if (!Mirror.hasPending() || MirrorService.instance?.ready == true) {
            finish()
            return
        }
        val mpm = getSystemService(MediaProjectionManager::class.java)
        // The whole screen only: mirroring a single app would look broken on the computer.
        val intent = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            mpm.createScreenCaptureIntent(MediaProjectionConfig.createConfigForDefaultDisplay())
        } else {
            mpm.createScreenCaptureIntent()
        }
        runCatching { ask.launch(intent) }.onFailure {
            Mirror.state("error")
            finish()
        }
    }
}
