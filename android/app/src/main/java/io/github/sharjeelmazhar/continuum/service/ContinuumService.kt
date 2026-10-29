package io.github.sharjeelmazhar.continuum.service

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.PackageManager
import android.content.pm.ServiceInfo
import android.net.ConnectivityManager
import android.net.Network
import android.os.BatteryManager
import android.os.Build
import android.os.IBinder
import android.provider.Settings
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat
import io.github.sharjeelmazhar.continuum.MainActivity
import io.github.sharjeelmazhar.continuum.R
import io.github.sharjeelmazhar.continuum.data.PairedComputer
import io.github.sharjeelmazhar.continuum.data.PairedStore
import io.github.sharjeelmazhar.continuum.net.Discovery
import io.github.sharjeelmazhar.continuum.net.Identity
import io.github.sharjeelmazhar.continuum.net.Session
import io.github.sharjeelmazhar.continuum.net.Tls
import io.github.sharjeelmazhar.continuum.net.UnsupportedAndroidException
import io.github.sharjeelmazhar.continuum.net.WrongComputerException
import io.github.sharjeelmazhar.continuum.proto.BatteryStatus
import io.github.sharjeelmazhar.continuum.proto.Crypto
import io.github.sharjeelmazhar.continuum.proto.Envelope
import io.github.sharjeelmazhar.continuum.proto.Hello
import io.github.sharjeelmazhar.continuum.proto.MessageType
import io.github.sharjeelmazhar.continuum.proto.PairAccept
import io.github.sharjeelmazhar.continuum.proto.PairReject
import io.github.sharjeelmazhar.continuum.proto.PairRequest
import io.github.sharjeelmazhar.continuum.proto.PairUri
import io.github.sharjeelmazhar.continuum.proto.Protocol
import io.github.sharjeelmazhar.continuum.ui.LOCAL_NETWORK
import java.io.IOException
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetSocketAddress
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull

/**
 * Foreground service that owns the connection to the paired computer: pairs, connects,
 * reconnects with backoff, keeps the link alive and reports the battery (docs/PROTOCOL.md).
 */
