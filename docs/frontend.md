# Native frontend integration

The pages and reusable controls in `src/ui/design/` were adapted from the user's supplied `purr-slint/ui/` project. Their visual reference is the user's `PURR-desktop-v5.html`.

The integrated files include the top bar, ear band, overview, node spectrum and detail panel, subscriptions, connections/logs, rules, preferences, switches, segmented controls and tray popover. `src/ui/native-icons.slint`, `faces.slint` and `animation.slint` retain the supplied native path geometry and animation functions.

The supplied project's demonstration Rust engine is not part of this application. `src/window/design.rs` connects its Slint presentation state and callbacks to the existing real controller and command handlers. Production rows come from Mihomo and saved subscriptions. Only the explicit `--preview` entry point supplies demonstration values.

`src/ui/types.slint` shares row types between the pages and the backend. Connection IDs remain strings. Subscription IDs survive URL edits. Connection, subscription, rule and log rows use scrolling lists; rule and connection models retain all matching rows.

The real OS tray window uses the same supplied tray layout as the in-window preview, with independent state and real actions. The supplied fake desktop, fake OS menubar and demo-state controls are excluded from production.

Both OS status-item mouse buttons open the designed Slint popover on mouse-down. The older native context menu is no longer presented by those clicks. Initial unfocused events are ignored until the popover acquires focus; later loss events are checked after 100 ms to allow a status-item press to toggle the current window. Using mouse-down prevents a held press from reopening it on release. On macOS the application is activated before the popover requests focus, and the native view's layer masks the software surface to a 12 px corner radius. The six page buttons are in the ear band, with zero flex gap and the icon's original 3 px bottom margin.

A bounded path provides the wake/sleep background flood. It avoids allocating the supplied prototype's oversized circle. The window follows the OS color scheme and reduced-motion preference. The original SVG comparison generator writes only to the legacy comparison directory and cannot overwrite active native icon components.

Theme colors interpolate over 500 ms, and the traffic lines use the original per-frame sine displacement while awake in rule/global modes. Reduced motion disables both. Pages retain their full layout through scrolling in smaller windows; additional route groups scroll horizontally. Windows tray shortcuts display `Ctrl+` labels.

CSS easing curves are solved as cubic Bezier curves instead of smoothstep approximations. Page entrance uses `.3,.8,.3,1` over 350 ms. Toasts appear at the top right and interpolate position/opacity over 250 ms; tray entrance uses 180 ms, trouble entrance 350 ms, and import guidance shakes three times from its trigger time. Icon blink, pulse and wag phases restart when enabled, and one-shot hover motion stops when the pointer leaves. Flowing dashes have 60 phases per 900 ms cycle instead of 16, and remain dashed at rest under reduced motion. Shared controls interpolate hover/selection color over 500 ms (400 ms for native tray controls). Reduced motion leaves the hero glyphs in their untransformed positions.

The background flood fixes its center when wake/sleep changes, and its percentage radius uses CSS's normalized diagonal. `tools/check_animation.py` compares 51 actual Slint curve evaluations to samples measured from the HTML in Chromium. This verifies those curves, not every rendered animation frame.

Rules share immutable storage between state snapshots. The UI reuses its published rule rows while the rules, filter and hit counters remain unchanged. Model equality checks stream through rows instead of allocating another comparison vector. These optimizations preserve complete lists and rule updates.

The software renderer is the default compile-time feature set. Build with `--features gpu-renderer` to include the optional femtovg renderer.

`vendor/softbuffer` retains version 0.4.8 and backports upstream CoreGraphics buffer reuse and surface-layer removal. The direct provider synchronizes compositor reads before the renderer writes to the same storage; a size change creates a new buffer. Display resolution, page geometry and animation behavior are unchanged. Other platform backends retain their original code. See its `PATCHES.md` for pinned upstream commits and adaptation details.

`tools/gen_icons.py`, its SVG path helper and `tools/icon_sources/` are adapted from the supplied frontend's generator and original JSON geometry. Run `python3 tools/gen_icons.py` to check that active icon and face components reproduce exactly; `--write` regenerates them. Animation clocks and theme bindings target the integrated globals. This is separate from the legacy `tools/generate_icons.py` comparison output.

Preview captures demonstrate appearance and interaction checks, not real traffic, resource usage or Windows runtime compatibility. See `verification.md` for measured results and remaining validation limits.

The software renderer in Slint 1.18.1 leaves item rotation and scaling unimplemented; upstream `master` still has the same empty methods. Icon, face and switch rotations therefore operate on their native SVG path coordinates, including Bézier controls and elliptical arc axes. SVG paths compensate Slint's stroke inset so their viewbox is not shrunk again. The switch translates the entire SVG before rotation, in CSS pixels. Its head, closed eyes and open eyes share that transform. The log cursor changes its rectangle height directly.

The switch knob and track implement the CSS reversing shortening factor and retain fractional millisecond durations. The shared knob is generated into `knob.slint`; the supplied and integrated controls use one implementation. Settings keep the latest uncommitted click visible during status refreshes. A single worker saves settings in click order; an older completion cannot clear a newer request, and failed saves return to committed preferences. Preview settings also update their demonstration state.

The ear band follows the actual window width, includes the reference's occasional asymmetric twitch, uses its 16 MiB/s load scale, rounds the hold split and draws the 5/8 dashed line when awake without system proxy or TUN. `tools/fixtures/wave-v5.json` contains 1,450 coordinates captured directly from the original `drawLine(t)` across nine states and three widths, with the HTML's SHA-256.

`tools/check_switch.py --browser-reference` exercises the shared native component through pointer events, compares ordinary and interrupted transitions against CSS and the owned Chromium reference tab, and compares three rendered switch frames with the original SVG. It separately checks delayed-setting refresh, repeated clicks, stale completion, rollback, and the original canvas samples. Without `--browser-reference`, it runs the CSS equation and canvas fixture comparisons. These checks do not establish full application pixel equivalence.
