# dmgbuild settings for the Roundtable disk image (desktop-release.yml).
# dmgbuild writes the window layout itself, so no Finder or AppleScript runs on the runner.
# Run: dmgbuild -s desktop/scripts/dmg-settings.py -D app=<Roundtable.app> -D background=<png> "Roundtable" <out.dmg>
import os.path

app = defines["app"]  # noqa: F821 (dmgbuild provides defines)
background = defines["background"]  # noqa: F821
appname = os.path.basename(app)

format = "UDZO"
filesystem = "HFS+"
files = [app]
symlinks = {"Applications": "/Applications"}
hide_extension = [appname]

# Matches core/dmg/background.png (660 x 400): the app left of the arrow, Applications right of it.
window_rect = ((200, 120), (660, 400))
icon_size = 96
text_size = 13
icon_locations = {appname: (180, 212), "Applications": (480, 212)}
default_view = "icon-view"
show_status_bar = False
show_tab_view = False
show_toolbar = False
show_pathbar = False
show_sidebar = False
