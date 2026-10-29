package io.github.sharjeelmazhar.continuum.service

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import io.github.sharjeelmazhar.continuum.data.PairedStore

/** Reconnects after a reboot or an app update, if a computer is paired. */
class BootReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val relevant = intent.action == Intent.ACTION_BOOT_COMPLETED || intent.action == Intent.ACTION_MY_PACKAGE_REPLACED
        if (relevant && PairedStore(context).load() != null) ContinuumService.start(context)
    }
}
