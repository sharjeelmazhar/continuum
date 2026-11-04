package io.github.sharjeelmazhar.continuum.mirror

import android.annotation.SuppressLint
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import io.github.sharjeelmazhar.continuum.R
import io.github.sharjeelmazhar.continuum.proto.Envelope
import io.github.sharjeelmazhar.continuum.proto.FeatureType
import io.github.sharjeelmazhar.continuum.proto.Limits
import io.github.sharjeelmazhar.continuum.service.Channels
import javax.net.ssl.SSLSocket
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonObject

@Serializable
data class MirrorStart(val token: String = "", val max: Int = 1920)

@Serializable
data class MirrorState(val state: String, val control: Boolean)

/**
 * Screen mirroring to the computer (docs/PROTOCOL.md, "Screen mirroring"): `mirror.start`
 * asks the user once (Android's "Start now"), then [MirrorService] streams the screen. The
 * permission is kept for a few minutes after the computer closes the window, so reopening it
 * needs no tap on the phone.
 */
object Mirror {
    /** The connection mirroring runs over: its computer's name, the control channel, a data socket. */
    class Link(val computer: String, val send: (Envelope) -> Unit, val open: () -> SSLSocket)

    data class Request(val token: String, val max: Int)

    private const val TAG = "Continuum"
    private const val ASK_ID = 0x4d49

    @Volatile
    var link: Link? = null
        private set

    @Volatile
    private var pending: Request? = null

    /** True while a stream is going to the computer; remote control works only then. */
    @Volatile
    var streaming = false

    fun handle(context: Context, link: Link, type: String, body: JsonObject) {
        when (type) {
            FeatureType.MIRROR_START -> {
                val start = runCatching { Envelope(type, body).body(MirrorStart.serializer()) }.getOrNull() ?: return
                if (!Limits.validToken(start.token)) return
                start(context, link, Request(start.token, start.max.coerceIn(480, 2560)))
            }
            FeatureType.MIRROR_STOP -> if (this.link === link) stop(context)
            FeatureType.MIRROR_INPUT -> if (this.link === link && streaming) {
                when (val input = MirrorInput.parse(body)) {
                    null -> {}
                    // Not an input: the computer asks for a key frame to recover from lost frames.
                    MirrorInput.Key("sync") -> MirrorService.instance?.requestSync()
                    else -> RemoteControl.instance?.perform(input)
                }
            }
        }
    }

    private fun start(context: Context, link: Link, req: Request) {
        this.link = link
        val svc = MirrorService.instance
        if (svc != null && svc.ready) {
            pending = null
            svc.stream(req)
            return
        }
        pending = req
        state("consent")
        ask(context.applicationContext, link.computer)
    }

    /** The consent activity and the service take the waiting request once. */
    fun takePending(): Request? = pending.also { pending = null }

    fun hasPending() = pending != null

    fun denied() {
        pending = null
        state("denied")
    }

    fun stop(context: Context) {
        pending = null
        cancelAsk(context)
        MirrorService.instance?.stopStream()
    }

    /** The control connection closed: nothing may stream to it any more. */
    fun detach(context: Context, link: Link) {
        if (this.link !== link) return
        stop(context)
        this.link = null
    }

    fun state(state: String) {
        val l = link ?: return
        runCatching {
            l.send(Envelope.of(FeatureType.MIRROR_STATE, MirrorState.serializer(), MirrorState(state, RemoteControl.instance != null)))
        }.onFailure { Log.i(TAG, "mirror.state not sent: $it") }
    }

    /** The remote-control service was turned on or off: tell a running stream's computer. */
    fun controlChanged() {
        if (streaming) state("control")
    }

    /**
     * Opens Android's screen-sharing prompt. Background apps may not open screens, so a
     * notification does it too (and is the only way on phones that block it, like MIUI).
     */
    @SuppressLint("MissingPermission")
    private fun ask(context: Context, computer: String) {
        val intent = Intent(context, MirrorConsentActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        Channels.ensure(context)
        val tap = PendingIntent.getActivity(context, ASK_ID, intent, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        val n = NotificationCompat.Builder(context, Channels.MIRROR)
            .setSmallIcon(R.drawable.ic_stat_continuum)
            .setContentTitle(context.getString(R.string.mirror_ask_title, computer))
            .setContentText(context.getString(R.string.mirror_ask_text))
            .setContentIntent(tap)
            .setAutoCancel(true)
            .setPriority(NotificationCompat.PRIORITY_HIGH)
            .setTimeoutAfter(120_000)
            .build()
        runCatching { NotificationManagerCompat.from(context).notify(ASK_ID, n) }
        // After the notification, so the prompt (if Android lets it open) can remove it.
        runCatching { context.startActivity(intent) }.onFailure { Log.i(TAG, "can't open the mirroring prompt: $it") }
    }

    fun cancelAsk(context: Context) = NotificationManagerCompat.from(context).cancel(ASK_ID)
}
