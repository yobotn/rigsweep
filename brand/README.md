# Brand studio

A local-only page that renders every RigSweep icon and the social card from one mark (`assets/mark.svg`, the R with a green square).

```
npm run brand        # then open http://127.0.0.1:4747
```

Tune the icon and the card, then click **Write all to repo**. It writes:

| File | What |
| --- | --- |
| `brand/app-icon.png` | 1024 px master, fed to `tauri icon` |
| `src-tauri/icons/*` | app, installer and taskbar icons (.ico, .icns, PNGs) |
| `docs/favicon-32.png`, `docs/icon.png` | site favicon and touch icon |
| `chrome-extension/icons/icon{16,32,48,128}.png` | extension icons |
| `docs/social.png` | 1280 × 640 Open Graph / Twitter card |
| `brand/brand.json` | the settings used, reloaded next time |

The server binds to 127.0.0.1, reads only `brand/`, `docs/screenshots/` and the font folders, and writes only the files above. `brand/` is not part of the site, so none of it is deployed.

Broom art: [Fluent Emoji](https://github.com/microsoft/fluentui-emoji) © Microsoft Corporation, MIT license (`assets/LICENSE-fluentui-emoji.txt`).
