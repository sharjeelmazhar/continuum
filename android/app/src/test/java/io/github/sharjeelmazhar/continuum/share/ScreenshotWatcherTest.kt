package io.github.sharjeelmazhar.continuum.share

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ScreenshotWatcherTest {
    @Test
    fun screenshotsFromEveryMaker() {
        // Pixel, Motorola, Huawei; Xiaomi/Redmi/POCO, Samsung, OnePlus, OPPO; older vivo; Android 9 paths.
        assertTrue(ScreenshotWatcher.isScreenshot("Pictures/Screenshots/"))
        assertTrue(ScreenshotWatcher.isScreenshot("DCIM/Screenshots/"))
        assertTrue(ScreenshotWatcher.isScreenshot("Screenshots/"))
        assertTrue(ScreenshotWatcher.isScreenshot("/storage/emulated/0/DCIM/Screenshots/Screenshot_1.jpg"))
        assertTrue(ScreenshotWatcher.isScreenshot("Pictures/screenshot/"))
        // Any folder, when the file is named like a screenshot.
        assertTrue(ScreenshotWatcher.isScreenshot("DCIM/Camera/", "Screenshot_2026-11-03-21-10-05-123_com.whatsapp.jpg"))
        assertTrue(ScreenshotWatcher.isScreenshot("Pictures/", "screenshot_20261103.png"))
    }

    @Test
    fun otherPicturesAreNot() {
        assertFalse(ScreenshotWatcher.isScreenshot("DCIM/Camera/", "IMG_20261103_101010.jpg"))
        assertFalse(ScreenshotWatcher.isScreenshot("Pictures/WhatsApp/", "IMG-20261103-WA0001.jpg"))
        assertFalse(ScreenshotWatcher.isScreenshot("Pictures/ScreenshotsBackup/", "IMG_1.jpg"))
    }
}
