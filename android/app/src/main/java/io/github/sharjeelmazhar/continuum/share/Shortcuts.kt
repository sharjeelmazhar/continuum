package io.github.sharjeelmazhar.continuum.share

import android.content.Context
import android.content.Intent
import androidx.core.content.pm.ShortcutInfoCompat
import androidx.core.content.pm.ShortcutManagerCompat
import androidx.core.graphics.drawable.IconCompat
import io.github.sharjeelmazhar.continuum.R
import io.github.sharjeelmazhar.continuum.ShareActivity
import io.github.sharjeelmazhar.continuum.data.PairedComputer

/**
 * Publishes the paired computer as a Direct Share target, so it shows up in the top row of the
 * Android share sheet with its own name and icon. Reporting each use lets Android rank it higher.
 */
object Shortcuts {
    /** Must match the category in res/xml/shortcuts.xml. */
    private const val CATEGORY = "io.github.sharjeelmazhar.continuum.category.SEND_TO_COMPUTER"

    fun publish(context: Context, pc: PairedComputer) {
        val icon = if (pc.type == "laptop") R.mipmap.ic_share_laptop else R.mipmap.ic_share_desktop
        val shortcut = ShortcutInfoCompat.Builder(context, pc.id)
            .setShortLabel(pc.name)
            .setLongLabel(context.getString(R.string.send_to, pc.name))
            .setIcon(IconCompat.createWithResource(context, icon))
            .setIntent(Intent(Intent.ACTION_SEND).setClass(context, ShareActivity::class.java))
            .setCategories(setOf(CATEGORY))
            .setLongLived(true)
            .setRank(0)
            .build()
        runCatching {
            // One computer at a time: a computer this phone was paired with before (replaced by a
            // new pairing) must not stay in the share sheet, where it would send to the new one.
            val flags = ShortcutManagerCompat.FLAG_MATCH_DYNAMIC or ShortcutManagerCompat.FLAG_MATCH_CACHED
            val stale = ShortcutManagerCompat.getShortcuts(context, flags).map { it.id }.filter { it != pc.id }
            if (stale.isNotEmpty()) ShortcutManagerCompat.removeLongLivedShortcuts(context, stale)
            ShortcutManagerCompat.pushDynamicShortcut(context, shortcut)
        }
    }

    fun reportUsed(context: Context, id: String) {
        runCatching { ShortcutManagerCompat.reportShortcutUsed(context, id) }
    }

    fun remove(context: Context) {
        runCatching {
            val ids = ShortcutManagerCompat.getDynamicShortcuts(context).map { it.id }
            ShortcutManagerCompat.removeLongLivedShortcuts(context, ids)
            ShortcutManagerCompat.removeAllDynamicShortcuts(context)
        }
    }
}
