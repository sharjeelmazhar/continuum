package io.github.sharjeelmazhar.continuum.proto

import java.security.MessageDigest
import java.util.Base64

/** A copied text or image. Mirrors `continuum_proto::clipboard` (docs/PROTOCOL.md, "Clipboard"). */
class ClipContent(val mime: String, val data: ByteArray) {
    /** Identifies the content, so a device never sends back what it just received. */
    val digest: String by lazy {
        val md = MessageDigest.getInstance("SHA-256")
        md.update(mime.toByteArray())
        md.update(0)
        Crypto.hex(md.digest(data))
    }

    val valid: Boolean
        get() = data.isNotEmpty() && data.size <= Clip.MAX_BYTES && when (mime) {
            Clip.TEXT -> runCatching { Charsets.UTF_8.newDecoder().decode(java.nio.ByteBuffer.wrap(data)) }.isSuccess
            else -> mime in Clip.IMAGE_TYPES
        }

    companion object {
        fun text(s: String) = ClipContent(Clip.TEXT, s.toByteArray(Charsets.UTF_8))
    }
}

object Clip {
    const val MAX_BYTES = 12 shl 20
    const val PART_BYTES = 512 shl 10
    const val TEXT = "text/plain"
    val IMAGE_TYPES = listOf("image/png", "image/jpeg", "image/webp", "image/gif")

    /** Splits content into frame-sized parts, or null if it can't be sent. */
    fun split(c: ClipContent): List<ClipboardPart>? {
        if (!c.valid) return null
        val id = c.digest.take(16)
        val parts = (c.data.size + PART_BYTES - 1) / PART_BYTES
        val enc = Base64.getEncoder()
        return (0 until parts).map { i ->
            val from = i * PART_BYTES
            val to = minOf(from + PART_BYTES, c.data.size)
            ClipboardPart(id, c.mime, c.data.size.toLong(), i, parts, enc.encodeToString(c.data.copyOfRange(from, to)))
        }
    }
}

/** Collects parts (in order) back into content; anything inconsistent drops what was collected. */
class ClipAssembler {
    private var first: ClipboardPart? = null
    private var buf: java.io.ByteArrayOutputStream? = null

    fun push(p: ClipboardPart): ClipContent? {
        val bytes = runCatching { Base64.getDecoder().decode(p.data) }.getOrNull()
        val size = p.size
        val expectedParts = ((size + Clip.PART_BYTES - 1) / Clip.PART_BYTES).toInt()
        val want = if (p.part + 1 == p.parts) size - Clip.PART_BYTES.toLong() * (p.parts - 1) else Clip.PART_BYTES.toLong()
        if (bytes == null || size <= 0 || size > Clip.MAX_BYTES || p.parts != expectedParts ||
            p.part !in 0 until p.parts || bytes.size.toLong() != want
        ) {
            reset()
            return null
        }
        if (p.part == 0) {
            first = p
            buf = java.io.ByteArrayOutputStream(size.toInt())
        } else {
            val f = first
            val b = buf
            if (f == null || b == null || f.id != p.id || f.mime != p.mime || f.size != p.size ||
                b.size().toLong() != Clip.PART_BYTES.toLong() * p.part
            ) {
                reset()
                return null
            }
        }
        buf!!.write(bytes)
        if (p.part + 1 < p.parts) return null
        val c = ClipContent(p.mime, buf!!.toByteArray())
        reset()
        return c.takeIf { it.valid }
    }

    private fun reset() {
        first = null
        buf = null
    }
}
