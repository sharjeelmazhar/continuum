package io.github.sharjeelmazhar.continuum.net

import android.content.Context
import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import java.net.Inet4Address
import kotlin.coroutines.resume
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withTimeoutOrNull

/**
 * Finds a paired computer on the LAN by its device id over mDNS (`_continuum._tcp`), for when
 * its address changed since pairing. The result is only a hint: the TLS pin still decides
 * whether it really is our computer, so a spoofed answer can't redirect us.
 */
object Discovery {
    private const val SERVICE_TYPE = "_continuum._tcp"

    suspend fun find(context: Context, deviceId: String, timeoutMs: Long = 4_000): Pair<String, Int>? {
        val nsd = context.getSystemService(NsdManager::class.java)
        var listener: NsdManager.DiscoveryListener? = null
        try {
            return withTimeoutOrNull(timeoutMs) {
                suspendCancellableCoroutine { cont ->
                    val l = object : NsdManager.DiscoveryListener {
                        override fun onServiceFound(info: NsdServiceInfo) {
                            if (info.serviceName != deviceId) return
                            resolve(nsd, info) { host, port -> if (cont.isActive) cont.resume(host to port) }
                        }
                        override fun onStartDiscoveryFailed(type: String, code: Int) {
                            listener = null
                            if (cont.isActive) cont.resume(null)
                        }
                        override fun onDiscoveryStarted(type: String) {}
                        override fun onDiscoveryStopped(type: String) {}
                        override fun onServiceLost(info: NsdServiceInfo) {}
                        override fun onStopDiscoveryFailed(type: String, code: Int) {}
                    }
                    listener = l
                    nsd.discoverServices(SERVICE_TYPE, NsdManager.PROTOCOL_DNS_SD, l)
                }
            }
        } finally {
            listener?.let { runCatching { nsd.stopServiceDiscovery(it) } }
        }
    }

    @Suppress("DEPRECATION") // resolveService/host still work on all versions; the API 34 replacement isn't on older ones.
    private fun resolve(nsd: NsdManager, info: NsdServiceInfo, done: (String, Int) -> Unit) {
        nsd.resolveService(info, object : NsdManager.ResolveListener {
            override fun onServiceResolved(resolved: NsdServiceInfo) {
                val host = resolved.host ?: return
                if (host is Inet4Address || !host.isLinkLocalAddress) done(host.hostAddress ?: return, resolved.port)
            }
            override fun onResolveFailed(info: NsdServiceInfo, code: Int) {}
        })
    }
}
