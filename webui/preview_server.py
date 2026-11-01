#!/usr/bin/env python3
import http.server
import socketserver
import os
import sys

PORT = 8080
DIRECTORY = os.path.dirname(os.path.abspath(__file__))

MOCK_BRIDGE = """
<script>
// Mock KernelSU / APatch / Magisk WebUI bridge for browser preview
(function() {
    const mockPackages = [
        { packageName: "com.google.android.gms", appLabel: "Google Play services" },
        { packageName: "com.android.vending", appLabel: "Google Play Store" },
        { packageName: "com.google.android.apps.walletnfcrel", appLabel: "Google Wallet" },
        { packageName: "com.google.android.gsf", appLabel: "Google Services Framework" },
        { packageName: "com.snapchat.android", appLabel: "Snapchat" },
        { packageName: "org.telegram.messenger", appLabel: "Telegram" },
        { packageName: "com.spotify.music", appLabel: "Spotify" },
        { packageName: "com.whatsapp", appLabel: "WhatsApp" },
        { packageName: "io.github.vvb2060.keyattestation", appLabel: "Key Attestation" },
        { packageName: "com.reveny.nativecheck", appLabel: "Native Detector" },
        { packageName: "com.sbi.lotusintouch", appLabel: "YONO SBI" },
        { packageName: "com.phonepe.app", appLabel: "PhonePe" }
    ];

    let targetTxt = "com.google.android.gms!\\ncom.android.vending\\ncom.google.android.apps.walletnfcrel";

    // Pre-populate tricky-applist-cache so app list renders immediately without waiting
    try {
        const appEntries = mockPackages.map(p => ({ appName: p.appLabel, packageName: p.packageName }));
        localStorage.setItem("tricky-applist-cache", JSON.stringify({
            entries: appEntries,
            target: targetTxt
        }));
    } catch(e) {}

    // Pre-seed tricky-webui-cache
    try {
        localStorage.setItem("tricky-webui-cache", JSON.stringify({
            v: 2,
            ts: Date.now(),
            module: { version: "v5.53.1" },
            status: {
                engine: "TrickyStore",
                engineRunning: true,
                restartCount: 0,
                tsForkSupported: true,
                isAospDevice: false,
                magiskAvailable: true
            },
            keybox: {
                valid: true,
                rootType: "google",
                validationErrors: []
            },
            config: {
                keybox: { enabled: true, interval: "12h", source: "takr" },
                region: { enabled: false },
                security_patch: { auto_update: true }
            }
        }));
    } catch(e) {}

    const ksuBridge = {
        listPackages: function(type) {
            if (type === "system") {
                return JSON.stringify(["com.android.settings", "com.google.android.gsf", "com.android.shell"]);
            }
            return JSON.stringify(mockPackages.map(p => p.packageName));
        },
        getPackagesInfo: function(json) {
            try {
                const pkgs = typeof json === "string" ? JSON.parse(json) : json;
                const res = pkgs.map(pkg => {
                    const found = mockPackages.find(p => p.packageName === pkg);
                    return found || { packageName: pkg, appLabel: pkg };
                });
                return JSON.stringify(res);
            } catch (e) {
                return "[]";
            }
        },
        toast: function(msg) {
            console.log("[KSU TOAST]:", msg);
            const prompt = document.getElementById("prompt");
            if (prompt) {
                const text = prompt.querySelector(".prompt-text");
                if (text) text.textContent = msg;
                const container = document.querySelector(".prompt-container");
                if (container && container.showPopover) {
                    try { container.showPopover(); } catch(e){}
                    setTimeout(() => { try { container.hidePopover(); } catch(e){} }, 2000);
                }
            }
        },
        exec: function(cmd, env, callback) {
            setTimeout(() => {
                let stdout = "";
                let errno = 0;
                if (cmd.includes("target.txt")) {
                    if (cmd.startsWith("echo") || cmd.startsWith("cat >")) {
                        stdout = "";
                    } else {
                        stdout = targetTxt;
                    }
                } else if (cmd.includes("system_app")) {
                    stdout = "com.google.android.gms\\ncom.google.android.gsf";
                } else if (cmd.includes("config get")) {
                    stdout = "true";
                } else if (cmd.includes("status update")) {
                    stdout = "OK";
                } else if (cmd.includes("keybox fetch") || cmd.includes("fetch-keybox")) {
                    stdout = "keybox fetched from takr";
                } else if (cmd.includes("config.toml")) {
                    stdout = 'custom_url = ""';
                } else if (cmd.includes("more-exclude.json")) {
                    stdout = "{}";
                } else if (cmd.includes("pif fetch")) {
                    stdout = "PIF successfully updated: Pixel 8a (akita_beta)";
                } else if (cmd.includes("pif.auto_update")) {
                    stdout = "true";
                } else if (cmd.includes("killall")) {
                    stdout = "";
                } else if (cmd.includes("keybox.xml")) {
                    stdout = "<AndroidAttestation>...</AndroidAttestation>";
                }
                if (window[callback]) {
                    window[callback](errno, stdout, "");
                }
            }, 10);
        },
        spawn: function(cmd, args, env, callback) {
            return {
                stdout: { on: () => {} },
                stderr: { on: () => {} },
                on: (e, cb) => { if (e === "exit") setTimeout(() => cb(0), 10); },
                emit: () => {}
            };
        }
    };

    window.ksu = ksuBridge;
    globalThis.ksu = ksuBridge;
})();
</script>
"""

class CustomHandler(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=DIRECTORY, **kwargs)

    def do_GET(self):
        if self.path in ('/', '/index.html'):
            file_path = os.path.join(DIRECTORY, 'index.html')
            with open(file_path, 'r', encoding='utf-8') as f:
                content = f.read()

            # Inject mock bridge right after <head> so it runs BEFORE any module scripts
            content = content.replace('<head>', '<head>\n' + MOCK_BRIDGE)
            # Remove [unresolved] so elements render immediately
            content = content.replace(' unresolved', '')

            data = content.encode('utf-8')
            self.send_response(200)
            self.send_header('Content-Type', 'text/html; charset=utf-8')
            self.send_header('Content-Length', str(len(data)))
            self.end_headers()
            self.wfile.write(data)
        else:
            super().do_GET()

if __name__ == '__main__':
    class ReusableTCPServer(socketserver.TCPServer):
        allow_reuse_address = True

    with ReusableTCPServer(("", PORT), CustomHandler) as httpd:
        print(f"Tricky Addon WebUI preview server running at http://localhost:{PORT}")
        sys.stdout.flush()
        httpd.serve_forever()
