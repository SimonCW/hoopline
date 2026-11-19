/* Keep bookings server-confirmed. Never show success before the response arrives. */
(() => {
    const body = document.body;
    const messages = body.dataset;
    const announcement = document.getElementById("announcements");
    let pending = false;
    let refreshQueued = false;
    let focusAfterSwap = false;
    let leaving = false;
    let pageGeneration = 0;
    const openRosters = new Set();
    const waitlistEnrollments = new WeakMap();
    const languageChanged = locale => (locale === "en" || locale === "de") && locale !== document.documentElement.lang;

    const languageSelect = document.getElementById("language-select");
    const languageForm = languageSelect?.form;
    if (languageForm?.requestSubmit) {
        languageSelect.addEventListener("change", () => languageForm.requestSubmit());
        languageForm.querySelector('button[type="submit"]').hidden = true;
    }
    languageForm?.addEventListener("submit", event => {
        const { pathname, search, hash } = window.location;
        if (pathname === "/slots" && /^#slot-[0-9]+$/.test(hash)) {
            event.currentTarget.elements.return_to.value = pathname + search + hash;
        }
    });

    function disableBookingControls() {
        document.querySelectorAll('#slots-content button:not([data-enable-alerts]), #user-selector form button, #refresh-slots, .admin-editor button, #language-select, .language-picker button').forEach(control => {
            // Only restore controls disabled by this request, not disabled slot states.
            if (!control.disabled) {
                control.dataset.requestDisabled = "";
                control.disabled = true;
            }
        });
    }

    function finishRequest() {
        pending = false;
        document.querySelectorAll('[data-request-disabled]').forEach(control => {
            control.disabled = false;
            delete control.dataset.requestDisabled;
        });
        document.querySelectorAll('[aria-busy="true"]').forEach(el => el.removeAttribute("aria-busy"));
        document.getElementById("booking-request-status").hidden = true;
    }

    function announce(message) {
        announcement.textContent = "";
        requestAnimationFrame(() => { announcement.textContent = message; });
    }

    function showError(element, message) {
        const scope = element?.closest?.(".slot-card, #name-step, #user-selector, .admin-editor");
        const error = scope?.querySelector(".request-error") || document.getElementById("global-error-banner");
        error.textContent = message;
        error.hidden = false;
        error.focus({ preventScroll: false });
    }

    function refresh() {
        if (pending) { refreshQueued = true; return; }
        if (!window.htmx || !document.getElementById("slots-content")) return;
        htmx.trigger(body, "refresh-slots");
    }

    if (!window.htmx) {
        showError(null, messages.controlsFailed);
        document.querySelectorAll("form:not([action]) button").forEach(button => { button.disabled = true; });
        return;
    }

    window.addEventListener("pagehide", () => { leaving = true; pageGeneration += 1; });
    body.addEventListener("htmx:confirm", event => {
        if (pending) {
            event.preventDefault();
            return;
        }
        const form = event.detail.elt;
        const waitlist = form.matches?.("form[data-join-waitlist]");
        if (!form.matches?.("form[data-signup]")) return;
        const push = window.HooplinePush;
        if (!push?.shouldPrepare()) {
            if (waitlist) {
                const onWaitlistSaved = push?.promptOnJoin();
                if (onWaitlistSaved) waitlistEnrollments.set(form, onWaitlistSaved);
            }
            return;
        }
        event.preventDefault();
        pending = true;
        const generation = pageGeneration;
        form.querySelectorAll("[data-push-preparation]").forEach(input => input.remove());
        disableBookingControls();
        const status = document.getElementById("booking-request-status");
        status.textContent = messages.preparingBooking;
        status.hidden = false;
        let timer;
        const prepared = push.prepare().catch(() => null);
        const timeout = new Promise(resolve => { timer = setTimeout(() => resolve(undefined), 1000); });
        const enrollAfterSignup = push.enrollAfterSignup(prepared);
        Promise.race([prepared, timeout]).then(subscription => {
            if (leaving || generation !== pageGeneration || !form.isConnected) return;
            if (subscription === undefined) waitlistEnrollments.set(form, enrollAfterSignup);
            if (!subscription) return;
            const input = document.createElement("input");
            input.type = "hidden";
            input.name = "push_subscription";
            input.dataset.pushPreparation = "";
            input.value = subscription;
            form.append(input);
        }).finally(() => {
            clearTimeout(timer);
            if (leaving || generation !== pageGeneration || !form.isConnected) {
                finishRequest();
                if (refreshQueued) {
                    refreshQueued = false;
                    if (!leaving) requestAnimationFrame(refresh);
                }
                return;
            }
            event.detail.issueRequest();
        });
    });

    body.addEventListener("click", (event) => {
        if (event.target.closest("#refresh-slots")) {
            focusAfterSwap = true;
            refresh();
        }
    });
    body.addEventListener("htmx:beforeRequest", (event) => {
        if (event.detail.elt.closest('.admin-editor')?.querySelector('[name="bookings"]:checked')?.value === 'clear' &&
            !window.confirm(event.detail.elt.dataset.confirmClear)) {
            event.preventDefault();
            return;
        }
        pending = true;
        const onWaitlistSaved = waitlistEnrollments.get(event.detail.elt);
        if (onWaitlistSaved) {
            waitlistEnrollments.delete(event.detail.elt);
            waitlistEnrollments.set(event.detail.xhr, onWaitlistSaved);
        }
        disableBookingControls();
        const status = document.getElementById("booking-request-status");
        status.textContent = document.getElementById("name-step") ? messages.openingGames
            : event.detail.requestConfig.verb === "get" ? messages.refreshingGames
                : messages.savingChanges;
        status.hidden = false;
        const scope = event.detail.elt.closest(".slot-card, #name-step, #user-selector, .admin-editor") || event.detail.elt;
        scope.setAttribute("aria-busy", "true");
        scope.querySelectorAll(".request-error").forEach(error => { error.hidden = true; });
        scope.querySelectorAll("[data-feedback]").forEach(message => { message.hidden = true; });
        document.getElementById("global-error-banner").hidden = true;
        openRosters.clear();
        document.querySelectorAll(".roster[open]").forEach(roster => openRosters.add(roster.id));
    });
    body.addEventListener("htmx:afterRequest", (event) => {
        event.detail.elt.querySelectorAll?.("[data-push-preparation]").forEach(input => input.remove());
        // Successful outerHTML swaps still need to settle. A queued request on
        // the old target would otherwise disappear when that node is removed.
        if (!event.detail.successful) {
            finishRequest();
            refreshQueued = false;
        }
    });
    body.addEventListener("htmx:afterSwap", () => {
        // Replaced cards and out-of-band headers contain fresh, enabled buttons.
        // Keep them locked until the successful request has fully settled.
        if (pending) disableBookingControls();
        // Restore expanded content before the browser paints the new HTML.
        // Waiting for afterSettle would briefly collapse every open roster.
        openRosters.forEach(id => {
            const roster = document.getElementById(id);
            if (roster) roster.open = true;
        });
    });
    body.addEventListener("htmx:beforeOnLoad", event => {
        if (languageChanged(event.detail.xhr.getResponseHeader("Content-Language"))) {
            // Another tab changed the shared cookie. Do not mix languages in this page.
            event.preventDefault();
            window.location.reload();
        }
    });
    body.addEventListener("htmx:responseError", (event) => {
        const doc = new DOMParser().parseFromString(event.detail.xhr.responseText, "text/html");
        showError(event.detail.elt, doc.querySelector("main p")?.textContent?.trim() || messages.requestFallback);
    });
    ["htmx:sendError", "htmx:timeout"].forEach(type => body.addEventListener(type, event => {
        showError(event.detail.elt, messages.connectionFailed);
    }));
    body.addEventListener("htmx:afterSettle", (event) => {
        // Out-of-band identity updates also emit this event for the same request.
        if (event.detail.elt.id !== event.detail.target.id) return;
        finishRequest();
        // outerHTML swaps replace the original target node.
        const target = document.getElementById(event.detail.target.id) || event.detail.target;
        const onWaitlistSaved = waitlistEnrollments.get(event.detail.xhr);
        if (onWaitlistSaved) {
            waitlistEnrollments.delete(event.detail.xhr);
            if (!leaving && target.querySelector("[data-waitlist-alerts]")) onWaitlistSaved(target.id);
        }
        const feedback = target.querySelector?.("[data-feedback]");
        if (feedback) {
            announce(feedback.textContent);
            target.querySelector("h3")?.focus({ preventScroll: true });
            window.HooplinePush?.refreshStatus();
        }
        if (target.id === "slots-content" && focusAfterSwap) {
            document.getElementById("refresh-slots")?.focus({ preventScroll: true });
            focusAfterSwap = false;
            announce(messages.refreshed);
        }
        if (refreshQueued) {
            refreshQueued = false;
            requestAnimationFrame(refresh);
        }
    });
    body.addEventListener("toggle", event => {
        if (event.target.matches?.(".roster")) {
            if (event.target.open) openRosters.add(event.target.id);
            else openRosters.delete(event.target.id);
        }
    }, true);
    window.addEventListener("focus", refresh);
    document.addEventListener("visibilitychange", () => { if (!document.hidden) refresh(); });
    window.addEventListener("pageshow", event => {
        leaving = false;
        if (!event.persisted) return;
        if (document.getElementById("slots-content")) {
            refresh();
        } else {
            fetch(window.location.href, { method: "HEAD", cache: "no-store" })
                .then(response => {
                    if (!leaving && languageChanged(response.headers.get("Content-Language"))) window.location.reload();
                })
                .catch(() => {});
        }
    });
})();
const themeSelect = document.getElementById('theme-select');
if (themeSelect) {
    themeSelect.value = document.documentElement.dataset.theme || 'light';
    themeSelect.addEventListener('change', () => {
        document.documentElement.dataset.theme = themeSelect.value;
        try { localStorage.setItem('hoopline-theme', themeSelect.value); } catch (_) { /* Keep the selection for this page. */ }
    });
}

document.querySelectorAll('[data-theme-award]').forEach(notice => {
    const key = `hoopline-theme-award:${notice.dataset.themeAward}`;
    try { notice.hidden = localStorage.getItem(key) === 'dismissed'; } catch (_) { /* Keep the notice visible. */ }
    const dismiss = notice.querySelector('[data-dismiss-theme-award]');
    dismiss.hidden = false;
    dismiss.addEventListener('click', () => {
        try { localStorage.setItem(key, 'dismissed'); } catch (_) { /* Dismiss for this page only. */ }
        notice.hidden = true;
        const next = document.querySelector('[data-theme-award]:not([hidden]) [data-dismiss-theme-award]');
        (next || document.getElementById('main-content')).focus({ preventScroll: true });
    });
});
