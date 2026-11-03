package io.github.sharjeelmazhar.continuum.feature

import io.github.sharjeelmazhar.continuum.proto.FileEntry
import io.github.sharjeelmazhar.continuum.proto.FilesListReply
import io.github.sharjeelmazhar.continuum.proto.Limits
import io.github.sharjeelmazhar.continuum.proto.WireJson
import java.io.File
import java.nio.file.Files
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** PROTOCOL.md "Files": hostile paths are refused, lists are sorted and fit in one frame. */
class StorageTest {
    private val tmp: File = Files.createTempDirectory("continuum-storage").toFile()
    private val root = File(tmp, "root").apply { mkdirs() }

    @After
    fun cleanUp() {
        tmp.deleteRecursively()
    }

    @Test
    fun segmentsRejectHostilePaths() {
        assertEquals(emptyList<String>(), StoragePaths.segments(""))
        assertEquals(listOf("DCIM", "Camera"), StoragePaths.segments("DCIM/Camera"))
        assertEquals(listOf("a b", "ü.txt"), StoragePaths.segments("a b/ü.txt"))
        for (bad in listOf("/", "/etc/passwd", "a//b", "a/", "..", "../x", "a/../b", "a/..", ".", "./a", "a/./b", "a\u0000b", "x".repeat(Limits.PATH_MAX + 1))) {
            assertNull("accepted: $bad", StoragePaths.segments(bad))
        }
        // `..` inside a name is just a name.
        assertEquals(listOf("a..b", "...", ".hidden"), StoragePaths.segments("a..b/.../.hidden"))
    }

    @Test
    fun resolveStaysUnderRoot() {
        File(root, "DCIM/Camera").mkdirs()
        File(tmp, "secret").writeText("x")
        val base = root.canonicalFile
        assertEquals(base, StoragePaths.resolve(root, ""))
        assertEquals(File(base, "DCIM/Camera"), StoragePaths.resolve(root, "DCIM/Camera"))
        assertNull(StoragePaths.resolve(root, "../secret"))
        assertNull(StoragePaths.resolve(root, "/etc"))
        // A sibling whose name starts with the root's name isn't inside it.
        File(tmp, "root2").mkdirs()
        assertFalse(StoragePaths.under(base, File(tmp, "root2").canonicalFile))
        // Symlinks out of the root are refused; links inside it are fine.
        Files.createSymbolicLink(File(root, "out").toPath(), tmp.toPath())
        Files.createSymbolicLink(File(root, "pics").toPath(), File(root, "DCIM").toPath())
        assertNull(StoragePaths.resolve(root, "out"))
        assertNull(StoragePaths.resolve(root, "out/secret"))
        assertEquals(File(base, "DCIM/Camera"), StoragePaths.resolve(root, "pics/Camera"))
    }

    @Test
    fun hiddenEntriesAreSkipped() {
        assertFalse(StoragePaths.visible(".nomedia"))
        assertFalse(StoragePaths.visible(""))
        assertTrue(StoragePaths.visible("Download"))
    }

    private fun file(name: String, dir: Boolean = false) = FileEntry(name, dir, if (dir) 0 else 1, 0, if (dir) "" else "text/plain")

    @Test
    fun listIsFoldersFirstThenNameIgnoringCase() {
        val (out, cut) = StoragePaths.fit(listOf(file("b.txt"), file("Music", true), file("A.txt"), file("alarms", true), file("a.txt")))
        assertEquals(listOf("alarms", "Music", "A.txt", "a.txt", "b.txt"), out.map { it.name })
        assertFalse(cut)
    }

    @Test
    fun listIsCutToCountAndBudget() {
        val many = (0 until 6000).map { file("f%05d".format(it)) }
        val (byCount, cut1) = StoragePaths.fit(many)
        assertEquals(Limits.FILES_MAX, byCount.size)
        assertTrue(cut1)
        assertEquals("f00000", byCount.first().name)

        // 255-byte names: 5000 of them would be ~1.5 MB, over the 1 MiB frame limit.
        val long = (0 until 5000).map { file("%05d".format(it) + "x".repeat(250)) }
        val (byBudget, cut2) = StoragePaths.fit(long)
        assertTrue(cut2)
        assertTrue(byBudget.size in 1 until 5000)
        val json = WireJson.encodeToString(FilesListReply.serializer(), FilesListReply(1, "x".repeat(4096), byBudget, true))
        assertTrue(json.toByteArray().size < 1 shl 20)
    }
}
