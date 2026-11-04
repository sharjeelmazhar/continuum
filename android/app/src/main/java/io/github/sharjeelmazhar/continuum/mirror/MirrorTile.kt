package io.github.sharjeelmazhar.continuum.mirror

import android.annotation.SuppressLint
import android.app.Activity
import android.app.PendingIntent
import android.content.Intent
import android.os.Build
import android.os.Bundle
import android.service.quicksettings.Tile
import android.service.quicksettings.TileService
import io.github.sharjeelmazhar.continuum.data.PairedStore
import io.github.sharjeelmazhar.continuum.service.ContinuumService

/**
 * Quick Settings tile: one tap opens Phone Mirroring on the paired computer (it asks the
 * computer, which then starts mirroring as if Mirror had been clicked there). Shows as on while
 * the phone is being mirrored.
 */
class MirrorTile : TileService() {
    override fun onStartListening() {
        super.onStartListening()
        qsTile?.apply {
            state = if (Mirror.streaming) Tile.STATE_ACTIVE else Tile.STATE_INACTIVE
            updateTile()
        }
    }

    @SuppressLint("StartActivityAndCollapseDeprecated")
    override fun onClick() {
        super.onClick()
        // Through a tiny activity, so the shade closes and the "Start now" prompt can show.
        val intent = Intent(this, MirrorRequestActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            startActivityAndCollapse(PendingIntent.getActivity(this, 0, intent, PendingIntent.FLAG_IMMUTABLE))
        } else {
            @Suppress("DEPRECATION")
            startActivityAndCollapse(intent)
        }
    }
}

/** Invisible: sends the mirror request (or opens the app if no computer is paired) and closes. */
class MirrorRequestActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        if (PairedStore(this).load() != null) {
            ContinuumService.requestMirror(this)
        } else {
            packageManager.getLaunchIntentForPackage(packageName)?.let { startActivity(it) }
        }
        finish()
    }
}
