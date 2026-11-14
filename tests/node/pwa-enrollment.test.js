import { readFile } from "node:fs/promises";
import { runInNewContext } from "node:vm";
import { test } from "node:test";
import assert from "node:assert/strict";

async function page({ state = "unregistered", permission = "granted", existing = null, readyGate = null } = {}) {
    const calls = [];
    const subscription = existing || { toJSON: () => ({ endpoint: "test" }) };
    const registration = {
        pushManager: {
            getSubscription: async () => existing,
            subscribe: async options => { calls.push(options); return subscription; }
        }
    };
    const button = { hidden: true, addEventListener() {} };
    const window = { addEventListener() {}, PushManager: class {}, Notification: {} };
    const context = {
        window,
        navigator: { serviceWorker: { register: async () => registration, ready: readyGate ? readyGate.then(() => registration) : Promise.resolve(registration) } },
        Notification: { permission },
        document: { getElementById: id => id === "install-app" ? button : null },
        fetch: async () => ({ ok: true, json: async () => ({ available: true, state, public_key: "AQID" }) }),
        atob, Uint8Array, AbortController, setTimeout, clearTimeout
    };
    runInNewContext(await readFile(new URL("../../assets/pwa.js", import.meta.url), "utf8"), context);
    await window.HooplinePush.refreshStatus();
    return { push: window.HooplinePush, calls, subscription };
}

test("granted permission creates a subscription without a second gesture", async () => {
    const { push, calls } = await page();
    assert.equal(push.shouldPrepare(), true);
    assert.equal(await push.prepare(), '{"endpoint":"test"}');
    assert.equal(calls.length, 1);
    assert.equal(calls[0].userVisibleOnly, true);
});

test("existing subscription is reused", async () => {
    const existing = { toJSON: () => ({ endpoint: "existing" }) };
    const { push, calls } = await page({ existing });
    assert.equal(await push.prepare(), '{"endpoint":"existing"}');
    assert.equal(calls.length, 0);
});

test("explicit off and default permission do not prepare subscriptions", async () => {
    const disabled = await page({ state: "disabled" });
    assert.equal(disabled.push.shouldPrepare(), false);
    assert.equal(await disabled.push.prepare(), null);
    assert.equal(await disabled.push.prepare(true), '{"endpoint":"test"}');
    assert.equal((await page({ permission: "default" })).push.shouldPrepare(), false);
});

test("expired registration replaces the stale subscription", async () => {
    let unsubscribed = false;
    const existing = { unsubscribe: async () => { unsubscribed = true; } };
    const { push, calls } = await page({ state: "expired", existing });
    await push.prepare();
    assert.equal(unsubscribed, true);
    assert.equal(calls.length, 1);
});

test("subscription waits for worker activation", async () => {
    let activate;
    const readyGate = new Promise(resolve => { activate = resolve; });
    const { push, calls } = await page({ readyGate });
    const pending = push.prepare();
    await Promise.resolve();
    assert.equal(calls.length, 0);
    activate();
    await pending;
    assert.equal(calls.length, 1);
});
