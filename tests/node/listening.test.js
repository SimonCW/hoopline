import { test } from 'node:test';
import assert from 'node:assert/strict';
import { PassThrough } from 'node:stream';
import { onListening } from '../scenarios/listening.js';

test('listener announcements survive a chunk split inside the port', () => {
  const stream = new PassThrough();
  const ports = [];
  const lines = onListening(stream, port => ports.push(port));
  try {
    stream.write('unrelated output\nHOOPLINE_LISTENING=0.0.0.0:61');
    assert.deepEqual(ports, []);
    stream.write('234');
    assert.deepEqual(ports, []);
    stream.write('\n');
    assert.deepEqual(ports, [61234]);
  } finally {
    lines.close();
    stream.destroy();
  }
});
