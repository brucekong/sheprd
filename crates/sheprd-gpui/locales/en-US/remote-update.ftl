# Updating Sheprd on another machine (an SSH or cloud device) from this
# app: the pill on the device's sidebar header and the confirmation it opens.
# $host is the device's name in the sidebar; $current and $latest are
# release versions such as 20261109.1.

# The pill when the machine's Sheprd offers a newer release.
remote-update-pill = Update
# The pill while that machine downloads, installs, and restarts.
remote-update-pill-updating = Updating…
# The pill when the update did not finish.
remote-update-pill-failed = Update failed
remote-update-pill-tooltip = { $host }: Sheprd { $latest } is available. Click to update it there.
remote-update-pill-tooltip-newer = { $host }: a newer Sheprd is available. Click to update it there.
remote-update-pill-tooltip-updating = { $host } is updating Sheprd
remote-update-pill-tooltip-failed = { $host } did not update Sheprd. Click for details.

remote-update-title = Update Sheprd on { $host }?
remote-update-versions = { $host } runs Sheprd { $current }. Sheprd { $latest } is available.
remote-update-latest = Sheprd { $latest } is available on { $host }.
remote-update-newer = A newer Sheprd is available on { $host }.
remote-update-note = Sheprd there downloads the signed release, installs it, and restarts. Its terminals and agents keep running.
remote-update-confirm = Update
remote-update-retry = Try again
remote-update-cancel = Cancel
remote-update-close = Close
remote-update-updating-title = Updating Sheprd on { $host }
remote-update-updating-body = Sheprd there downloads and installs the release, then restarts. This can take a few minutes.
remote-update-failed-title = Sheprd on { $host } did not update
remote-update-current-title = Sheprd on { $host } is up to date
remote-update-current-body = There is no newer release for it now.
