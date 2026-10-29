package io.github.sharjeelmazhar.continuum.net

import android.annotation.SuppressLint
import android.os.Build
import io.github.sharjeelmazhar.continuum.proto.Crypto
import java.net.InetSocketAddress
import java.net.Socket
import java.security.MessageDigest
import java.security.Principal
import java.security.PrivateKey
import java.security.cert.CertificateException
import java.security.cert.X509Certificate
import javax.net.ssl.SSLContext
import javax.net.ssl.SSLEngine
import javax.net.ssl.SSLSocket
import javax.net.ssl.X509ExtendedKeyManager
import javax.net.ssl.X509ExtendedTrustManager

/** Thrown when the computer's certificate isn't the one we pinned. */
class WrongComputerException : CertificateException("certificate does not match the paired computer")

class UnsupportedAndroidException : Exception("Android 10 or newer is needed for secure connections")

/**
 * Opens a TLS 1.3 connection that trusts only the computer whose certificate fingerprint is
 * [pinned] (ADR-0005), and proves our identity with the Keystore key.
 */
object Tls {
    private const val CONNECT_TIMEOUT_MS = 4_000

    fun connect(host: String, port: Int, identity: Identity, pinned: ByteArray): SSLSocket {
        // TODO(M2): bundle Conscrypt so Android 8-9 get TLS 1.3 too.
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) throw UnsupportedAndroidException()
        val context = SSLContext.getInstance("TLSv1.3")
        context.init(arrayOf(KeyManager(identity)), arrayOf(PinnedTrustManager(pinned)), null)
        val plain = Socket()
        try {
            plain.tcpNoDelay = true
            plain.connect(InetSocketAddress(host, port), CONNECT_TIMEOUT_MS)
            val socket = context.socketFactory.createSocket(plain, host, port, true) as SSLSocket
            socket.enabledProtocols = arrayOf("TLSv1.3")
            socket.soTimeout = CONNECT_TIMEOUT_MS * 2
            socket.startHandshake()
            return socket
        } catch (e: Exception) {
            plain.close()
            throw e
        }
    }

    private class KeyManager(private val id: Identity) : X509ExtendedKeyManager() {
        private val alias = "continuum"
        override fun chooseClientAlias(keyType: Array<out String>?, issuers: Array<out Principal>?, socket: Socket?) = alias
        override fun chooseEngineClientAlias(keyType: Array<out String>?, issuers: Array<out Principal>?, engine: SSLEngine?) = alias
        override fun getCertificateChain(alias: String?): Array<X509Certificate> = arrayOf(id.certificate)
        override fun getPrivateKey(alias: String?): PrivateKey = id.privateKey
        override fun getClientAliases(keyType: String?, issuers: Array<out Principal>?) = arrayOf(alias)
        override fun getServerAliases(keyType: String?, issuers: Array<out Principal>?): Array<String>? = null
        override fun chooseServerAlias(keyType: String?, issuers: Array<out Principal>?, socket: Socket?): String? = null
    }

    // Not a weakened default: it accepts exactly one certificate, the pinned one (ADR-0005).
    @SuppressLint("CustomX509TrustManager")
    private class PinnedTrustManager(private val pinned: ByteArray) : X509ExtendedTrustManager() {
        private fun check(chain: Array<out X509Certificate>?) {
            val cert = chain?.firstOrNull() ?: throw CertificateException("no certificate")
            if (!MessageDigest.isEqual(Crypto.sha256(cert.encoded), pinned)) throw WrongComputerException()
        }

        override fun checkServerTrusted(chain: Array<out X509Certificate>?, authType: String?) = check(chain)
        override fun checkServerTrusted(chain: Array<out X509Certificate>?, authType: String?, socket: Socket?) = check(chain)
        override fun checkServerTrusted(chain: Array<out X509Certificate>?, authType: String?, engine: SSLEngine?) = check(chain)
        override fun checkClientTrusted(chain: Array<out X509Certificate>?, authType: String?) = throw CertificateException("client mode only")
        override fun checkClientTrusted(chain: Array<out X509Certificate>?, authType: String?, socket: Socket?) = throw CertificateException("client mode only")
        override fun checkClientTrusted(chain: Array<out X509Certificate>?, authType: String?, engine: SSLEngine?) = throw CertificateException("client mode only")
        override fun getAcceptedIssuers(): Array<X509Certificate> = emptyArray()
    }
}