class ContinuumService : Service() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val wake = Channel<Unit>(Channel.CONFLATED)
    private lateinit var store: PairedStore
    private var loop: Job? = null
    @Volatile private var session: Session? = null
    @Volatile private var lastBattery: BatteryStatus? = null
    @Volatile private var sentBattery: BatteryStatus? = null
    @Volatile private var wakeSocket: DatagramSocket? = null

    private class PairRejected(reason: String) : Exception(reason)
    private class UnpairedByComputer : Exception()

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        store = PairedStore(this)
        val nm = getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(
            NotificationChannel(CHANNEL, getString(R.string.channel_connection), NotificationManager.IMPORTANCE_MIN),
        )
        val type = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE else 0
        ServiceCompat.startForeground(this, NOTIFICATION_ID, notification(Link.state.value), type)
        scope.launch { Link.state.collect { nm.notify(NOTIFICATION_ID, notification(it)) } }
        ContextCompat.registerReceiver(this, batteryReceiver, IntentFilter(Intent.ACTION_BATTERY_CHANGED), ContextCompat.RECEIVER_NOT_EXPORTED)
        getSystemService(ConnectivityManager::class.java).registerDefaultNetworkCallback(networkCallback)
        scope.launch { listenForWake() }
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_PAIR -> PairUri.parse(intent.getStringExtra(EXTRA_URI).orEmpty())?.let { restart(it) }
            ACTION_UNPAIR -> unpair()
            // A plain start (app opened, boot) also means "try now" if we're waiting.
            else -> if (loop?.isActive != true) restart(null) else wake.trySend(Unit)
        }
        return START_STICKY
    }

    override fun onDestroy() {
        unregisterReceiver(batteryReceiver)
        runCatching { getSystemService(ConnectivityManager::class.java).unregisterNetworkCallback(networkCallback) }
        session?.close()
        wakeSocket?.close()
        scope.cancel()
        super.onDestroy()
    }

    private fun restart(pairUri: PairUri?) {
        val old = loop
        session?.close()
        loop = scope.launch {
            old?.cancel()
            old?.join()
            run(pairUri)
        }
    }

    private fun unpair() {
        val s = session
        scope.launch {
            runCatching { s?.send(Envelope(MessageType.UNPAIR)) }
            s?.close()
            loop?.cancel()
            store.clear()
            Link.mutableState.value = LinkState.Unpaired()
            stopSelf()
        }
    }

    private suspend fun run(initialPairUri: PairUri?) {
        var pairUri = initialPairUri
        var backoffMs = 1_000L
        while (scope.isActive) {
            val pc = store.load()
            if (pairUri == null && pc == null) {
                if (Link.state.value !is LinkState.Unpaired) Link.mutableState.value = LinkState.Unpaired()
                stopSelf()
                return
            }
            try {
                if (pairUri != null) {
                    val uri = pairUri
                    pairUri = null
                    Link.mutableState.value = LinkState.Pairing(uri.name.ifEmpty { "your computer" })
                    val session = Session(connectAny(uri.hosts, uri.port, uri.fingerprint))
                    val paired = try {
                        pair(session, uri)
                    } catch (e: Exception) {
                        session.close()
                        throw e
                    }
                    backoffMs = 1_000
                    runSession(session, paired, justPaired = true)
                } else if (pc != null) {
                    if (!hasLocalNetworkPermission()) {
                        // Opening the app (or granting in Settings and coming back) wakes us.
                        Link.mutableState.value = LinkState.Offline(pc.name, getString(R.string.local_network_missing))
                        wake.receive()
                        continue
                    }
                    if (!hasNetwork()) {
                        // No network at all: don't burn battery retrying; the network callback wakes us.
                        Link.mutableState.value = LinkState.Offline(pc.name, getString(R.string.no_network))
                        wake.receive()
                        continue
                    }
                    Link.mutableState.value = LinkState.Connecting(pc.name)
                    val socket = connectKnownOrDiscovered(pc)
                    backoffMs = 1_000
                    runSession(Session(socket), pc, justPaired = false)
                }
            } catch (e: PairRejected) {
                setFailure(pc, e.message ?: "The computer refused the pairing.")
            } catch (_: UnpairedByComputer) {
                Log.i(TAG, "computer removed the pairing")
                store.clear()
                Link.mutableState.value = LinkState.Unpaired("The computer removed this phone. Scan a new code to pair again.")
                continue
            } catch (e: UnsupportedAndroidException) {
                setFailure(pc, "Continuum needs Android 10 or newer for now.")
                if (pc != null) return
            } catch (e: Exception) {
                if (e is kotlinx.coroutines.CancellationException) throw e
                Log.i(TAG, "connection failed: $e")
                val name = pc?.name ?: "your computer"
                val reason = if (causedByWrongComputer(e)) {
                    "$name has a different security key now. Remove it here and pair again."
                } else {
                    "Can't reach $name. Make sure Continuum is open on it and both devices are on the same Wi-Fi."
                }
                setFailure(pc, reason)
            }
            withTimeoutOrNull(backoffMs) { wake.receive() }
            backoffMs = (backoffMs * 2).coerceAtMost(MAX_BACKOFF_MS)
        }
    }

    private fun setFailure(pc: PairedComputer?, reason: String) {
        Link.mutableState.value = if (pc == null) LinkState.Unpaired(reason) else LinkState.Offline(pc.name, reason)
    }

    private fun causedByWrongComputer(e: Throwable): Boolean =
        generateSequence(e) { it.cause }.take(8).any { it is WrongComputerException }

    private fun connectAny(hosts: List<String>, port: Int, fingerprint: ByteArray): javax.net.ssl.SSLSocket {
        var last: Exception = IOException("no address")
        for (host in hosts) {
            try {
                return Tls.connect(host, port, Identity.get(), fingerprint)
            } catch (e: Exception) {
                if (causedByWrongComputer(e) || e is UnsupportedAndroidException) throw e
                last = e
            }
        }
        throw last
    }

    /** Tries the saved addresses; if they fail, looks the computer up by id on the LAN (its IP may have changed). */
    private suspend fun connectKnownOrDiscovered(pc: PairedComputer): javax.net.ssl.SSLSocket {
        val fp = Crypto.unhex(pc.fingerprint)!!
        return try {
            connectAny(pc.hosts, pc.port, fp)
        } catch (e: Exception) {
            if (causedByWrongComputer(e) || e is UnsupportedAndroidException) throw e
            val (host, port) = Discovery.find(this, pc.id) ?: throw e
            if (host in pc.hosts && port == pc.port) throw e
            Log.i(TAG, "${pc.name} found at a new address")
            connectAny(listOf(host), port, fp).also {
                // Saved only after the pinned TLS handshake proved it's our computer.
                store.save(pc.copy(hosts = listOf(host), port = port))
            }
        }
    }

    private fun hasLocalNetworkPermission(): Boolean = Build.VERSION.SDK_INT < 37 ||
        ContextCompat.checkSelfPermission(this, LOCAL_NETWORK) == PackageManager.PERMISSION_GRANTED

    private fun hasNetwork(): Boolean = getSystemService(ConnectivityManager::class.java).activeNetwork != null

    private fun pair(session: Session, uri: PairUri): PairedComputer {
        val me = Identity.get()
        val request = PairRequest(deviceName(), deviceType(), Crypto.pairingProof(uri.secret, uri.fingerprint, me.fingerprint))
        session.send(Envelope.of(MessageType.PAIR_REQUEST, PairRequest.serializer(), request))
        val reply = session.receive()
        when (reply.type) {
            MessageType.PAIR_ACCEPT -> {
                val accept = reply.body(PairAccept.serializer())
                if (accept.id != Crypto.deviceId(uri.fingerprint)) throw IOException("computer id mismatch")
                val pc = PairedComputer(accept.id, accept.name, accept.deviceType, Crypto.hex(uri.fingerprint), uri.hosts, uri.port)
                store.save(pc)
                Log.i(TAG, "paired with ${pc.name}")
                return pc
            }
            MessageType.PAIR_REJECT -> throw PairRejected(reply.body(PairReject.serializer()).reason)
            else -> throw IOException("unexpected reply ${reply.type}")
        }
    }

    private suspend fun runSession(session: Session, pc: PairedComputer, justPaired: Boolean) {
        val me = Identity.get()
        try {
            session.send(Envelope.of(MessageType.HELLO, Hello.serializer(), Hello(Protocol.VERSION, me.deviceId, deviceName(), deviceType(), Protocol.CAPS)))
            val reply = session.receive()
            // The computer answers hello with pair.reject when it no longer knows this phone.
            if (reply.type == MessageType.PAIR_REJECT && !justPaired) throw UnpairedByComputer()
            if (reply.type != MessageType.HELLO) throw IOException("expected hello, got ${reply.type}")
            session.socket.soTimeout = Protocol.IDLE_TIMEOUT_MS
            this.session = session
            sentBattery = null
            Link.mutableState.value = LinkState.Connected(pc.name)
            Log.i(TAG, "connected to ${pc.name}")
            sendBatteryIfChanged()
            val pinger = scope.launch {
                while (isActive) {
                    delay(Protocol.PING_INTERVAL_MS)
                    runCatching { session.send(Envelope(MessageType.PING)) }.onFailure { session.close() }
                }
            }
            try {
                while (true) {
                    val msg = session.receive()
                    when (msg.type) {
                        MessageType.UNPAIR -> throw UnpairedByComputer()
                        MessageType.PING -> session.send(Envelope(MessageType.PONG))
                        else -> {}
                    }
                }
            } finally {
                pinger.cancel()
            }
        } finally {
            this.session = null
            session.close()
        }
    }

    private fun sendBatteryIfChanged() {
        val battery = lastBattery ?: return
        val s = session ?: return
        if (battery == sentBattery) return
        scope.launch {
            runCatching { s.send(Envelope.of(MessageType.BATTERY_STATUS, BatteryStatus.serializer(), battery)) }
                .onSuccess { sentBattery = battery }
        }
    }

    /**
     * Listens for the computer's "I'm back" nudge (desktop `wake.rs`) so we reconnect at once
     * instead of at the next retry. Only a nudge from our paired computer's id counts, and it
     * only triggers a normal (pinned) connection attempt.
     */
    private fun listenForWake() {
        val socket = runCatching {
            DatagramSocket(null).apply {
                reuseAddress = true
                bind(InetSocketAddress(Protocol.DEFAULT_PORT))
            }
        }.getOrElse {
            Log.i(TAG, "wake listener unavailable: $it")
            return
        }
        wakeSocket = socket
        val buf = ByteArray(128)
        socket.use {
            while (scope.isActive) {
                val packet = DatagramPacket(buf, buf.size)
                runCatching { it.receive(packet) }.onFailure { return }
                val text = String(packet.data, 0, packet.length, Charsets.UTF_8)
                val id = store.load()?.id ?: continue
                if (text == "$WAKE_PREFIX$id" && session == null) wake.trySend(Unit)
            }
        }
    }

    private val batteryReceiver = object : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) {
            val level = intent.getIntExtra(BatteryManager.EXTRA_LEVEL, -1)
            val scale = intent.getIntExtra(BatteryManager.EXTRA_SCALE, 100)
            if (level < 0 || scale <= 0) return
            val status = intent.getIntExtra(BatteryManager.EXTRA_STATUS, -1)
            val charging = status == BatteryManager.BATTERY_STATUS_CHARGING || status == BatteryManager.BATTERY_STATUS_FULL
            lastBattery = BatteryStatus((level * 100 / scale).coerceIn(0, 100), charging)
            sendBatteryIfChanged()
        }
    }

    private val networkCallback = object : ConnectivityManager.NetworkCallback() {
        override fun onAvailable(network: Network) {
            wake.trySend(Unit)
        }
    }

    private fun deviceName(): String =
        Settings.Global.getString(contentResolver, Settings.Global.DEVICE_NAME)?.takeIf { it.isNotBlank() } ?: Build.MODEL

    private fun deviceType(): String = if (resources.configuration.smallestScreenWidthDp >= 600) "tablet" else "phone"

    private fun notification(state: LinkState): Notification {
        val text = when (state) {
            is LinkState.Connected -> getString(R.string.notif_connected, state.computer)
            is LinkState.Connecting -> getString(R.string.notif_connecting, state.computer)
            is LinkState.Pairing -> getString(R.string.notif_pairing, state.computer)
            is LinkState.Offline -> getString(R.string.notif_offline, state.computer)
            is LinkState.Unpaired -> getString(R.string.no_devices)
        }
        val open = PendingIntent.getActivity(this, 0, Intent(this, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE)
        return NotificationCompat.Builder(this, CHANNEL)
            .setSmallIcon(R.drawable.ic_stat_continuum)
            .setContentTitle(text)
            .setContentIntent(open)
            .setOngoing(true)
            .setSilent(true)
            .setPriority(NotificationCompat.PRIORITY_MIN)
            .build()
    }

    companion object {
        private const val TAG = "Continuum"
        private const val CHANNEL = "connection"
        private const val NOTIFICATION_ID = 1

        /** Retry at least this often while on a network; joining a network retries at once. */
        private const val MAX_BACKOFF_MS = 60_000L
        const val ACTION_PAIR = "io.github.sharjeelmazhar.continuum.PAIR"
        const val ACTION_UNPAIR = "io.github.sharjeelmazhar.continuum.UNPAIR"
        const val EXTRA_URI = "uri"
        private const val WAKE_PREFIX = "continuum-wake 1 "

        fun start(context: Context) =
            ContextCompat.startForegroundService(context, Intent(context, ContinuumService::class.java))

        fun pair(context: Context, uri: String) = ContextCompat.startForegroundService(
            context,
            Intent(context, ContinuumService::class.java).setAction(ACTION_PAIR).putExtra(EXTRA_URI, uri),
        )

        fun unpair(context: Context) = ContextCompat.startForegroundService(
            context,
            Intent(context, ContinuumService::class.java).setAction(ACTION_UNPAIR),
        )
    }
}
