(() => {
    const controls = document.getElementById("push-controls");
    const worker = "serviceWorker" in navigator
        ? navigator.serviceWorker.register("/sw.js")
            .then(() => navigator.serviceWorker.ready).catch(() => null) : Promise.resolve(null);
    const standalone = window.matchMedia?.("(display-mode: standalone)").matches || navigator.standalone === true;
    const appleMobile = /iPhone|iPad|iPod/.test(navigator.userAgent) ||
        (/Macintosh/.test(navigator.userAgent) && navigator.maxTouchPoints > 1);

    function setupInstall() {
        const buttons = document.querySelectorAll("[data-install-app]");
        if (standalone || !buttons.length) return;

        const banner = document.querySelector("[data-install-banner]");
        const mobile = appleMobile || navigator.userAgentData?.mobile || /Android|Mobi/.test(navigator.userAgent);
        const dismissKey = "hoopline-install-banner-dismissed";
        let dismissed = false;
        try { dismissed = sessionStorage.getItem(dismissKey) === "true"; } catch (_) {}
        const dismiss = banner?.querySelector("[data-install-dismiss]");
        const manual = banner?.querySelector("[data-install-manual]");
        let installPrompt;
        let installed = false;

        function renderInstall() {
            buttons.forEach(button => { button.hidden = !installPrompt; });
            if (manual) manual.hidden = appleMobile || Boolean(installPrompt);
        }

        function hideBanner() {
            if (!banner) return;
            const hadFocus = banner.contains(document.activeElement);
            banner.hidden = true;
            if (hadFocus) document.getElementById("welcome-title").focus();
        }

        if (banner && mobile && !dismissed) {
            banner.querySelector("[data-install-apple]").hidden = !appleMobile;
            banner.hidden = false;
            dismiss.onclick = () => {
                try { sessionStorage.setItem(dismissKey, "true"); } catch (_) {}
                hideBanner();
            };
        }
        renderInstall();

        window.addEventListener("beforeinstallprompt", event => {
            if (appleMobile || installed) return;
            event.preventDefault();
            installPrompt = event;
            renderInstall();
        });
        buttons.forEach(button => {
            button.onclick = async () => {
                if (!installPrompt) return;
                const prompt = installPrompt;
                installPrompt = null;
                try { await prompt.prompt(); } catch (_) {
                    // The browser menu still offers a way to install.
                }
                if (!installed && button === document.activeElement && banner?.contains(button)) dismiss.focus();
                renderInstall();
            };
        });
        window.addEventListener("appinstalled", () => {
            installed = true;
            installPrompt = null;
            hideBanner();
            renderInstall();
        });
    }

    setupInstall();
    if (!controls) return;
    const messages = controls.dataset;

    function renderedStatus() {
        const state = document.getElementById("slots-content")?.dataset.pushState || "unavailable";
        return { available: state !== "unavailable", state };
    }
    let status = renderedStatus();
    let statusSequence = 0;
    let prompted = false;
    let permissionRequest;
    let settingUp = false;
    const promptKey = "hoopline-waitlist-permission-requested";
    try { prompted = sessionStorage.getItem(promptKey) === "true"; } catch (_) {}
    const supported = "Notification" in window && "PushManager" in window && "serviceWorker" in navigator;
    const permission = () => supported ? Notification.permission : "unsupported";
    const available = () => status?.available && status.state !== "unavailable" && status.state !== "stale";
    const canPrompt = () => available() && status.state !== "disabled" && permission() === "default" && !prompted;

    function render() {
        document.querySelectorAll("[data-alert-hint]").forEach(hint => { hint.hidden = !canPrompt(); });
        const on = status?.state === "registered" && permission() === "granted";
        const canEnable = available() && supported && permission() !== "denied";
        const help = status?.state === "stale" ? messages.stale
            : !available() ? messages.unavailable
            : !supported && appleMobile && !standalone
                ? messages.install
            : !supported ? messages.unsupported
            : permission() === "denied" ? messages.blocked
            : messages.help;
        document.querySelectorAll("[data-waitlist-alerts]").forEach(details => {
            if (on && details.contains(document.activeElement)) details.closest(".slot-card").querySelector("h3").focus({ preventScroll: true });
            details.hidden = on;
            details.querySelector("[data-alert-help]").textContent = help;
            details.querySelector("[data-enable-alerts]").hidden = !canEnable;
        });
    }

    async function refreshStatus() {
        const sequence = ++statusSequence;
        const identity = controls.dataset.userId;
        const controller = new AbortController();
        const timeout = setTimeout(() => controller.abort(), 3000);
        let next = null;
        try {
            const response = await fetch("/push/status", { credentials: "same-origin", cache: "no-store", signal: controller.signal });
            if (response.ok) next = await response.json();
        } catch (_) {
            // Booking still works without alerts.
        } finally {
            clearTimeout(timeout);
        }
        if (sequence === statusSequence && identity === controls.dataset.userId) {
            if (next) status = next;
            render();
        }
        return identity === controls.dataset.userId ? next : null;
    }

    function requestPermission() {
        if (permission() !== "default") return Promise.resolve(permission());
        if (!permissionRequest) {
            try {
                permissionRequest = Promise.resolve(Notification.requestPermission())
                    .catch(() => "default").finally(() => { permissionRequest = null; });
            } catch (_) {
                return Promise.resolve("default");
            }
        }
        return permissionRequest;
    }

    async function prepare(explicit = false) {
        const current = await refreshStatus();
        if (!current?.available || !current.public_key || current.state === "unavailable" || current.state === "stale" ||
            (!explicit && current.state === "disabled")) return null;
        const registration = await worker;
        if (!registration) return null;
        let subscription = await registration.pushManager.getSubscription();
        if (current.state === "expired" && subscription) {
            await subscription.unsubscribe();
            subscription = null;
        }
        if (!subscription) {
            const encoded = current.public_key.replace(/-/g, "+").replace(/_/g, "/");
            const binary = atob(encoded + "=".repeat((4 - encoded.length % 4) % 4));
            const key = Uint8Array.from(binary, char => char.charCodeAt(0));
            subscription = await registration.pushManager.subscribe({ userVisibleOnly: true, applicationServerKey: key });
        }
        return JSON.stringify(subscription);
    }

    const currentAlerts = cardId => document.getElementById(cardId)?.querySelector("[data-waitlist-alerts]");

    async function enroll(identity, cardId, explicit, prepared) {
        const stillWaiting = () => identity === controls.dataset.userId && currentAlerts(cardId);
        if (!stillWaiting()) return false;
        const controller = new AbortController();
        let timer;
        const timeout = new Promise(resolve => {
            timer = setTimeout(() => {
                controller.abort();
                resolve(false);
            }, 10000);
        });
        const setup = async () => {
            const subscription = await (prepared ?? prepare(explicit));
            if (!subscription || !stillWaiting() || controller.signal.aborted) return false;
            const response = await fetch(explicit ? "/push/subscribe" : "/push/subscribe?automatic=true", {
                method: "POST",
                credentials: "same-origin",
                headers: { "Content-Type": "application/json", "X-Hoopline-User": identity },
                body: subscription,
                signal: controller.signal
            });
            return response.ok;
        };
        try {
            return await Promise.race([setup(), timeout]);
        } catch (_) {
            return false;
        } finally {
            clearTimeout(timer);
        }
    }

    function enrollAfterSignup(prepared, answer) {
        const identity = controls.dataset.userId;
        return async cardId => {
            if (!answer || await answer === "granted") await enroll(identity, cardId, false, prepared);
            refreshStatus();
        };
    }

    window.HooplinePush = {
        shouldPrepare() {
            return !!(available() && status.state !== "disabled" && permission() === "granted");
        },
        promptOnJoin() {
            if (!canPrompt()) return;
            prompted = true;
            try { sessionStorage.setItem(promptKey, "true"); } catch (_) {}
            const answer = requestPermission();
            render();
            return enrollAfterSignup(null, answer);
        },
        enrollAfterSignup,
        prepare,
        refreshStatus
    };

    render();
    refreshStatus();
    let openAlerts = [];
    let focusedCard;
    let focusedControl;
    document.body.addEventListener("htmx:beforeSwap", event => {
        if (event.detail.target?.id !== "slots-content" || !event.detail.shouldSwap) return;
        openAlerts = Array.from(document.querySelectorAll("[data-waitlist-alerts][open]"),
            details => details.closest(".slot-card").id);
        const active = document.activeElement;
        focusedCard = active?.closest("[data-waitlist-alerts]")?.closest(".slot-card").id;
        focusedControl = active?.matches("[data-enable-alerts]") ? "[data-enable-alerts]" : "summary";
    });
    document.body.addEventListener("htmx:afterSettle", event => {
        if (event.detail.elt.id !== "slots-content" || event.detail.target?.id !== "slots-content") return;
        const identity = document.getElementById("user-selector")?.dataset.userId;
        status = renderedStatus();
        render();
        if (identity === controls.dataset.userId) {
            for (const cardId of openAlerts) {
                const details = currentAlerts(cardId);
                if (details && !details.hidden) details.open = true;
            }
            const details = focusedCard && currentAlerts(focusedCard);
            if (details && !details.hidden) {
                const control = details.querySelector(focusedControl);
                (control.hidden ? details.querySelector("summary") : control).focus({ preventScroll: true });
            }
        }
        if (identity) controls.dataset.userId = identity;
        openAlerts = [];
        focusedCard = null;
        refreshStatus();
    });

    document.body.addEventListener("click", async event => {
        const button = event.target.closest("[data-enable-alerts]");
        if (!button || settingUp) return;
        const identity = controls.dataset.userId;
        const cardId = button.closest(".slot-card").id;
        const granted = await requestPermission() === "granted";
        if (settingUp || identity !== controls.dataset.userId) return;
        const feedback = message => {
            const node = currentAlerts(cardId)?.querySelector("[data-alert-feedback]");
            if (node && identity === controls.dataset.userId) node.textContent = message;
        };
        if (!granted) {
            feedback("");
            render();
            return;
        }
        settingUp = true;
        feedback(messages.settingUp);
        const enabled = await enroll(identity, cardId, true);
        if (enabled && identity === controls.dataset.userId) {
            document.getElementById("announcements").textContent = messages.on;
        } else {
            feedback(messages.failed);
        }
        settingUp = false;
        refreshStatus();
    });
})();
