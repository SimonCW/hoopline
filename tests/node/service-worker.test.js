import { readFile } from "node:fs/promises";
import { runInNewContext } from "node:vm";
import { test } from "node:test";
import assert from "node:assert/strict";

async function worker() {
    const handlers = {};
    const notifications = [];
    const opened = [];
    const self = {
        location: { origin: "https://hoopline.example" },
        addEventListener: (name, handler) => { handlers[name] = handler; },
        registration: { showNotification: async (title, options) => notifications.push({ title, options }) },
        clients: { matchAll: async () => [], openWindow: async url => opened.push(url) }
    };
    runInNewContext(await readFile(new URL("../../assets/sw.js", import.meta.url), "utf8"), { self, URL });
    return { handlers, notifications, opened };
}

test("worker shows a safe fallback for malformed pushes", async () => {
    const { handlers, notifications } = await worker();
    let result;
    handlers.push({ data: { json() { throw new Error("invalid"); } }, waitUntil(promise) { result = promise; } });
    await result;
    assert.equal(notifications[0].title, "Hoopline");
    assert.equal(notifications[0].options.data.url, "/slots");
});

test("worker uses stable promotion tags and rejects external destinations", async () => {
    const { handlers, notifications, opened } = await worker();
    let result;
    handlers.push({
        data: { json: () => ({ title: "Playing", body: "Check your booking", tag: "promotion:42", url: "https://example.com/" }) },
        waitUntil(promise) { result = promise; }
    });
    await result;
    assert.equal(notifications[0].options.tag, "promotion:42");
    assert.equal(notifications[0].options.data.url, "/slots");
    let closed = false;
    handlers.notificationclick({
        notification: { data: { url: "//example.com/" }, close() { closed = true; } },
        waitUntil(promise) { result = promise; }
    });
    await result;
    assert.equal(closed, true);
    assert.deepEqual(opened, ["/slots"]);
});

test("worker opens a valid roster destination", async () => {
    const { handlers, opened } = await worker();
    let result;
    handlers.notificationclick({
        notification: { data: { url: "/slots#slot-123" }, close() {} },
        waitUntil(promise) { result = promise; }
    });
    await result;
    assert.deepEqual(opened, ["/slots#slot-123"]);
});
