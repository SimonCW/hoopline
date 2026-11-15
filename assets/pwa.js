(() => {
    let status = null;
    let statusSequence = 0;
    const controls = document.getElementById("push-controls");
    const statusText = document.getElementById("push-status");
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
        controls.hidden = !identity;
        if (controls.hidden) return;
        const supported = "Notification" in window && "PushManager" in window && "serviceWorker" in navigator;
        const permission = supported ? Notification.permission : "unsupported";
        let active = false;
        if (status?.state === "registered" && permission === "granted") {
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
        statusText.textContent = !status?.available ? "Alerts are unavailable. Check your booking here."
            : !supported ? "This browser cannot receive alerts. Check your booking here."
            : permission === "denied" ? "Alerts are blocked. Change notification permission in your browser settings."
                    : active ? `Promotion alerts are on for ${name} on this device.`
                        : "Alerts are not active on this device. Check your booking here.";
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
