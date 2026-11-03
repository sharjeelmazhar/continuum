package io.github.sharjeelmazhar.continuum.feature

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.database.ContentObserver
import android.database.Cursor
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.net.Uri
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.provider.ContactsContract
import android.provider.Telephony
import android.telephony.SmsManager
import androidx.core.content.ContextCompat
import io.github.sharjeelmazhar.continuum.proto.Limits
import io.github.sharjeelmazhar.continuum.proto.SmsMessage
import io.github.sharjeelmazhar.continuum.proto.SmsThread
import androidx.core.net.toUri
import java.io.ByteArrayOutputStream
import java.util.Base64

/** Reads and sends text messages for the computer (PROTOCOL.md, "Messages"). */
class Sms(private val context: Context) {
    private companion object {
        /** Contact photos are sent at this size, within this much of the threads reply. */
        const val PHOTO_PX = 96
        const val PHOTOS_BUDGET = 500_000

        /** Contact photo URI → base64 JPEG, so refreshing the threads doesn't decode them again. */
        val photoCache = object : LinkedHashMap<String, String>(64, 0.75f, true) {
            override fun removeEldestEntry(eldest: MutableMap.MutableEntry<String, String>?) = size > 50
        }
    }

    private val cr = context.contentResolver

    fun canRead() = granted(Manifest.permission.READ_SMS)
    fun canSend() = granted(Manifest.permission.SEND_SMS)
    private fun granted(p: String) = ContextCompat.checkSelfPermission(context, p) == PackageManager.PERMISSION_GRANTED

    fun threads(limit: Int): List<SmsThread> {
        val rows = ArrayList<SmsRow>()
        rows += querySms(null, null, SmsData.SCAN_MAX)
        val mms = queryMms(null, null, SmsData.SCAN_MAX)
        rows += mms
        val contacts = HashMap<String, Contact>()
        val contact = { addr: String -> contacts.getOrPut(addr) { contact(addr) } }
        val threads = SmsData.threads(rows, limit) { addr -> contact(addr).name }
        // Threads with only MMS have no address yet: read it from the latest MMS.
        val named = threads.map { t ->
            if (t.address.isNotEmpty()) return@map t
            val latest = mms.firstOrNull { it.thread == t.thread } ?: return@map t
            val addr = mmsAddress(-latest.id, SmsData.isOut(latest.type))
            if (addr.isEmpty()) t else t.copy(address = addr, name = contact(addr).name)
        }
        var budget = PHOTOS_BUDGET
        return named.map { t ->
            val uri = contacts[t.address]?.photo ?: return@map t
            val data = photo(uri) ?: return@map t
            if (data.length > budget) return@map t
            budget -= data.length
            t.copy(photo = data)
        }
    }

    private data class Contact(val name: String, val photo: Uri?)

    /** A contact's photo, shrunk to a small JPEG (base64); recent ones are kept (see [photoCache]). */
    private fun photo(uri: Uri): String? {
        val key = uri.toString()
        synchronized(photoCache) { photoCache[key]?.let { return it } }
        return decodePhoto(uri)?.also { synchronized(photoCache) { photoCache[key] = it } }
    }

    private fun decodePhoto(uri: Uri): String? = runCatching {
        val bmp = cr.openInputStream(uri)?.use { BitmapFactory.decodeStream(it) } ?: return null
        val small = Bitmap.createScaledBitmap(bmp, PHOTO_PX, PHOTO_PX, true)
        val jpeg = ByteArrayOutputStream().also { small.compress(Bitmap.CompressFormat.JPEG, 85, it) }.toByteArray()
        if (small !== bmp) small.recycle()
        bmp.recycle()
        Base64.getEncoder().encodeToString(jpeg)
    }.getOrNull()

    fun messages(thread: Long, before: Long, limit: Int): List<SmsMessage> {
        val n = Limits.limit(limit)
        val sel = StringBuilder("${Telephony.Sms.THREAD_ID} = ?")
        val args = arrayListOf(thread.toString())
        if (before > 0) {
            sel.append(" AND ${Telephony.Sms.DATE} < ?")
            args += before.toString()
        }
        val rows = ArrayList<SmsRow>()
        rows += querySms(sel.toString(), args.toTypedArray(), n)
        // MMS dates are in seconds.
        val mmsSel = StringBuilder("${Telephony.Mms.THREAD_ID} = ?")
        val mmsArgs = arrayListOf(thread.toString())
        if (before > 0) {
            mmsSel.append(" AND ${Telephony.Mms.DATE} < ?")
            mmsArgs += ((before + 999) / 1000).toString()
        }
        rows += queryMms(mmsSel.toString(), mmsArgs.toTypedArray(), n)
        return SmsData.messages(rows, before, n)
    }

