package io.github.sharjeelmazhar.continuum.feature

import android.content.Context
import android.net.Uri
import android.util.Log
import io.github.sharjeelmazhar.continuum.net.Session
import io.github.sharjeelmazhar.continuum.proto.Ack
import io.github.sharjeelmazhar.continuum.proto.Envelope
import io.github.sharjeelmazhar.continuum.proto.ErrorReply
import io.github.sharjeelmazhar.continuum.proto.FeatureType
import io.github.sharjeelmazhar.continuum.proto.Limits
import io.github.sharjeelmazhar.continuum.proto.NotifAction
import io.github.sharjeelmazhar.continuum.proto.NotifKey
import io.github.sharjeelmazhar.continuum.proto.NotifPosted
import io.github.sharjeelmazhar.continuum.proto.NotifReply
import io.github.sharjeelmazhar.continuum.proto.PhotosFetch
import io.github.sharjeelmazhar.continuum.proto.PhotosFetchReply
import io.github.sharjeelmazhar.continuum.proto.PhotosList
import io.github.sharjeelmazhar.continuum.proto.PhotosListReply
import io.github.sharjeelmazhar.continuum.proto.PhotosPart
import io.github.sharjeelmazhar.continuum.proto.PhotosSend
import io.github.sharjeelmazhar.continuum.proto.PhotosThumbs
import io.github.sharjeelmazhar.continuum.proto.PhotosThumbsReply
import io.github.sharjeelmazhar.continuum.proto.Protocol
import io.github.sharjeelmazhar.continuum.proto.RequestId
import io.github.sharjeelmazhar.continuum.proto.SmsMessages
import io.github.sharjeelmazhar.continuum.proto.SmsMessagesReply
import io.github.sharjeelmazhar.continuum.proto.SmsSend
import io.github.sharjeelmazhar.continuum.proto.SmsThreads
import io.github.sharjeelmazhar.continuum.proto.SmsThreadsReply
import java.util.Base64
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.onSubscription
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.serialization.KSerializer

/**
 * The phone as a data source for one connection: answers the computer's requests (photos,
 * messages) off the read loop, and mirrors notifications and SMS changes while connected.
 * Everything from the computer is treated as hostile: bodies are parsed strictly and clamped.
 */
