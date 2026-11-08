# Samsung TV: quick test

You only need Director, your TV remote and a device connected to an HDMI port. Keep the TV and PC on the same home network.

1. **Add the TV.** Turn it on. In Director, open **TVs → + → Samsung Tizen**. Give it any name and enter its IP address, found in the TV’s network settings.
2. **Pair.** Choose **Pair TV** and accept Director’s connection request on the TV.
3. **Test HDMI switching.** Choose an HDMI port with something connected. Use the TV remote to switch to a different input or the TV home screen. In Director, tick **The TV is on a different input**, then click **Test HDMI**. Answer **Yes** only if the TV actually switches to the chosen port. Repeat for any other ports you want to test. You can skip this if you have no HDMI device connected.
4. **Test power.** Continue through setup, leave the optional Windows display/audio links empty, and **Save TV**. Click the **remote icon** beside the TV. Check its power status, try **Turn off**, then try **Wake**. Give it up to 45 seconds to come on. If Wake is missing or fails, leave that result as No / Not tested as appropriate; you do not need to configure advanced settings to send a useful report.
5. **Fill in the results.** Close controls and click the TV’s **pencil icon**. Under **Test report**, enter the TV model and software version from **Settings → About / Support** if available. Select Wi-Fi or Ethernet and answer the three Yes / No / Not tested questions based on what you actually saw.
6. **Send one file.** Click **Export report…**. A folder opens. Attach the newest **samsung-compatibility-….json** file to a [new GitHub issue](https://github.com/Xarishark/Director/issues/new) with your TV model in the title. No other files or logs are needed.

**If pairing fails:** stay on that step, fill in the model/network details under **Test report**, and click **Export report…** there. Send the JSON even if nothing worked. If Director itself will not open, say so in the issue instead.

The file includes your answers, HDMI results, detected TV information, Director/Windows details and sanitized error history. It excludes pairing secrets and IP/MAC addresses. Nothing is sent automatically.

[Developer details](SAMSUNG_TECHNICAL.md)
