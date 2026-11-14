(() => {
    let status = null;
    let statusSequence = 0;
    const controls = document.getElementById("push-controls");
    const statusText = document.getElementById("push-status");
    const enable = document.getElementById("push-enable");
    const test = document.getElementById("push-test");
    const disable = document.getElementById("push-disable");
    const worker = "serviceWorker" in navigator
        ? navigator.serviceWorker.register("/sw.js")
            .then(() => navigator.serviceWorker.ready).catch(() => null) : Promise.resolve(null);

    async function refreshStatus() {
        const sequence = ++statusSequence;
        const identity = controls?.dataset.userId;
        const controller = new AbortController();
        const timeout = setTimeout(() => controller.abort(), 3000);
        try {
            const response = await fetch("/push/status", { credentials: "same-origin", cache: "no-store", signal: controller.signal });
            const next = response.ok ? await response.json() : null;
            if (sequence !== statusSequence || identity !== controls?.dataset.userId) return;
            status = next;
        } catch (_) {
            if (sequence !== statusSequence) return;
            status = null;
        } finally {
            clearTimeout(timeout);
        }
        await renderStatus();
    }

    async function renderStatus() {
        if (!controls) return;
        const sequence = statusSequence;
        const identity = document.getElementById("user-selector")?.dataset.userId;
        if (identity && identity !== controls.dataset.userId) {
            controls.hidden = true;
            return;
        }
        controls.hidden = !status?.available;
        if (controls.hidden) return;
        const supported = "Notification" in window && "PushManager" in window && "serviceWorker" in navigator;
        const permission = supported ? Notification.permission : "unsupported";
        let active = false;
        if (status.state === "registered" && permission === "granted") {
            try {
                const registration = await Promise.race([
                    worker,
                    new Promise(resolve => setTimeout(() => resolve(null), 3000))
                ]);
                active = !!(await registration?.pushManager.getSubscription());
            } catch (_) { /* Offer setup again. */ }
        }
        if (sequence !== statusSequence || identity !== document.getElementById("user-selector")?.dataset.userId) return;
        const name = document.querySelector(".identity-name")?.textContent?.replace(/ \| Admin$/, "") || "you";
        statusText.textContent = !supported ? "This browser cannot receive alerts. Check your booking here."
            : status.state === "disabled" ? "Alerts are off on this device."
                : permission === "denied" ? "Alerts are blocked. Change notification permission in your browser settings."
                    : active ? `Promotion alerts are on for ${name} on this device.`
                        : "Enable alerts to hear when you move up. Check your booking here.";
        enable.hidden = active || !supported || permission === "denied";
        test.hidden = !active;
        disable.hidden = status.state === "disabled";
        document.querySelectorAll("[data-waitlist-alert]").forEach(element => {
            element.hidden = active || !supported || permission === "denied" || status.state === "disabled";
        });
    }
    const initialStatus = refreshStatus();

    window.HooplinePush = {
        shouldPrepare() {
            const mayBeAvailable = status ? status.available && status.state !== "disabled" && status.public_key
                : controls?.dataset.available === "true";
            return !!(mayBeAvailable &&
                "Notification" in window && Notification.permission === "granted" &&
                "PushManager" in window && "serviceWorker" in navigator);
        },
        async prepare(explicit = false) {
            if (!status) await initialStatus;
            await refreshStatus();
            if (!status?.available || (!explicit && status.state === "disabled") || !status.public_key) return null;
            const registration = await worker;
            if (!registration) throw new Error("Worker unavailable");
            let subscription = await registration.pushManager.getSubscription();
            if (status?.state === "expired" && subscription) {
                await subscription.unsubscribe();
                subscription = null;
            }
            if (!subscription) {
                const encoded = status.public_key.replace(/-/g, "+").replace(/_/g, "/");
                const binary = atob(encoded + "=".repeat((4 - encoded.length % 4) % 4));
                const key = Uint8Array.from(binary, char => char.charCodeAt(0));
                subscription = await registration.pushManager.subscribe({ userVisibleOnly: true, applicationServerKey: key });
            }
            return JSON.stringify(subscription);
        },
        refreshStatus
    };

    async function write(path, body) {
        const controller = new AbortController();
        const timeout = setTimeout(() => controller.abort(), 10000);
        try {
            const response = await fetch(path, {
                method: "POST",
                credentials: "same-origin",
                headers: {
                    "Content-Type": "application/json",
                    "X-Hoopline-User": controls.dataset.userId
                },
                signal: controller.signal,
                body
            });
            if (!response.ok) throw new Error("Request failed");
        } finally {
            clearTimeout(timeout);
        }
    }

    function bounded(promise) {
        return Promise.race([
            promise,
            new Promise((_, reject) => setTimeout(() => reject(new Error("timeout")), 3000))
        ]);
    }

    if (controls) {
        document.body.addEventListener("click", event => {
            if (event.target.closest("[data-enable-alerts]") && !enable.hidden && !enable.disabled) enable.click();
        });
        document.body.addEventListener("htmx:afterSwap", () => { renderStatus(); });
        document.body.addEventListener("htmx:afterSettle", event => {
            if (event.detail.target?.id !== "slots-content") return;
            const identity = document.getElementById("user-selector")?.dataset.userId;
            if (identity && controls.dataset.userId !== identity) {
                controls.dataset.userId = identity;
                status = null;
            }
            refreshStatus();
        });
        enable.addEventListener("click", () => {
            if (!window.HooplineRequestLock?.acquire()) return;
            // Call requestPermission directly in the user gesture.
            let permission;
            try {
                permission = Notification.permission === "default"
                    ? Notification.requestPermission() : Promise.resolve(Notification.permission);
            } catch (_) {
                statusText.textContent = "Alerts could not be enabled. Try again.";
                window.HooplineRequestLock.release();
                return;
            }
            enable.disabled = true;
            permission.then(async result => {
                if (result !== "granted") { await refreshStatus(); return; }
                const subscription = await bounded(window.HooplinePush.prepare(true));
                await write("/push/subscribe", subscription);
                await refreshStatus();
                window.HooplineRequestLock.refresh();
            }).catch(() => {
                statusText.textContent = "Alerts could not be enabled. Try again.";
            }).finally(() => { enable.disabled = false; window.HooplineRequestLock.release(); });
        });
        disable.addEventListener("click", async () => {
            if (!window.HooplineRequestLock?.acquire()) return;
            disable.disabled = true;
            try { await write("/push/disable"); await refreshStatus(); }
            catch (_) { statusText.textContent = "Alerts could not be turned off. Try again."; }
            finally { disable.disabled = false; window.HooplineRequestLock.release(); }
        });
        test.addEventListener("click", async () => {
            if (!window.HooplineRequestLock?.acquire()) return;
            test.disabled = true;
            try { await write("/push/test"); statusText.textContent = "Test alert requested. Check this device."; }
            catch (_) { statusText.textContent = "Test alert could not be sent. Wait and try again."; }
            finally { test.disabled = false; window.HooplineRequestLock.release(); }
        });
    }

    let installPrompt;
    const button = document.getElementById("install-app");
    window.addEventListener("beforeinstallprompt", event => {
        event.preventDefault();
        installPrompt = event;
        button.hidden = false;
    });
    button.addEventListener("click", async () => {
        if (!installPrompt) return;
        const prompt = installPrompt;
        installPrompt = null;
        button.hidden = true;
        await prompt.prompt();
    });
    window.addEventListener("appinstalled", () => {
        installPrompt = null;
        button.hidden = true;
    });
})();
