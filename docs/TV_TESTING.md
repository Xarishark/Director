# Test a Samsung or TCL TV

Samsung Tizen and TCL Android / Google TV support is a preview: it works on the protocol level, but every model behaves a little differently. If you have one of these TVs, a test report tells us what works on yours. It takes about ten minutes.

You need Director, your TV remote, and a device connected to one of the TV's HDMI ports. Keep the TV and the PC on the same home network.

1. **Add the TV.** Turn it on. In Director, open **TVs → +** and choose **Samsung Tizen** or **TCL — Android / Google TV**. Give it any name and enter its IP address, from the TV's network settings.
2. **Pair.** Choose **Pair TV**.
   - Samsung: accept Director's connection request on the TV.
   - TCL: type the six-character code the TV shows, then choose **Confirm code**. If it fails, use **Back** and pair again for a new code.
3. **Test HDMI switching.** Choose an HDMI port with something connected. With the remote, switch the TV to a different input or its home screen. In Director, tick **The TV is on a different input** and choose **Test HDMI**. Answer **Yes** only if the TV actually switched to that port. Repeat for other ports if you like, or skip this step if nothing is connected.
4. **Test power.** Continue through setup, leave the optional Windows display and audio links empty, and save the TV. Choose the **remote icon** on the TV's card: check that the power status is right, try **Turn off**, then **Wake**. Give it up to 45 seconds to come on.
   - Samsung: Wake needs the TV's network standby setting.
   - TCL: if Wake fails, turn on **Screenless service** or **network standby** under the TV's **Settings → System → Power and energy**, if it has one, and try again.
5. **Fill in the results.** Close the controls and choose the TV's **pencil**. Under **Test report**, enter the TV model and software version from the TV's **Settings → About** (or **Support**), choose Wi-Fi or Ethernet, and answer the three Yes / No / Not tested questions with what you actually saw.
6. **Send the report.** Choose **Export report…**. A folder opens. Attach the newest `samsung-compatibility-….json` or `android-tv-compatibility-….json` file to a [new GitHub issue](https://github.com/Xarishark/Director/issues/new) with your TV model in the title.

**If pairing fails**, stay on that step, fill in the model and network details under **Test report**, and choose **Export report…** there. A report from a failed pairing helps just as much. If Director itself won't open, say so in the issue instead.

## What the report contains

The report holds your answers, the HDMI test results, what the TV reported (model, software version, power state), the Director version, the Windows version and build, a fingerprint of `director.exe`, and a cleaned-up history of the TV actions you tried with this build: what was tried, the outcome, timing and a coarse failure reason.

It never contains pairing codes, keys, tokens, certificates, IP or MAC addresses, TV or device names, raw TV messages or log lines. Director builds it from an allowlist instead of filtering a log, and never sends it anywhere by itself. You can also export it with `director tv report <tv>`.
