package io.github.sharjeelmazhar.continuum.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp

/** Green used for "on" and "connected" (readable on light and dark surfaces). */
val GoodGreen = Color(0xFF1E9E63)

/** Small coloured header above a group, e.g. "Notifications". */
@Composable
fun SectionHeader(text: String, modifier: Modifier = Modifier, trailing: String? = null) {
    Row(
        modifier.fillMaxWidth().padding(start = 16.dp, end = 16.dp, top = 24.dp, bottom = 10.dp),
        verticalAlignment = Alignment.Bottom,
    ) {
        Text(
            text,
            style = MaterialTheme.typography.titleSmall,
            color = MaterialTheme.colorScheme.primary,
            modifier = Modifier.weight(1f).semantics { heading() },
        )
        if (trailing != null) {
            Text(trailing, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
}

/**
 * Rows stacked as one rounded block with hairline gaps between them (the Android 16 Settings
 * look): large corners outside, small ones between rows.
 */
@Composable
fun SettingsGroup(content: @Composable ColumnScope.() -> Unit) {
    Column(
        Modifier.fillMaxWidth().padding(horizontal = 16.dp).clip(RoundedCornerShape(24.dp)),
        verticalArrangement = Arrangement.spacedBy(2.dp),
        content = content,
    )
}

/** Round tonal badge behind a row's icon. */
@Composable
fun IconBadge(icon: ImageVector, tint: Color = MaterialTheme.colorScheme.onSecondaryContainer, container: Color = MaterialTheme.colorScheme.secondaryContainer) {
    Surface(shape = CircleShape, color = container, modifier = Modifier.size(40.dp)) {
        Box(contentAlignment = Alignment.Center) {
            Icon(icon, contentDescription = null, tint = tint, modifier = Modifier.size(22.dp))
        }
    }
}

/**
 * The coloured rounded-square tile in front of a settings row (like iOS Settings): a white
 * glyph on a soft vertical gradient, its colour picked from what the icon means.
 */
@Composable
fun SettingsTile(icon: ImageVector, enabled: Boolean = true, size: androidx.compose.ui.unit.Dp = 34.dp) {
    val base = tileColor(icon.name)
    val top = androidx.compose.ui.graphics.lerp(base, Color.White, 0.18f)
    Box(
        Modifier
            .size(size)
            .clip(RoundedCornerShape(size * 0.265f))
            .background(androidx.compose.ui.graphics.Brush.verticalGradient(listOf(top, base)))
            .then(if (enabled) Modifier else Modifier.background(Color.Black.copy(alpha = 0.25f))),
        contentAlignment = Alignment.Center,
    ) {
        Icon(icon, contentDescription = null, tint = Color.White, modifier = Modifier.size(size * 0.62f))
    }
}

/** Apple-style system colours by what the icon means; anything unknown is grey. */
internal fun tileColor(name: String): Color = when (name.substringAfterLast('.')) {
    "Notifications", "NotificationsOff" -> Red
    "Lock" -> Indigo
    "Screenshot", "ContentPaste", "Language", "DesktopWindows", "OpenInNew", "Folder" -> Blue
    "PhotoLibrary" -> Orange
    "Sms" -> Green
    "UploadFile" -> Purple
    "Mouse" -> Purple
    "BatteryChargingFull", "SystemUpdate" -> Green
    "AccountBalance", "AddBox" -> Teal
    "RestartAlt" -> Orange
    else -> Grey
}

private val Red = Color(0xFFFF3B30)
private val Orange = Color(0xFFFF9500)
private val Green = Color(0xFF34C759)
private val Teal = Color(0xFF30B0C7)
private val Blue = Color(0xFF007AFF)
private val Indigo = Color(0xFF5856D6)
private val Purple = Color(0xFFAF52DE)
private val Grey = Color(0xFF8E8E93)

/** One settings row: icon, title, optional summary, and something at the end (switch, status). */
@Composable
fun SettingsRow(
    title: String,
    modifier: Modifier = Modifier,
    summary: String? = null,
    icon: ImageVector? = null,
    titleColor: Color = MaterialTheme.colorScheme.onSurface,
    enabled: Boolean = true,
    onClick: (() -> Unit)? = null,
    trailing: (@Composable () -> Unit)? = null,
) {
    val clickable = if (onClick != null) Modifier.clickable(enabled = enabled, role = Role.Button, onClick = onClick) else Modifier
    RowFrame(modifier.then(clickable), title, summary, icon, titleColor, enabled, trailing)
}

/** A row whose whole surface toggles a switch (one touch target, read out as a switch). */
@Composable
fun SwitchSettingsRow(
    title: String,
    checked: Boolean,
    onChange: (Boolean) -> Unit,
    summary: String? = null,
    icon: ImageVector? = null,
    enabled: Boolean = true,
) {
    RowFrame(
        Modifier.toggleable(value = checked, enabled = enabled, role = Role.Switch, onValueChange = onChange),
        title, summary, icon, MaterialTheme.colorScheme.onSurface, enabled,
    ) { Switch(checked = checked, onCheckedChange = null, enabled = enabled) }
}

@Composable
private fun RowFrame(
    modifier: Modifier,
    title: String,
    summary: String?,
    icon: ImageVector?,
    titleColor: Color,
    enabled: Boolean,
    trailing: (@Composable () -> Unit)?,
) {
    val alpha = if (enabled) 1f else 0.45f
    Row(
        modifier
            .fillMaxWidth()
            .background(MaterialTheme.colorScheme.surfaceContainer)
            .heightIn(min = 72.dp)
            .padding(horizontal = 16.dp, vertical = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (icon != null) {
            SettingsTile(icon, enabled)
            Spacer(Modifier.width(16.dp))
        }
        Column(Modifier.weight(1f)) {
            Text(title, style = MaterialTheme.typography.bodyLarge, color = titleColor.copy(alpha = alpha))
            if (summary != null) {
                Text(summary, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = alpha))
            }
        }
        if (trailing != null) {
            Spacer(Modifier.width(12.dp))
            trailing()
        }
    }
}

/** Small explanatory text under a group. */
@Composable
fun GroupFootnote(text: String) {
    Text(
        text,
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        modifier = Modifier.padding(start = 32.dp, end = 32.dp, top = 8.dp),
    )
}
