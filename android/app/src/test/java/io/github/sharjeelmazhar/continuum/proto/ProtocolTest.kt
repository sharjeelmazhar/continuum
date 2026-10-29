package io.github.sharjeelmazhar.continuum.proto

import java.io.ByteArrayInputStream
import java.io.File
import java.util.Base64
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Test

/** Uses the same test vectors as the Rust crate, so both sides provably agree. */
class ProtocolTest {
    private val vectors: JsonObject =
        Json.parseToJsonElement(File("../../desktop/continuum-proto/tests/vectors.json").readText()).jsonObject

    private fun str(section: String, key: String) = vectors[section]!!.jsonObject[key]!!.jsonPrimitive.content

    @Test
    fun constantsMatchDesktop() {
        assertEquals(1, Protocol.VERSION)
        assertEquals(1_048_576, Protocol.MAX_FRAME_BYTES)
    }

    @Test
    fun pairingProofMatchesVector() {
        val secret = Base64.getUrlDecoder().decode(str("pairing", "secret_b64url"))
        val fpD = Crypto.unhex(str("pairing", "fp_desktop"))!!
        val fpP = Crypto.unhex(str("pairing", "fp_phone"))!!
        assertEquals(str("pairing", "proof"), Crypto.pairingProof(secret, fpD, fpP))
        assertNotEquals(str("pairing", "proof"), Crypto.pairingProof(secret, fpP, fpD))
    }

    @Test
    fun uriMatchesVector() {
        val uri = PairUri.parse(str("uri", "text"))!!
        val v = vectors["uri"]!!.jsonObject
        assertEquals(v["hosts"]!!.jsonArray.map { it.jsonPrimitive.content }, uri.hosts)
        assertEquals(v["port"]!!.jsonPrimitive.int, uri.port)
        assertEquals(v["name"]!!.jsonPrimitive.content, uri.name)
        assertEquals(str("pairing", "fp_desktop"), Crypto.hex(uri.fingerprint))
        assertEquals(16, uri.secret.size)
    }

    @Test
    fun badUrisAreRejected() {
        val good = str("uri", "text")
        listOf(
            "https://example.com",
            good.replace("v=1", "v=2"),
            good.replace("p=47820", "p=0"),
            good.replace("p=47820", "p=99999"),
            good.replace("&s=AAECAwQFBgcICQoLDA0ODw", "&s=AAEC"),
            good.replace("fp=bac8", "fp=zz"),
            good.replace("n=My", "n=%G1"),
        ).forEach { assertNull(it, PairUri.parse(it)) }
    }

    @Test
    fun frameMatchesVector() {
        val env = WireJson.decodeFromString(Envelope.serializer(), str("frame", "json"))
        assertEquals(str("frame", "hex"), Crypto.hex(Frame.encode(env)))
        assertEquals(env, Frame.read(ByteArrayInputStream(Crypto.unhex(str("frame", "hex"))!!)))
    }

    @Test
    fun emptyBodyIsWritten() {
        assertEquals("{\"type\":\"ping\",\"body\":{}}", Frame.encode(Envelope(MessageType.PING)).drop(4).toByteArray().toString(Charsets.UTF_8))
    }

    @Test
    fun oversizedFrameIsRejectedBeforeReading() {
        val header = byteArrayOf(0x00, 0x10, 0x00, 0x01) // 1 MiB + 1
        assertThrows(ProtocolException::class.java) { Frame.read(ByteArrayInputStream(header)) }
    }

    @Test
    fun typedBodiesIgnoreUnknownFields() {
        val env = WireJson.decodeFromString(
            Envelope.serializer(),
            """{"type":"pair.accept","body":{"id":"ab","name":"PC","type":"desktop","future":1}}""",
        )
        assertEquals(PairAccept("ab", "PC", "desktop"), env.body(PairAccept.serializer()))
    }

    @Test
    fun hexRoundTrip() {
        val bytes = byteArrayOf(0, 1, -1, 127)
        assertArrayEquals(bytes, Crypto.unhex(Crypto.hex(bytes)))
        assertNull(Crypto.unhex("abc"))
    }
}
