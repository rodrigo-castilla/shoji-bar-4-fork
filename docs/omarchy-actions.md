# Omarchy actions (planned, not started)

Goal: like `Super+Alt+Space` opens apps through Omarchy, let the shell emulate more Omarchy
actions, starting with media-track notifications (next/previous song).

Open scope, to be designed before coding:
- Which Omarchy commands to wrap (`omarchy-*`), and how the shell invokes them (a plugin, not core).
- Notification when the track changes (via the MPRIS player the music panel already follows).
- Next/previous/play-pause actions exposed as hooks for plugins.
