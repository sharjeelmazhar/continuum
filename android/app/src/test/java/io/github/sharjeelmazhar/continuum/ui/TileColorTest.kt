package io.github.sharjeelmazhar.continuum.ui

import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.AccountBalance
import androidx.compose.material.icons.outlined.Notifications
import androidx.compose.material.icons.outlined.PhotoLibrary
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Test

class TileColorTest {
    @Test
    fun iconsGetTheirOwnColours() {
        val grey = tileColor("Outlined.Whatever")
        assertNotEquals(grey, tileColor(Icons.Outlined.Notifications.name))
        assertNotEquals(grey, tileColor(Icons.Outlined.PhotoLibrary.name))
        assertNotEquals(tileColor(Icons.Outlined.Notifications.name), tileColor(Icons.Outlined.AccountBalance.name))
        assertEquals(tileColor("Filled.Notifications"), tileColor("Outlined.Notifications"))
    }
}
