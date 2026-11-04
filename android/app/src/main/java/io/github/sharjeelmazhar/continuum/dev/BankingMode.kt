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
 * debugging are on. One switch turns Developer options, USB debugging and wireless debugging
 * off, and turning it off again restores exactly what was on before. Continuum's accessibility
 * helpers stay on, so the computer can still use the phone (and switch banking mode back). Needs WRITE_SECURE_SETTINGS, granted once from a computer
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
                prefs.edit()
                    .putBoolean("on", true)
                    .putInt("dev", global(context, Settings.Global.DEVELOPMENT_SETTINGS_ENABLED))
                    .putInt("adb", global(context, Settings.Global.ADB_ENABLED))
                    .putInt("wifi", global(context, ADB_WIFI))
                    .apply()
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) Settings.Global.putInt(cr, ADB_WIFI, 0)
                Settings.Global.putInt(cr, Settings.Global.ADB_ENABLED, 0)
                Settings.Global.putInt(cr, Settings.Global.DEVELOPMENT_SETTINGS_ENABLED, 0)
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
                prefs.edit().putBoolean("on", false).apply()
            }
            true
        }.getOrElse {
            Log.i(TAG, "banking mode not switched: $it")
            false
        }
    }

    fun openDeveloperOptions(context: Context) {
        val dev = Intent(Settings.ACTION_APPLICATION_DEVELOPMENT_SETTINGS).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        runCatching { context.startActivity(dev) }.onFailure {
            context.startActivity(Intent(Settings.ACTION_SETTINGS).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        }
    }
}
