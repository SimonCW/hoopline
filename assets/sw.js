// Online only: do not intercept requests or cache roster data.
self.addEventListener("push", event => {
    let payload = {};
    try {
        const data = event.data?.json();
        if (data && typeof data === "object") payload = data;
    } catch (_) {
        // A malformed push must still produce a visible notification.
    }
    const title = typeof payload.title === "string" && payload.title.length <= 100
        ? payload.title : "Hoopline";
    const body = typeof payload.body === "string" && payload.body.length <= 300
        ? payload.body : "Open Hoopline to check your booking.";
    const url = typeof payload.url === "string" && /^\/slots#slot-[0-9]+$/.test(payload.url)
        ? payload.url : "/slots";
    const tag = typeof payload.tag === "string" && /^promotion:[0-9]+$/.test(payload.tag)
        ? payload.tag : undefined;
    event.waitUntil(self.registration.showNotification(title, {
        body, tag, icon: "/assets/icon-192.png", data: { url }
    }));
});

self.addEventListener("notificationclick", event => {
    event.notification.close();
    const candidate = event.notification.data?.url;
    const path = typeof candidate === "string" && /^\/slots#slot-[0-9]+$/.test(candidate)
        ? candidate : "/slots";
    event.waitUntil((async () => {
        const windows = await self.clients.matchAll({ type: "window", includeUncontrolled: true });
        const window = windows.find(client => new URL(client.url).origin === self.location.origin);
        if (window) {
            await window.navigate(path);
            await window.focus();
        } else {
            await self.clients.openWindow(path);
        }
    })());
});
