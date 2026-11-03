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

        /**
         * A link from outside the app (web page, other app) keeps only addresses on a home or office
         * network, so a page can't pair the phone with a computer somewhere on the internet.
         */
        fun localOnly(uri: String): String? {
            val hosts = parse(uri)?.hosts?.filter(::isLocalAddress)?.takeIf { it.isNotEmpty() } ?: return null
            val params = uri.removePrefix(PREFIX).split('&')
                .map { if (it.startsWith("h=")) "h=" + hosts.joinToString(",") else it }
            return PREFIX + params.joinToString("&")
        }

        /** Private, link-local or carrier-grade NAT IPv4; link-local or unique-local IPv6; mDNS names. */
        fun isLocalAddress(host: String): Boolean {
            val h = host.lowercase().removePrefix("[").removeSuffix("]")
            if (h.endsWith(".local")) return true
            val v4 = h.split('.').takeIf { it.size == 4 }?.map { it.toIntOrNull()?.takeIf { n -> n in 0..255 } ?: return false }
            if (v4 != null) {
                val (a, b) = v4
                return a == 10 || a == 172 && b in 16..31 || a == 192 && b == 168 || a == 169 && b == 254 ||
                    a == 100 && b in 64..127
            }
            if (':' !in h) return false
            return h.startsWith("fe8") || h.startsWith("fe9") || h.startsWith("fea") || h.startsWith("feb") ||
                h.startsWith("fc") || h.startsWith("fd")
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