    /** Hands the text to the modem (split into parts as needed). Throws if Android refuses. */
    fun send(address: String, body: String) {
        val sms = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            context.getSystemService(SmsManager::class.java)
        } else {
            @Suppress("DEPRECATION")
            SmsManager.getDefault()
        } ?: throw IllegalStateException("this phone can't send texts")
        val parts = sms.divideMessage(body)
        // Android saves it to "Sent" for us (we aren't the default SMS app).
        sms.sendMultipartTextMessage(address, null, parts, null, null)
    }

    private fun querySms(selection: String?, args: Array<String>?, max: Int): List<SmsRow> {
        val projection = arrayOf(
            Telephony.Sms._ID, Telephony.Sms.THREAD_ID, Telephony.Sms.ADDRESS, Telephony.Sms.BODY,
            Telephony.Sms.DATE, Telephony.Sms.TYPE, Telephony.Sms.READ, Telephony.Sms.STATUS,
        )
        return read(Telephony.Sms.CONTENT_URI, projection, selection, args, "${Telephony.Sms.DATE} DESC", max) { c ->
            SmsRow(
                id = c.getLong(0),
                thread = c.getLong(1),
                address = c.getString(2).orEmpty(),
                body = c.getString(3).orEmpty(),
                date = c.getLong(4),
                type = c.getInt(5),
                read = c.getInt(6) != 0,
                status = if (c.isNull(7)) -1 else c.getInt(7),
            )
        }
    }

    /** MMS as "[MMS]" rows; ids are negated so they never clash with SMS ids. */
    private fun queryMms(selection: String?, args: Array<String>?, max: Int): List<SmsRow> {
        val projection = arrayOf(Telephony.Mms._ID, Telephony.Mms.THREAD_ID, Telephony.Mms.DATE, Telephony.Mms.MESSAGE_BOX, Telephony.Mms.READ)
        return runCatching {
            read(Telephony.Mms.CONTENT_URI, projection, selection, args, "${Telephony.Mms.DATE} DESC", max) { c ->
                SmsRow(-c.getLong(0), c.getLong(1), "", SmsData.MMS_TEXT, c.getLong(2) * 1000, SmsData.mmsType(c.getInt(3)), c.getInt(4) != 0)
            }
        }.getOrDefault(emptyList())
    }

    /** The other party of an MMS: its sender (137) if received, else its recipient (151). */
    private fun mmsAddress(id: Long, out: Boolean): String = runCatching {
        val type = if (out) "151" else "137"
        cr.query("content://mms/$id/addr".toUri(), arrayOf("address"), "type = ?", arrayOf(type), null)?.use { c ->
            if (c.moveToFirst()) c.getString(0).orEmpty().takeIf { it != "insert-address-token" } else null
        }
    }.getOrNull().orEmpty()

    private fun contact(address: String): Contact {
        val none = Contact("", null)
        if (address.isEmpty() || !granted(Manifest.permission.READ_CONTACTS)) return none
        return runCatching {
            val uri = Uri.withAppendedPath(ContactsContract.PhoneLookup.CONTENT_FILTER_URI, Uri.encode(address))
            val cols = arrayOf(ContactsContract.PhoneLookup.DISPLAY_NAME, ContactsContract.PhoneLookup.PHOTO_THUMBNAIL_URI)
            cr.query(uri, cols, null, null, null)?.use { c ->
                if (c.moveToFirst()) Contact(c.getString(0).orEmpty(), c.getString(1)?.toUri()) else null
            }
        }.getOrNull() ?: none
    }

    private fun <T> read(uri: Uri, projection: Array<String>, selection: String?, args: Array<String>?, order: String, max: Int, row: (Cursor) -> T): List<T> {
        val out = ArrayList<T>()
        cr.query(uri, projection, selection, args, order)?.use { c ->
            while (out.size < max && c.moveToNext()) out += row(c)
        }
        return out
    }

    /** Calls [onChange] (at most every 500 ms) while the SMS database changes. */
    class Watcher(private val context: Context, private val onChange: () -> Unit) {
        private val handler = Handler(Looper.getMainLooper())
        private val fire = Runnable { onChange() }
        private val observer = object : ContentObserver(handler) {
            override fun onChange(selfChange: Boolean) {
                handler.removeCallbacks(fire)
                handler.postDelayed(fire, 500)
            }
        }
        private var registered = false

        @Synchronized
        fun start() {
            if (registered || !Sms(context).canRead()) return
            registered = runCatching { context.contentResolver.registerContentObserver(Telephony.Sms.CONTENT_URI, true, observer) }.isSuccess
            if (registered) runCatching { context.contentResolver.registerContentObserver(Telephony.Mms.CONTENT_URI, true, observer) }
        }

        @Synchronized
        fun stop() {
            handler.removeCallbacks(fire)
            if (registered) context.contentResolver.unregisterContentObserver(observer)
            registered = false
        }
    }
}
