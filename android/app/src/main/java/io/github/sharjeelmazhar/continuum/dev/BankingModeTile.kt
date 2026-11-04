package io.github.sharjeelmazhar.continuum.dev

import android.annotation.SuppressLint
import android.app.PendingIntent
import android.content.Intent
import android.os.Build
import android.service.quicksettings.Tile
import android.service.quicksettings.TileService
import android.widget.Toast
import io.github.sharjeelmazhar.continuum.MainActivity
import io.github.sharjeelmazhar.continuum.R

/** Quick Settings tile: one tap turns banking mode on or off (see [BankingMode]). */
class BankingModeTile : TileService() {
    override fun onStartListening() {
        super.onStartListening()
        BankingMode.setTileAdded(this, true)
        update()
    }

    override fun onTileAdded() {
        super.onTileAdded()
        BankingMode.setTileAdded(this, true)
    }

    override fun onTileRemoved() {
        super.onTileRemoved()
        BankingMode.setTileAdded(this, false)
    }

    @SuppressLint("StartActivityAndCollapseDeprecated")
    override fun onClick() {
        super.onClick()
        if (!BankingMode.canSwitch(this)) {
            // Not set up yet: the app explains the one-time step.
            val intent = Intent(this, MainActivity::class.java)
                .putExtra(MainActivity.EXTRA_SHOW, MainActivity.SHOW_BANKING)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
                startActivityAndCollapse(PendingIntent.getActivity(this, 0, intent, PendingIntent.FLAG_IMMUTABLE))
            } else {
                @Suppress("DEPRECATION")
                startActivityAndCollapse(intent)
            }
            return
        }
        val on = !BankingMode.isOn(this)
        if (BankingMode.set(this, on)) {
            Toast.makeText(this, if (on) R.string.banking_on_toast else R.string.banking_off_toast, Toast.LENGTH_LONG).show()
        }
        update()
    }

    private fun update() {
        val tile = qsTile ?: return
        val ready = BankingMode.canSwitch(this)
        tile.state = when {
            !ready -> Tile.STATE_INACTIVE
            BankingMode.isOn(this) -> Tile.STATE_ACTIVE
            else -> Tile.STATE_INACTIVE
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            tile.subtitle = getString(
                when {
                    !ready -> R.string.banking_tile_setup
                    BankingMode.isOn(this) -> R.string.banking_tile_on
                    else -> R.string.banking_tile_off
                },
            )
        }
        tile.updateTile()
    }
}
