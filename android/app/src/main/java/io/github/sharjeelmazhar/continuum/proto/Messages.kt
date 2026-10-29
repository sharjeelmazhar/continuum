package io.github.sharjeelmazhar.continuum.proto

import kotlinx.serialization.KSerializer
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject

/** One control message: `{"type": "...", "body": {...}}`. */
@Serializable
data class Envelope(val type: String, val body: JsonObject = JsonObject(emptyMap())) {
    fun <T> body(serializer: KSerializer<T>): T = WireJson.decodeFromJsonElement(serializer, body)

    companion object {
        fun <T> of(type: String, serializer: KSerializer<T>, value: T) =
            Envelope(type, WireJson.encodeToJsonElement(serializer, value).jsonObject)
    }
}

object MessageType {
    const val HELLO = "hello"
    const val PING = "ping"
    const val PONG = "pong"
    const val UNPAIR = "unpair"
    const val ERROR = "error"
    const val PAIR_REQUEST = "pair.request"
    const val PAIR_ACCEPT = "pair.accept"
    const val PAIR_REJECT = "pair.reject"
    const val BATTERY_STATUS = "battery.status"
    const val SHARE_OFFER = "share.offer"
    const val SHARE_ACCEPT = "share.accept"
    const val SHARE_REJECT = "share.reject"
    const val SHARE_RESULT = "share.result"
    const val DATA_HELLO = "data.hello"
}

/** Peers may be newer than us: ignore unknown fields, always write `body`. */
val WireJson = Json {
    ignoreUnknownKeys = true
    encodeDefaults = true
}

@Serializable
data class Hello(
    val v: Int,
    val id: String,
    val name: String,
    @SerialName("type") val deviceType: String,
    val caps: List<String> = emptyList(),
)

@Serializable
data class PairRequest(val name: String, @SerialName("type") val deviceType: String, val proof: String)

@Serializable
data class PairAccept(val id: String, val name: String, @SerialName("type") val deviceType: String)

@Serializable
data class PairReject(val reason: String)

@Serializable
data class BatteryStatus(val level: Int, val charging: Boolean)

@Serializable
data class FileMeta(val name: String, val size: Long, val mime: String = "")

@Serializable
data class ShareOffer(@SerialName("transfer_id") val transferId: String, val files: List<FileMeta>)

@Serializable
data class ShareAccept(@SerialName("transfer_id") val transferId: String, val token: String, val offsets: List<Long>)

@Serializable
data class ShareReject(@SerialName("transfer_id") val transferId: String, val reason: String)

@Serializable
data class ShareResult(
    @SerialName("transfer_id") val transferId: String,
    val ok: Boolean,
    val saved: List<String> = emptyList(),
    val error: String? = null,
)

@Serializable
data class DataHello(val token: String)
