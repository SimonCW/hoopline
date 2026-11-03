/* Keep bookings server-confirmed. Never show success before the response arrives. */
(() => {
    const body = document.body;
    const announcement = document.getElementById("announcements");
    let pending = false;
    let refreshQueued = false;
    let focusAfterSwap = false;
    const openRosters = new Set();

    function announce(message) {
        announcement.textContent = "";
        requestAnimationFrame(() => { announcement.textContent = message; });
    }

    function showError(element, message) {
        const scope = element?.closest?.(".slot-card, #name-step, #user-selector");
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
        showError(null, "Booking controls could not load. Check your connection and reload this page.");
        document.querySelectorAll("form:not([action]) button").forEach(button => { button.disabled = true; });
        return;
    }

    body.addEventListener("click", (event) => {
        if (event.target.closest("#refresh-slots")) {
            focusAfterSwap = true;
            refresh();
        }
    });
    body.addEventListener("htmx:beforeRequest", (event) => {
        pending = true;
        const scope = event.detail.elt.closest(".slot-card, #name-step, #user-selector") || event.detail.elt;
        scope.setAttribute("aria-busy", "true");
        scope.querySelectorAll(".request-error").forEach(error => { error.hidden = true; });
        scope.querySelectorAll("[data-feedback]").forEach(message => { message.hidden = true; });
        document.getElementById("global-error-banner").hidden = true;
        document.querySelectorAll(".roster[open]").forEach(roster => openRosters.add(roster.id));
    });
    body.addEventListener("htmx:afterRequest", (event) => {
        document.querySelectorAll('[aria-busy="true"]').forEach(el => el.removeAttribute("aria-busy"));
        // Successful outerHTML swaps still need to settle. A queued request on
        // the old target would otherwise disappear when that node is removed.
        if (!event.detail.successful) {
            pending = false;
            refreshQueued = false;
        }
    });
    body.addEventListener("htmx:responseError", (event) => {
        const doc = new DOMParser().parseFromString(event.detail.xhr.responseText, "text/html");
        showError(event.detail.elt, doc.querySelector("main p")?.textContent?.trim() || "Request failed. Refresh and check your booking before trying again.");
    });
    ["htmx:sendError", "htmx:timeout"].forEach(type => body.addEventListener(type, event => {
        showError(event.detail.elt, "Connection failed. We could not confirm your booking. Refresh and check your status before trying again.");
    }));
    body.addEventListener("htmx:afterSettle", (event) => {
        // Out-of-band identity updates also emit this event for the same request.
        if (event.detail.elt.id !== event.detail.target.id) return;
        openRosters.forEach(id => {
            const roster = document.getElementById(id);
            if (roster) roster.open = true;
        });
        // outerHTML swaps replace the original target node.
        const target = document.getElementById(event.detail.target.id) || event.detail.target;
        const feedback = target.querySelector?.("[data-feedback]");
        if (feedback) {
            announce(feedback.textContent);
            target.querySelector("h3")?.focus({ preventScroll: true });
        }
        if (target.id === "slots-content" && focusAfterSwap) {
            document.getElementById("refresh-slots")?.focus({ preventScroll: true });
            focusAfterSwap = false;
            announce("Upcoming slots refreshed.");
        }
        pending = false;
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
    window.addEventListener("pageshow", event => { if (event.persisted) refresh(); });
})();
