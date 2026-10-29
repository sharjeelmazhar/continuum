package io.github.sharjeelmazhar.continuum.net

import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import io.github.sharjeelmazhar.continuum.proto.Crypto
import java.math.BigInteger
import java.security.KeyPairGenerator
import java.security.KeyStore
import java.security.PrivateKey
import java.security.cert.X509Certificate
import java.security.spec.ECGenParameterSpec
import javax.security.auth.x500.X500Principal

/**
 * This phone's identity: an ECDSA P-256 key inside the Android Keystore (it never leaves the
 * device) and the self-signed certificate the Keystore creates for it (ADR-0015).
 */
class Identity private constructor(val privateKey: PrivateKey, val certificate: X509Certificate) {
    val fingerprint: ByteArray = Crypto.sha256(certificate.encoded)
    val deviceId: String = Crypto.deviceId(fingerprint)

    companion object {
        private const val ALIAS = "continuum-identity"

        @Volatile private var cached: Identity? = null

        @Synchronized
        fun get(): Identity = cached ?: load().also { cached = it }

        private fun load(): Identity {
            val ks = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
            if (!ks.containsAlias(ALIAS)) generate()
            val entry = ks.getEntry(ALIAS, null) as KeyStore.PrivateKeyEntry
            return Identity(entry.privateKey, entry.certificate as X509Certificate)
        }

        private fun generate() {
            val spec = KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_SIGN)
                .setAlgorithmParameterSpec(ECGenParameterSpec("secp256r1"))
                // TLS signs pre-hashed data on some Android versions, so allow DIGEST_NONE too.
                .setDigests(KeyProperties.DIGEST_NONE, KeyProperties.DIGEST_SHA256)
                .setCertificateSubject(X500Principal("CN=Continuum"))
                .setCertificateSerialNumber(BigInteger.ONE)
                .build()
            KeyPairGenerator.getInstance(KeyProperties.KEY_ALGORITHM_EC, "AndroidKeyStore").run {
                initialize(spec)
                generateKeyPair()
            }
        }
    }
}
