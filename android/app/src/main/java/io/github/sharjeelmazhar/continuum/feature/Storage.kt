package io.github.sharjeelmazhar.continuum.feature

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import android.os.Environment
import android.os.ParcelFileDescriptor
import android.system.Os
import android.webkit.MimeTypeMap
import androidx.core.content.ContextCompat
import io.github.sharjeelmazhar.continuum.proto.FileEntry
import io.github.sharjeelmazhar.continuum.proto.Limits
import io.github.sharjeelmazhar.continuum.proto.WireJson
import java.io.File

/** The phone's shared storage (what the Files app shows) for the computer (PROTOCOL.md, "Files"). */
class Storage(private val context: Context) {
    private val root: File get() = Environment.getExternalStorageDirectory()

    /** "All files access" on Android 11+, the storage permission before. */
    fun canRead(): Boolean = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
        Environment.isExternalStorageManager()
    } else {
        ContextCompat.checkSelfPermission(context, Manifest.permission.READ_EXTERNAL_STORAGE) == PackageManager.PERMISSION_GRANTED
    }

    /** A folder's entries, sorted and cut to the reply budget; the flag says whether it was cut. */
    fun list(path: String): Pair<List<FileEntry>, Boolean> {
        val dir = StoragePaths.resolve(root, path) ?: throw FetchError("bad request")
        if (!dir.exists()) throw FetchError("not found")
        if (!dir.isDirectory) throw FetchError("not a folder")
        val names = dir.list() ?: throw FetchError("can't read this folder")
        val entries = names.asSequence().filter { StoragePaths.visible(it) }.map { name ->
            val f = File(dir, name)
            val isDir = f.isDirectory
            FileEntry(name, isDir, if (isDir) 0 else f.length(), f.lastModified(), if (isDir) "" else mimeOf(name))
        }.toList()
        return StoragePaths.fit(entries)
    }

    /** Opens a regular file (≤ 4 GiB) under the root for `files.fetch`. */
    fun open(path: String): Outgoing {
        val canonicalRoot = root.canonicalFile
        val f = StoragePaths.resolve(root, path) ?: throw FetchError("bad request")
        if (f == canonicalRoot) throw FetchError("not a file")
        if (!f.exists()) throw FetchError("not found")
        if (!f.isFile) throw FetchError("not a file")
        val pfd = runCatching { ParcelFileDescriptor.open(f, ParcelFileDescriptor.MODE_READ_ONLY) }.getOrElse { throw FetchError("can't read") }
        try {
            // A folder on the way could have been swapped for a link since the check: make sure
            // the file we actually opened is under the root.
            val opened = runCatching { Os.readlink("/proc/self/fd/${pfd.fd}") }.getOrNull()
            if (opened != null && !StoragePaths.under(canonicalRoot, File(opened))) throw FetchError("not found")
            val size = pfd.statSize
            if (size > Limits.FETCH_MAX_BYTES) throw FetchError("too large")
            if (size <= 0) throw FetchError("empty file")
            return Outgoing(f.name, mimeOf(f.name), size, ParcelFileDescriptor.AutoCloseInputStream(pfd))
        } catch (e: Exception) {
            pfd.close()
            throw e
        }
    }

    private fun mimeOf(name: String): String {
        val ext = name.substringAfterLast('.', "").lowercase()
        return MimeTypeMap.getSingleton().getMimeTypeFromExtension(ext) ?: "application/octet-stream"
    }
}

/** Path checks and list shaping for [Storage], kept free of Android so they're unit-tested. */
object StoragePaths {
    /**
     * The segments of a request path, or null if it's hostile: absolute, too long, NUL, empty,
     * `.` or `..` segments. `""` is the root (no segments).
     */
    fun segments(path: String): List<String>? {
        if (path.isEmpty()) return emptyList()
        if (path.length > Limits.PATH_MAX || '\u0000' in path) return null
        val parts = path.split('/')
        return parts.takeIf { p -> p.none { it.isEmpty() || it == "." || it == ".." } }
    }

    /** The canonical file [path] names under [root], or null if it's invalid or leads outside (symlinks included). */
    fun resolve(root: File, path: String): File? {
        val segs = segments(path) ?: return null
        val base = root.canonicalFile
        if (segs.isEmpty()) return base
        val target = runCatching { File(base, segs.joinToString("/")).canonicalFile }.getOrNull() ?: return null
        return target.takeIf { under(base, it) }
    }

    /** True if canonical [f] is [base] or inside it. */
    fun under(base: File, f: File) = f == base || f.path.startsWith(base.path.trimEnd('/') + "/")

    /** Hidden (`.`-prefixed) entries aren't listed. */
    fun visible(name: String) = name.isNotEmpty() && !name.startsWith('.')

    /** Folders first, then by name ignoring case; then as many as fit in 5000 entries and the JSON budget. */
    fun fit(entries: List<FileEntry>, max: Int = Limits.FILES_MAX, budget: Int = Limits.FILES_BUDGET): Pair<List<FileEntry>, Boolean> {
        val sorted = entries.sortedWith(compareBy<FileEntry> { !it.dir }.thenBy(String.CASE_INSENSITIVE_ORDER) { it.name }.thenBy { it.name })
        val out = ArrayList<FileEntry>(minOf(sorted.size, max))
        var used = 0
        for (e in sorted) {
            if (out.size >= max) break
            val bytes = WireJson.encodeToString(FileEntry.serializer(), e).toByteArray(Charsets.UTF_8).size + 1
            if (used + bytes > budget) break
            used += bytes
            out += e
        }
        return out to (out.size < sorted.size)
    }
}
