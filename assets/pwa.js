(() => {
    let status = null;
    let statusSequence = 0;
    const controls = document.getElementById("push-controls");
    const alertSetup = document.getElementById("alert-setup");
    const enableAlerts = document.getElementById("enable-alerts");
    const alertMessage = document.getElementById("alert-setup-status");
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
            if (alertSetup) {
                alertSetup.hidden = !(status?.available && status.public_key && status.state !== "registered" &&
                    status.state !== "disabled" && "Notification" in window && "PushManager" in window &&
                    "serviceWorker" in navigator);
                if (!alertSetup.hidden) {
                    const denied = Notification.permission === "denied";
                    enableAlerts.hidden = denied;
                    alertMessage.textContent = denied
                        ? "Alerts are blocked. Allow notifications in your browser or device settings." : "";
                }
            }
        } catch (_) {
            if (sequence !== statusSequence) return;
            status = null;
        } finally {
            clearTimeout(timeout);
        }
    }
    const initialStatus = refreshStatus();

    window.HooplinePush = {
        shouldPrepare(waitlist = false) {
            const mayBeAvailable = status ? status.available && status.public_key
                : controls?.dataset.available === "true";
            return !!(mayBeAvailable &&
                "Notification" in window && (Notification.permission === "granted" || (waitlist && Notification.permission === "default")) &&
                "PushManager" in window && "serviceWorker" in navigator);
        },
        requestPermission() {
            return Notification.permission === "default"
                ? Notification.requestPermission() : Promise.resolve(Notification.permission);
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

    if (controls) {
        document.body.addEventListener("htmx:afterSettle", event => {
            if (event.detail.target?.id !== "slots-content") return;
            const identity = document.getElementById("user-selector")?.dataset.userId;
            if (identity && controls.dataset.userId !== identity) {
                controls.dataset.userId = identity;
                status = null;
            }
            refreshStatus();
        });
    }

    enableAlerts?.addEventListener("click", async () => {
        if (!window.HooplineRequestLock?.acquire()) return;
        const identity = controls.dataset.userId;
        const controller = new AbortController();
        let expired = false;
        let timer;
        alertMessage.textContent = "Setting up alerts…";
        try {
            const setup = async () => {
                if (await window.HooplinePush.requestPermission() !== "granted" || expired) {
                    throw new Error("Permission not granted");
                }
                const subscription = await window.HooplinePush.prepare(true);
                if (expired || !subscription || identity !== controls.dataset.userId) throw new Error("Setup unavailable");
                const response = await fetch("/push/subscribe", {
                    method: "POST",
                    credentials: "same-origin",
                    headers: { "Content-Type": "application/json", "X-Hoopline-User": identity },
                    body: subscription,
                    signal: controller.signal
                });
                const result = response.ok ? await response.json() : null;
                if (expired || identity !== controls.dataset.userId || result?.state !== "registered") throw new Error("Registration failed");
                return result;
            };
            const timeout = new Promise((_, reject) => {
                timer = setTimeout(() => {
                    expired = true;
                    controller.abort();
                    reject(new Error("Setup timed out"));
                }, 10000);
            });
            status = await Promise.race([setup(), timeout]);
            enableAlerts.hidden = true;
            alertMessage.textContent = "Promotion alerts are on for this device.";
        } catch (_) {
            alertMessage.textContent = Notification.permission === "denied"
                ? "Alerts are blocked. Allow notifications in your browser or device settings."
                : "Alerts could not be enabled. Try again.";
            if (Notification.permission === "denied") enableAlerts.hidden = true;
        } finally {
            clearTimeout(timer);
            window.HooplineRequestLock.release();
        }
    });

    let installPrompt;
    const buttons = document.querySelectorAll("[data-install-app]");
    const guidance = document.getElementById("install-guidance");
    const standalone = window.matchMedia?.("(display-mode: standalone)").matches || navigator.standalone === true;
    let dismissed = false;
    try { dismissed = sessionStorage.getItem("hoopline-install-dismissed") === "true"; } catch (_) {}
    if (guidance && !standalone && !dismissed) guidance.hidden = false;
    document.getElementById("dismiss-install")?.addEventListener("click", () => {
        guidance.hidden = true;
        try { sessionStorage.setItem("hoopline-install-dismissed", "true"); } catch (_) {}
    });
    window.addEventListener("beforeinstallprompt", event => {
        if (standalone) return;
        event.preventDefault();
        installPrompt = event;
        buttons.forEach(button => { button.hidden = false; });
    });
    buttons.forEach(button => button.addEventListener("click", async () => {
        if (!installPrompt) return;
        const prompt = installPrompt;
        installPrompt = null;
        buttons.forEach(item => { item.hidden = true; });
        await prompt.prompt();
    }));
    window.addEventListener("appinstalled", () => {
        installPrompt = null;
        buttons.forEach(button => { button.hidden = true; });
        if (guidance) guidance.hidden = true;
    });
})();
