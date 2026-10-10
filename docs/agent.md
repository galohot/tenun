# Keep published dashboards updated with `tenun agent`

The Tenun app updates your published dashboards while it is open. If the computer with the data is
turned off or the app is closed, the dashboards stop updating, and their pages on tenun.cloud say so.

`tenun agent` is the same publisher without a window. Run it on a computer that stays on and can
read the data: an office PC, a small server, or the database server itself. It only makes
outgoing HTTPS connections to tenun.cloud. You do not open any firewall ports, and your database
passwords stay on that computer.

## 1. Connect the computer (once)

```sh
tenun login
```

It prints a link and a code. Open the link on any device, sign in, check the code, and choose
**Connect**. The computer then appears under *Connected apps* on your account page, where you can
disconnect it at any time.

## 2. Choose what to publish

In the Tenun app, open each dashboard, choose **Publish**, and set **Keep it updated**
(when the data changes, every hour, or every day). The setting is saved in the `.tenun` file.
Copy that file and its data files to the always-on computer, or keep them on a shared drive.

To test: `tenun publish path/to/sales.tenun` publishes every dashboard that has publish settings,
once, and prints the links.

## 3. Run the agent

```sh
tenun agent path/to/sales.tenun
```

It checks once a minute. It publishes a dashboard when its schedule says so, or, for "when the data
changes", when a data file changes (at most once an hour on the free plan). It writes one line per
publish or error. Stop it with Ctrl+C.

### Start it with the computer

**Windows** (Task Scheduler, runs at sign-in):

```bat
schtasks /Create /TN "Tenun agent" /SC ONLOGON /RL LIMITED ^
  /TR "\"C:\Program Files\Tenun\tenun.exe\" agent \"C:\Data\sales.tenun\""
```

**Linux** (systemd user service):

```ini
# ~/.config/systemd/user/tenun-agent.service
[Unit]
Description=Tenun agent

[Service]
ExecStart=%h/.local/bin/tenun agent %h/data/sales.tenun
Restart=on-failure

[Install]
WantedBy=default.target
```

```sh
systemctl --user enable --now tenun-agent
loginctl enable-linger "$USER"   # keep it running after you sign out
journalctl --user -u tenun-agent -f
```

**macOS** (launchd):

```xml
<!-- ~/Library/LaunchAgents/cloud.tenun.agent.plist -->
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>Label</key><string>cloud.tenun.agent</string>
  <key>ProgramArguments</key>
  <array><string>/usr/local/bin/tenun</string><string>agent</string><string>/Users/me/Data/sales.tenun</string></array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>StandardErrorPath</key><string>/tmp/tenun-agent.log</string>
</dict></plist>
```

```sh
launchctl load ~/Library/LaunchAgents/cloud.tenun.agent.plist
```

## When something goes wrong

- A failed publish is tried again after 5 minutes, then 10, 20 and so on, up to 6 hours.
- If tenun.cloud says the free plan's limit is reached, the agent waits exactly as long as it is told.
- The dashboard's page on tenun.cloud shows a banner when an update is more than twice its schedule
  late, so viewers are never shown old numbers without knowing it.
- `tenun logout` removes this computer's connection. Disconnect it on your account page as well.
