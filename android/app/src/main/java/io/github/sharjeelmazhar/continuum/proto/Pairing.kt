package io.github.sharjeelmazhar.continuum.proto

import java.io.ByteArrayOutputStream
import java.security.MessageDigest
import java.util.Base64
import javax.crypto.Mac
import javax.crypto.spec.SecretKeySpec

object Crypto {
    fun sha256(data: ByteArray): ByteArray = MessageDigest.getInstance("SHA-256").digest(data)

    fun hex(bytes: ByteArray): String = bytes.joinToString("") { "%02x".format(it) }

    fun unhex(s: String): ByteArray? {
        if (s.length % 2 != 0) return null
        return ByteArray(s.length / 2) { i ->
            s.substring(2 * i, 2 * i + 2).toIntOrNull(16)?.toByte() ?: return null
        }
    }

    /** Device id: first 16 bytes of the certificate fingerprint, as lowercase hex. */
    fun deviceId(fingerprint: ByteArray): String = hex(fingerprint.copyOf(16))

    /** HMAC-SHA256(secret, fpDesktop ‖ fpPhone), as hex. */
    fun pairingProof(secret: ByteArray, fpDesktop: ByteArray, fpPhone: ByteArray): String {
        val mac = Mac.getInstance("HmacSHA256")
        mac.init(SecretKeySpec(secret, "HmacSHA256"))
        mac.update(fpDesktop)
        mac.update(fpPhone)
        return hex(mac.doFinal())
    }
}

/** Contents of the pairing QR code (docs/PROTOCOL.md). */
data class PairUri(
    val hosts: List<String>,
    val port: Int,
    val fingerprint: ByteArray,
    val secret: ByteArray,
    val name: String,
) {
    override fun equals(other: Any?) = other is PairUri && hosts == other.hosts && port == other.port &&
        fingerprint.contentEquals(other.fingerprint) && secret.contentEquals(other.secret) && name == other.name

    override fun hashCode() = hosts.hashCode() * 31 + port

    companion object {
        private const val PREFIX = "continuum://pair?"

        /** Returns null for anything that isn't a valid v1 pairing link. */
        fun parse(uri: String): PairUri? {
            if (!uri.startsWith(PREFIX)) return null
            val params = uri.removePrefix(PREFIX).split('&').associate {
                val i = it.indexOf('=')
                if (i < 0) it to "" else it.substring(0, i) to it.substring(i + 1)
            }
            if (params["v"] != "1") return null
            val hosts = params["h"]?.split(',')?.filter { it.isNotEmpty() } ?: return null
            if (hosts.isEmpty() || hosts.size > 8) return null
            val port = params["p"]?.toIntOrNull()?.takeIf { it in 1..65535 } ?: return null
            val fp = params["fp"]?.let(Crypto::unhex)?.takeIf { it.size == 32 } ?: return null
            val secret = params["s"]?.let {
                try {
                    Base64.getUrlDecoder().decode(it)
                } catch (_: IllegalArgumentException) {
                    null
                }
            }?.takeIf { it.size == 16 } ?: return null
            val name = percentDecode(params["n"] ?: "") ?: return null
            return PairUri(hosts, port, fp, secret, name)
        }

        private fun percentDecode(s: String): String? {
            val out = ByteArrayOutputStream()
            var i = 0
            while (i < s.length) {
                when (val c = s[i]) {
                    '%' -> {
                        if (i + 3 > s.length) return null
                        out.write(s.substring(i + 1, i + 3).toIntOrNull(16) ?: return null)
                        i += 3
                    }
                    '+' -> { out.write(' '.code); i++ }
                    else -> { out.write(c.toString().toByteArray(Charsets.UTF_8)); i++ }
                }
            }
            return out.toByteArray().toString(Charsets.UTF_8)
        }
    }
}
