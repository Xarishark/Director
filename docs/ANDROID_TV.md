# TCL Android / Google TV: quick test

This is for a TCL TV running **Android TV or Google TV**. You only need Director, your TV remote and a device connected to HDMI. Keep the TV and PC on the same home network.

1. **Add the TV.** Turn it on. In Director, open **TVs → + → TCL — Android / Google TV**. Give it any name and enter its IP address, found in the TV’s network settings.
2. **Pair.** Choose **Pair TV**. Enter the six-character code shown on the TV, then click **Confirm code**. If it fails, use **Back** and try pairing again with a new code.
3. **Test HDMI switching.** Choose an HDMI port with something connected. Use the TV remote to switch to a different input or the TV home screen. Tick **The TV is on a different input**, then click **Test HDMI**. Answer **Yes** only if the TV actually switches to the chosen port. Repeat for other ports if wanted, or skip if no HDMI device is connected.
4. **Test power.** Continue through setup, leave the optional Windows display/audio links empty, and **Save TV**. Click the **remote icon**. Check the power status, try **Turn off**, then **Wake**. Allow up to 45 seconds. If wake fails, try enabling **Screenless service / network standby** in the TV’s **Settings → System → Power and energy**, if available, and repeat the test.
5. **Fill in the results.** Close controls and click the TV’s **pencil icon**. Under **Test report**, enter the model and TV software version from **Settings → About**, if available. Select Wi-Fi or Ethernet and answer the three Yes / No / Not tested questions based on what you saw.
6. **Send one file.** Click **Export report…**. A folder opens. Attach the newest **android-tv-compatibility-….json** file to a [new GitHub issue](https://github.com/Xarishark/Director/issues/new) with your TV model in the title. No other files or logs are needed.

**If pairing fails:** fill in the model/network details under **Test report** on that step and export there. Send the JSON even if nothing worked. If Director itself will not open, say so in the issue instead.

The file includes your answers, HDMI results, detected TV information, Director/Windows details and sanitized error history. It excludes pairing codes, keys and IP/MAC addresses. Nothing is sent automatically.

[Developer details](ANDROID_TV_TECHNICAL.md)
