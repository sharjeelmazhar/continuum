package io.github.sharjeelmazhar.continuum.update

import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageInstaller
import android.os.Build
import android.util.Log
import androidx.core.content.IntentCompat
import io.github.sharjeelmazhar.continuum.BuildConfig
import java.io.File
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL
import java.security.MessageDigest
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long

/** A newer release on GitHub. */
data class Release(val version: String, val apkUrl: String, val shaUrl: String, val size: Long, val notes: String)

sealed interface UpdateState {
    data object None : UpdateState
    data class Available(val release: Release) : UpdateState
    data class Downloading(val release: Release, val done: Long) : UpdateState
    data class Failed(val release: Release, val reason: String) : UpdateState
    data object Installing : UpdateState
}

/**
 * Updates the app from GitHub Releases (the only place Continuum is published). The only network
 * request is to the GitHub API; the download is checked against the release's SHA-256 file, and
 * Android itself refuses an update that isn't signed with the same key as the installed app.
 */
object Updater {
    // The list, not /releases/latest: 0.x versions are published as pre-releases, which /latest skips.
    private const val API = "https://api.github.com/repos/${BuildConfig.REPO}/releases?per_page=10"
    private const val CHECK_EVERY_MS = 12L * 3600 * 1000
    private const val TAG = "Continuum"

    private val mutableState = MutableStateFlow<UpdateState>(UpdateState.None)
    val state: StateFlow<UpdateState> = mutableState

    /** Checks at most every 12 hours unless [force]. Returns a newer release, if any. */
    fun check(context: Context, force: Boolean = false): Release? {
        // Development builds aren't signed with the release key, so they can't be updated this way.
        if (BuildConfig.DEBUG) return null
        val prefs = context.getSharedPreferences("update", Context.MODE_PRIVATE)
        val now = System.currentTimeMillis()
        val release = if (!force && now - prefs.getLong("checked", 0) < CHECK_EVERY_MS) {
            // Checked recently: reuse what we found then (it survives the app being closed).
            prefs.getString("found", null)?.let { runCatching { parse(Json.parseToJsonElement(it).jsonObject) }.getOrNull() }
        } else {
            val raw = runCatching { fetchLatestRaw() }.onFailure { Log.i(TAG, "update check failed: $it") }.getOrNull()
            prefs.edit().putLong("checked", now).putString("found", raw?.toString()).apply()
            raw?.let { parse(it) }
        }
        if (release != null && isNewer(release.version, BuildConfig.VERSION_NAME)) {
            if (state.value is UpdateState.None || state.value is UpdateState.Failed) mutableState.value = UpdateState.Available(release)
            return release
        }
        return null
    }

    /** The newest published release that has an APK, as GitHub describes it. */
    private fun fetchLatestRaw(): JsonObject? =
        Json.parseToJsonElement(get(API).decodeToString()).jsonArray
            .map { it.jsonObject }
            .filter { parse(it) != null }
            .maxWithOrNull { a, b ->
                val (x, y) = parse(a)!!.version to parse(b)!!.version
                if (isNewer(x, y)) 1 else if (isNewer(y, x)) -1 else 0
            }

    /** A published release with an APK and its checksum file; drafts and others are skipped. */
    internal fun parse(json: JsonObject): Release? {
        if (json["draft"]?.jsonPrimitive?.content == "true") return null
        val version = json["tag_name"]?.jsonPrimitive?.content?.removePrefix("v") ?: return null
        val assets = json["assets"]?.jsonArray?.map { it.jsonObject } ?: return null
        fun asset(suffix: String): JsonObject? = assets.firstOrNull { it["name"]?.jsonPrimitive?.content?.endsWith(suffix) == true }
        val apk = asset(".apk") ?: return null
        val sha = asset(".apk.sha256") ?: return null
        return Release(
            version,
            apk["browser_download_url"]?.jsonPrimitive?.content ?: return null,
            sha["browser_download_url"]?.jsonPrimitive?.content ?: return null,
            apk["size"]?.jsonPrimitive?.long ?: 0,
            json["body"]?.jsonPrimitive?.content.orEmpty(),
        )
    }

    /** "0.10.0" > "0.9.3"; anything unparsable counts as older. */
    fun isNewer(candidate: String, current: String): Boolean {
        fun parts(v: String) = v.split('.', '-').map { it.toIntOrNull() ?: -1 }
        val a = parts(candidate)
        val b = parts(current)
        for (i in 0 until maxOf(a.size, b.size)) {
            val x = a.getOrElse(i) { 0 }
            val y = b.getOrElse(i) { 0 }
            if (x != y) return x > y
        }
        return false
    }

