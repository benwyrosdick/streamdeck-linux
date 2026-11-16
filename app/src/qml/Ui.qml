pragma Singleton
import QtQuick

// Surfaces are fixed dark greys like Omarchy's own apps; only the accent
// follows the desktop theme.
QtObject {
    readonly property color window: "#0e0e10"
    readonly property color panel: "#161619"
    readonly property color raised: "#1d1d21"
    readonly property color well: "#0a0a0b"
    readonly property color border: "#2a2a30"
    readonly property color text: "#e9e9ec"
    readonly property color dim: "#9b9ba3"
    readonly property color faint: "#64646c"
    readonly property color danger: "#ff5f57"
    readonly property color accent: theme.accent
    readonly property color accentText: theme.accentForeground
    readonly property int radius: theme.radius
    readonly property int small: Math.min(4, theme.radius)
}