class Features(
    private val context: Context,
    parent: CoroutineScope,
    private val session: Session,
    private val peerCaps: List<String>,
    /** Queues MediaStore items as a normal file transfer; false if none could be read. */
    private val sendFiles: suspend (List<Uri>) -> Boolean,
) {
    private val scope = CoroutineScope(parent.coroutineContext + SupervisorJob(parent.coroutineContext[Job]))
    private val photos = Photos(context)
    private val sms = Sms(context)
    private val smsWatcher = Sms.Watcher(context) { scope.launch { send(FeatureType.SMS_CHANGED, Envelope(FeatureType.SMS_CHANGED)) } }
    private val inFlight = AtomicInteger()

    /** Full-size photo downloads hold the whole image in memory: one at a time. */
    private val fetchLock = Mutex()

    fun start() {
        if (Protocol.CAP_SMS in peerCaps) smsWatcher.start()
        if (Protocol.CAP_NOTIF in peerCaps) {
            scope.launch {
                NotifBridge.events
                    .onSubscription { NotifBridge.listener?.snapshot()?.forEach { emit(NotifEvent.Posted(it)) } }
                    .collect { e ->
                        when (e) {
                            is NotifEvent.Posted -> send(FeatureType.NOTIF_POSTED, Envelope.of(FeatureType.NOTIF_POSTED, NotifPosted.serializer(), e.n))
                            is NotifEvent.Removed -> send(FeatureType.NOTIF_REMOVED, Envelope.of(FeatureType.NOTIF_REMOVED, NotifKey.serializer(), NotifKey(e.key)))
                        }
                    }
            }
        }
    }

    fun stop() {
        smsWatcher.stop()
        scope.cancel()
    }

    /** Handles a feature message; false if [msg] isn't one. Never blocks the read loop. */
    fun handle(msg: Envelope): Boolean {
        when (msg.type) {
            in FeatureType.REQUESTS -> request(msg)
            FeatureType.NOTIF_DISMISS, FeatureType.NOTIF_REPLY, FeatureType.NOTIF_ACTION -> scope.launch { notifCommand(msg) }
            else -> return false
        }
        return true
    }

    private fun request(msg: Envelope) {
        val req = runCatching { msg.body(RequestId.serializer()).req }.getOrNull()?.takeIf { Limits.validReq(it) } ?: return
        val replyType = FeatureType.reply(msg.type)
        if (inFlight.incrementAndGet() > MAX_IN_FLIGHT) {
            inFlight.decrementAndGet()
            replyError(replyType, req, "busy")
            return
        }
        scope.launch {
            try {
                answer(msg, req, replyType)
            } catch (e: BadRequest) {
                replyError(replyType, req, "bad request")
            } catch (e: Exception) {
                if (e is kotlinx.coroutines.CancellationException) throw e
                Log.i(TAG, "${msg.type} failed: $e")
                replyError(replyType, req, Limits.cutUtf8(e.message ?: e.javaClass.simpleName, 200))
            } finally {
                inFlight.decrementAndGet()
            }
        }
    }

    private class BadRequest : Exception()

    private fun <T> Envelope.parse(serializer: KSerializer<T>): T = runCatching { body(serializer) }.getOrElse { throw BadRequest() }

    private suspend fun answer(msg: Envelope, req: Long, replyType: String) {
        when (msg.type) {
            FeatureType.PHOTOS_LIST -> {
                val r = msg.parse(PhotosList.serializer())
                if (!photos.canRead()) return replyError(replyType, req, ErrorReply.PERMISSION)
                val (items, total) = photos.list(r.offset.coerceAtLeast(0), Limits.limit(r.limit))
                reply(replyType, PhotosListReply.serializer(), PhotosListReply(req, items, total))
            }
            FeatureType.PHOTOS_THUMBS -> {
                val r = msg.parse(PhotosThumbs.serializer())
                if (!photos.canRead()) return replyError(replyType, req, ErrorReply.PERMISSION)
                val items = photos.thumbs(r.ids.distinct().take(Limits.THUMB_IDS_MAX), Limits.thumbSize(r.size))
                reply(replyType, PhotosThumbsReply.serializer(), PhotosThumbsReply(req, items))
            }
            FeatureType.PHOTOS_FETCH -> {
                val r = msg.parse(PhotosFetch.serializer())
                if (!photos.canRead()) return replyError(replyType, req, ErrorReply.PERMISSION)
                fetchLock.withLock { fetch(req, replyType, r.id) }
            }
            FeatureType.PHOTOS_SEND -> {
                val r = msg.parse(PhotosSend.serializer())
                if (!photos.canRead()) return replyError(replyType, req, ErrorReply.PERMISSION)
                val found = photos.lookup(r.ids.take(Limits.SEND_IDS_MAX))
                if (found.isEmpty() || !sendFiles(found.map { it.uri })) return replyError(replyType, req, "not found")
                reply(replyType, Ack.serializer(), Ack(req))
            }
            FeatureType.SMS_THREADS -> {
                val r = msg.parse(SmsThreads.serializer())
                if (!sms.canRead()) return replyError(replyType, req, ErrorReply.PERMISSION)
                if (Protocol.CAP_SMS in peerCaps) smsWatcher.start()
                reply(replyType, SmsThreadsReply.serializer(), SmsThreadsReply(req, sms.threads(Limits.limit(r.limit))))
            }
            FeatureType.SMS_MESSAGES -> {
                val r = msg.parse(SmsMessages.serializer())
                if (!sms.canRead()) return replyError(replyType, req, ErrorReply.PERMISSION)
                val messages = sms.messages(r.thread, r.before.coerceAtLeast(0), Limits.limit(r.limit))
                reply(replyType, SmsMessagesReply.serializer(), SmsMessagesReply(req, messages))
            }
            FeatureType.SMS_SEND -> {
                val r = msg.parse(SmsSend.serializer())
                val address = Limits.smsAddress(r.address) ?: throw BadRequest()
                val body = Limits.smsBody(r.body) ?: throw BadRequest()
                if (!sms.canSend()) return replyError(replyType, req, ErrorReply.PERMISSION)
                sms.send(address, body)
                reply(replyType, Ack.serializer(), Ack(req))
            }
        }
    }

    /** `photos.fetch`: the reply with name, type and size, then the parts in order. */
    private fun fetch(req: Long, replyType: String, id: Long) {
        val f = photos.fetch(id)
        val parts = Limits.parts(f.data.size.toLong())
        if (parts == 0) return replyError(replyType, req, "empty file")
        reply(replyType, PhotosFetchReply.serializer(), PhotosFetchReply(req, f.name, f.mime, f.data.size.toLong(), parts))
        val enc = Base64.getEncoder()
        // Already replied: if the connection drops now, the computer just times out.
        runCatching {
            for (i in 0 until parts) {
                val from = i * Limits.PART_BYTES
                val to = minOf(from + Limits.PART_BYTES, f.data.size)
                val data = enc.encodeToString(f.data.copyOfRange(from, to))
                session.send(Envelope.of(FeatureType.PHOTOS_PART, PhotosPart.serializer(), PhotosPart(req, i, data)))
            }
        }.onFailure { Log.i(TAG, "photo parts not sent: $it") }
    }

    private fun notifCommand(msg: Envelope) {
        val listener = NotifBridge.listener ?: return
        runCatching {
            when (msg.type) {
                FeatureType.NOTIF_DISMISS -> msg.body(NotifKey.serializer()).takeIf { validKey(it.key) }?.let { listener.dismiss(it.key) }
                FeatureType.NOTIF_REPLY -> msg.body(NotifReply.serializer())
                    .takeIf { validKey(it.key) && it.text.isNotEmpty() && it.text.length <= Limits.NOTIF_REPLY_MAX }
                    ?.let { listener.reply(it.key, it.text) }
                FeatureType.NOTIF_ACTION -> msg.body(NotifAction.serializer())
                    .takeIf { validKey(it.key) && it.index in 0..2 }
                    ?.let { listener.action(it.key, it.index) }
                else -> {}
            }
        }.onFailure { Log.i(TAG, "${msg.type} ignored: $it") }
    }

    private fun validKey(key: String) = key.isNotEmpty() && key.length <= Limits.NOTIF_KEY_MAX

    private fun <T> reply(type: String, serializer: KSerializer<T>, value: T) = send(type, Envelope.of(type, serializer, value))

    private fun replyError(type: String, req: Long, error: String) =
        send(type, Envelope.of(type, ErrorReply.serializer(), ErrorReply(req, error)))

    private fun send(what: String, envelope: Envelope) {
        runCatching { session.send(envelope) }.onFailure { Log.i(TAG, "$what not sent: $it") }
    }

    companion object {
        private const val TAG = "Continuum"

        /** Requests answered at the same time; more get `error: "busy"`. */
        private const val MAX_IN_FLIGHT = 8
    }
}
