package io.github.sharjeelmazhar.continuum.data

import android.content.Context
import androidx.core.content.edit

/** The computer this phone is paired with. One for now (SRS Q-2). */
data class PairedComputer(
    val id: String,
    val name: String,
    val type: String,
    /** Pinned certificate fingerprint, hex. */
    val fingerprint: String,
    val hosts: List<String>,
    val port: Int,
)

/** SharedPreferences writes go through a temp file and rename, so a power cut can't corrupt them. */
class PairedStore(context: Context) {
    private val prefs = context.applicationContext.getSharedPreferences("paired", Context.MODE_PRIVATE)

    fun load(): PairedComputer? {
        val id = prefs.getString("id", null) ?: return null
        return PairedComputer(
            id = id,
            name = prefs.getString("name", null) ?: "Computer",
            type = prefs.getString("type", null) ?: "desktop",
            fingerprint = prefs.getString("fp", null) ?: return null,
            hosts = prefs.getString("hosts", null)?.split(',')?.filter { it.isNotEmpty() } ?: return null,
            port = prefs.getInt("port", 0).takeIf { it > 0 } ?: return null,
        )
    }

    fun save(pc: PairedComputer) = prefs.edit(commit = true) {
        putString("id", pc.id)
        putString("name", pc.name)
        putString("type", pc.type)
        putString("fp", pc.fingerprint)
        putString("hosts", pc.hosts.joinToString(","))
        putInt("port", pc.port)
    }

    fun clear() = prefs.edit(commit = true) { clear() }
}
