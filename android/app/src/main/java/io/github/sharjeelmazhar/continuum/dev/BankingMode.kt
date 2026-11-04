package io.github.sharjeelmazhar.continuum.dev

import android.Manifest
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.provider.Settings
import android.util.Log

/**
 * Banking mode: many banking and wallet apps refuse to open while Developer options or USB
 * debugging are on, and many (UBL, for one) also refuse while any accessibility service from
 * outside the system runs. One switch turns Developer options, USB and wireless debugging and
 * Continuum's own accessibility helpers off; turning it off restores exactly what was on before. Needs WRITE_SECURE_SETTINGS, granted once from a computer
 * (`adb shell pm grant <package> android.permission.WRITE_SECURE_SETTINGS`); the grant stays
 * after debugging is switched off and across reboots.
 */
object BankingMode {
    private const val TAG = "Continuum"
    private const val PREFS = "banking_mode"
    private const val ADB_WIFI = "adb_wifi_enabled"

    fun grantCommand(context: Context) = "adb shell pm grant ${context.packageName} ${Manifest.permission.WRITE_SECURE_SETTINGS}"

    fun canSwitch(context: Context) = context.checkSelfPermission(Manifest.permission.WRITE_SECURE_SETTINGS) == PackageManager.PERMISSION_GRANTED

    private fun global(context: Context, key: String) = Settings.Global.getInt(context.contentResolver, key, 0)

    /** Developer options are on (what banking apps check). */
    fun developerOptionsOn(context: Context) =
        global(context, Settings.Global.DEVELOPMENT_SETTINGS_ENABLED) != 0 || global(context, Settings.Global.ADB_ENABLED) != 0

    /** Banking mode is on: Developer options and debugging are off, so banking apps open. */
    fun isOn(context: Context) = !developerOptionsOn(context)

    /** Turns banking mode on or off. False if Android refused (permission missing). */
    fun set(context: Context, on: Boolean): Boolean {
        if (!canSwitch(context)) return false
        val cr = context.contentResolver
        val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        return runCatching {
            if (on) {
                // Remember what was on, then switch it all off (wireless first, then USB, then the menu).
                val helpers = ourServices(context).filter { it in enabledServices(context) }
                prefs.edit()
                    .putBoolean("on", true)
                    .putStringSet("helpers", helpers.toSet())
                    .putInt("dev", global(context, Settings.Global.DEVELOPMENT_SETTINGS_ENABLED))
                    .putInt("adb", global(context, Settings.Global.ADB_ENABLED))
                    .putInt("wifi", global(context, ADB_WIFI))
                    .apply()
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) Settings.Global.putInt(cr, ADB_WIFI, 0)
                Settings.Global.putInt(cr, Settings.Global.ADB_ENABLED, 0)
                Settings.Global.putInt(cr, Settings.Global.DEVELOPMENT_SETTINGS_ENABLED, 0)
                if (helpers.isNotEmpty()) setServices(context, enabledServices(context) - helpers.toSet())
            } else {
                // Nothing saved, or nothing was on (e.g. switched off in Android settings): turn
                // the usual ones on, since that's what turning banking mode off asks for.
                val saved = prefs.getBoolean("on", false) &&
                    (prefs.getInt("dev", 0) != 0 || prefs.getInt("adb", 0) != 0 || prefs.getInt("wifi", 0) != 0)
                val dev = if (saved) prefs.getInt("dev", 1) else 1
                val adb = if (saved) prefs.getInt("adb", 1) else 1
                val wifi = if (saved) prefs.getInt("wifi", 0) else 0
                Settings.Global.putInt(cr, Settings.Global.DEVELOPMENT_SETTINGS_ENABLED, maxOf(dev, if (adb != 0 || wifi != 0) 1 else 0))
                Settings.Global.putInt(cr, Settings.Global.ADB_ENABLED, adb)
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R && wifi != 0) Settings.Global.putInt(cr, ADB_WIFI, wifi)
                val helpers = prefs.getStringSet("helpers", emptySet()).orEmpty()
                if (helpers.isNotEmpty()) setServices(context, enabledServices(context) + helpers)
                prefs.edit().putBoolean("on", false).putStringSet("helpers", emptySet()).apply()
            }
            true
        }.getOrElse {
            Log.i(TAG, "banking mode not switched: $it")
            false
        }
    }

    /** Asks Android to put the Banking mode tile in Quick Settings (Android 13+). */
    fun addTile(context: Context) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return
        val sb = context.getSystemService(android.app.StatusBarManager::class.java) ?: return
        val icon = android.graphics.drawable.Icon.createWithResource(context, io.github.sharjeelmazhar.continuum.R.drawable.ic_stat_continuum)
        runCatching {
            sb.requestAddTileService(
                android.content.ComponentName(context, BankingModeTile::class.java),
                context.getString(io.github.sharjeelmazhar.continuum.R.string.banking_title),
                icon,
                context.mainExecutor,
            ) { result -> Log.i(TAG, "add tile: $result") }
        }.onFailure { Log.i(TAG, "add tile failed: $it") }
    }

    fun openDeveloperOptions(context: Context) {
        val dev = Intent(Settings.ACTION_APPLICATION_DEVELOPMENT_SETTINGS).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        runCatching { context.startActivity(dev) }.onFailure {
            context.startActivity(Intent(Settings.ACTION_SETTINGS).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        }
    }

    private fun ourServices(context: Context) =
        listOf(io.github.sharjeelmazhar.continuum.clip.CopyWatcher::class.java, io.github.sharjeelmazhar.continuum.mirror.RemoteControl::class.java)
            .map { android.content.ComponentName(context, it).flattenToString() }

    private fun enabledServices(context: Context): Set<String> =
        Settings.Secure.getString(context.contentResolver, Settings.Secure.ENABLED_ACCESSIBILITY_SERVICES)
            .orEmpty().split(':').filter { it.isNotBlank() }.toSet()

    private fun setServices(context: Context, services: Set<String>) {
        val cr = context.contentResolver
        Settings.Secure.putString(cr, Settings.Secure.ENABLED_ACCESSIBILITY_SERVICES, services.joinToString(":"))
        Settings.Secure.putInt(cr, Settings.Secure.ACCESSIBILITY_ENABLED, if (services.isEmpty()) 0 else 1)
    }
}