    /** Downloads and verifies the APK, then hands it to Android's installer. Blocking; run off the main thread. */
    fun downloadAndInstall(context: Context, release: Release) {
        val dir = File(context.cacheDir, "update").apply { mkdirs() }
        dir.listFiles()?.forEach { it.delete() }
        val apk = File(dir, "Continuum-${release.version}.apk")
        try {
            mutableState.value = UpdateState.Downloading(release, 0)
            val expected = get(release.shaUrl).decodeToString().trim().substringBefore(' ').lowercase()
            if (!expected.matches(Regex("[0-9a-f]{64}"))) throw IOException("bad checksum file")
            val digest = MessageDigest.getInstance("SHA-256")
            open(release.apkUrl).inputStream.use { input ->
                apk.outputStream().use { out ->
                    val buf = ByteArray(64 * 1024)
                    var done = 0L
                    var last = 0L
                    while (true) {
                        val n = input.read(buf)
                        if (n < 0) break
                        out.write(buf, 0, n)
                        digest.update(buf, 0, n)
                        done += n
                        if (done - last > 256 * 1024) {
                            last = done
                            mutableState.value = UpdateState.Downloading(release, done)
                        }
                    }
                }
            }
            val actual = digest.digest().joinToString("") { "%02x".format(it) }
            if (actual != expected) throw IOException("the download was damaged; try again")
            mutableState.value = UpdateState.Installing
            install(context, apk)
        } catch (e: Exception) {
            Log.i(TAG, "update failed: $e")
            apk.delete()
            mutableState.value = UpdateState.Failed(release, e.message ?: e.toString())
        }
    }

    private fun install(context: Context, apk: File) {
        val installer = context.packageManager.packageInstaller
        val params = PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL).apply {
            setAppPackageName(context.packageName)
            // Android 12+: once Continuum installed itself, later updates may need no confirmation.
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) setRequireUserAction(PackageInstaller.SessionParams.USER_ACTION_NOT_REQUIRED)
        }
        val id = installer.createSession(params)
        installer.openSession(id).use { session ->
            session.openWrite("continuum.apk", 0, apk.length()).use { out ->
                apk.inputStream().use { it.copyTo(out) }
                session.fsync(out)
            }
            val callback = PendingIntent.getBroadcast(
                context, id,
                Intent(context, InstallResultReceiver::class.java),
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_MUTABLE,
            )
            session.commit(callback.intentSender)
        }
    }

    internal fun onInstallFailed(message: String?) {
        val release = (state.value as? UpdateState.Downloading)?.release
        mutableState.value = UpdateState.None
        Log.i(TAG, "install did not finish: $message ($release)")
    }

    private fun open(url: String): HttpURLConnection {
        var current = URL(url)
        repeat(5) {
            val c = current.openConnection() as HttpURLConnection
            c.connectTimeout = 15_000
            c.readTimeout = 30_000
            c.instanceFollowRedirects = false
            c.setRequestProperty("User-Agent", "Continuum/${BuildConfig.VERSION_NAME}")
            c.setRequestProperty("Accept", "application/vnd.github+json, application/octet-stream")
            when (c.responseCode) {
                in 200..299 -> return c
                301, 302, 303, 307, 308 -> {
                    val next = URL(current, c.getHeaderField("Location"))
                    // GitHub sends release files from its own HTTPS hosts only.
                    if (next.protocol != "https") throw IOException("insecure redirect")
                    current = next
                    c.disconnect()
                }
                else -> throw IOException("HTTP ${c.responseCode}")
            }
        }
        throw IOException("too many redirects")
    }

    private fun get(url: String): ByteArray = open(url).inputStream.use { it.readBytes() }
}

/** Android reports the install here; if it needs the user's OK, we show its confirmation screen. */
class InstallResultReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        when (intent.getIntExtra(PackageInstaller.EXTRA_STATUS, PackageInstaller.STATUS_FAILURE)) {
            PackageInstaller.STATUS_PENDING_USER_ACTION -> {
                IntentCompat.getParcelableExtra(intent, Intent.EXTRA_INTENT, Intent::class.java)
                    ?.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                    ?.let { runCatching { context.startActivity(it) } }
            }
            PackageInstaller.STATUS_SUCCESS -> Unit // The app restarts as the new version.
            else -> Updater.onInstallFailed(intent.getStringExtra(PackageInstaller.EXTRA_STATUS_MESSAGE))
        }
    }
}
