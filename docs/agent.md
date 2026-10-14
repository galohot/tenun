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
When the workspace is a site, choose **Publish the whole site** in the same sheet and set
**Keep it updated** there. The agent then keeps the site updated too, as long as the workspace
stays a site.
Copy that file and its data files to the always-on computer, or keep them on a shared drive.

The agent also watches the folders of a folder source, and fetches CSV links on the schedule you
chose for each link. Copy the data folder next to the `.tenun` file too: downloaded links are kept
in a folder called `<workspace name> data`.

To test: `tenun publish path/to/sales.tenun` publishes every dashboard that has publish settings,
and the site when it has them, once, and prints the links.

### A password

A password is not in the `.tenun` file. It is kept in the private secrets file of the computer
where it was set, and tenun.cloud keeps a hash of it. So the agent on another computer does not need
it: it publishes, and the password stays as it is.

To set a password from the command line, put it in an environment variable and name the variable:

```sh
TENUN_PASSWORD='the password' tenun publish path/to/sales.tenun --password-from-env TENUN_PASSWORD
```

This gives every dashboard it publishes (or the one named with `--dashboard`) that password. Their
access becomes "anyone with the link and the password". Do not type a password as a command-line
argument. Other users of the computer can see the arguments of a running program.

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

## Alerts

An alert watches one number tile. When its value goes below, above or outside the numbers you set,
Tenun sends one message. It can send one more when the value is back to normal. Set one in the app:
turn on **Advanced features** in Settings, open the tile's **⋯** menu, and choose **Alert me…**.

Tenun reads the value as the page opens: its saved filters, its filter tiles and its default view.
It never reads someone's clicks. The app checks after each reload of the data and once a minute,
while it is open. `tenun agent` checks on each cycle, with or without publishing. To check once from
a scheduled task, run:

```sh
tenun alerts check path/to/sales.tenun
```

A message that cannot be sent is tried again at the next check, three times in all. Then the alert
shows the error in Settings › Alerts, and Tenun stops trying until the value starts to hold again.

The address of an alert (a web address, or a Telegram bot token) is kept in the private secrets file
of the computer where you set it. It is not in the `.tenun` file. So set the alert on the computer
that checks it. On another computer, the alert says that it has no address there.

**What a message holds:** one sentence with the tile's name, its value, the period and the number it
was compared with, the dashboard's name, and its tenun.cloud link when it is published. Nothing else.
For example: `Revenue (Oct 1–14, 2026, so far) is 12.1M, below 15M. — Coffee overview`.

### Slack

In Slack, add the **Incoming Webhooks** app to the channel. Copy its webhook URL
(`https://hooks.slack.com/services/…`) and paste it in **Send to › Slack**.

### Microsoft Teams

In the Teams channel, open **Workflows** and choose the template **Post to a channel when a webhook
request is received**. Copy the URL it gives you and paste it in **Send to › Microsoft Teams**.
Tenun sends the message as an Adaptive Card, which is what that template posts.

### Discord

In Discord, open **Server Settings › Integrations › Webhooks**, make a webhook for the channel, and
copy its URL (`https://discord.com/api/webhooks/…`).

### Telegram

1. In Telegram, talk to **@BotFather**, send `/newbot`, and copy the token it gives you
   (`123456789:AA…`).
2. Add the bot to the group or channel.
3. Type the chat id: a number such as `-1001234567890`, or `@channelname` for a public channel.

### Another web address

Tenun sends a `POST` with a JSON body:

```json
{
  "rule": "a3f9…", "event": "start", "name": "Revenue", "value": 12100000,
  "op": "below", "threshold": 15000000, "to": null, "period": "Oct 1–14, 2026, so far",
  "dashboard": "Coffee overview", "link": "https://tenun.cloud/d/…", "at": "2026-10-14T09:10:00Z",
  "text": "Revenue (Oct 1–14, 2026, so far) is 12.1M, below 15M. — Coffee overview"
}
```

`event` is `start` when the alert starts to hold, `back` when it is back to normal, and `test` for
**Send a test**. Any answer from 200 to 299 counts as sent.

## When something goes wrong

- A failed publish is tried again after 5 minutes, then 10, 20 and so on, up to 6 hours.
- If tenun.cloud says the free plan's limit is reached, the agent waits exactly as long as it is told.
- The dashboard's page on tenun.cloud shows a banner when an update is more than twice its schedule
  late, so viewers are never shown old numbers without knowing it.
- `tenun logout` removes this computer's connection. Disconnect it on your account page as well.
